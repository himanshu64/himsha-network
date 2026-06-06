//! Quorum-based leader election (Raft-style safety) for partition-safe failover.
//!
//! The earlier crash-only scheme (promote if no higher-priority peer answers) is not
//! partition-safe: a network split could let two standbys each see "no higher peer"
//! and both promote → split-brain. This module fixes that with the Raft election
//! *safety* rule:
//!
//!   - monotonic **terms**; each node grants **at most one vote per term**;
//!   - a candidate may promote only after collecting votes from a **majority** of the
//!     configured member set.
//!
//! By quorum intersection, at most one candidate can reach a majority in any term, so
//! there is **at most one leader per term** — even under partition (a minority side can
//! never reach majority, so it can never elect). Block/state replication is already
//! handled by ZK-verifying followers, so we only need the election half here.
//!
//! Liveness is handled by the follower loop: a successful block poll acts as the
//! leader's heartbeat (resets the failover counter), a candidate first re-points to any
//! already-elected leader (`getLeader`) instead of electing, randomized candidacy jitter
//! avoids split votes, and a higher term triggers automatic step-down. The last piece —
//! preventing a partitioned node from **inflating its term** and later disrupting a live
//! leader on rejoin — is the Raft **PreVote** phase ([`ElectionState::consider_pre_vote`]):
//! a candidate must win a non-binding pre-vote quorum *before* bumping its term.

use serde::{Deserialize, Serialize};

/// Per-node election state (term, per-term vote, and current-leader view).
#[derive(Clone, Debug, Default)]
pub struct ElectionState {
    pub current_term: u64,
    pub voted_for: Option<String>,
    /// True if this node currently believes it is the leader for `current_term`.
    pub is_leader: bool,
    /// Best-known leader URL for `current_term` (this node if `is_leader`).
    pub leader: Option<String>,
}

/// A vote response (Raft RequestVote reply).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoteReply {
    pub term: u64,
    pub granted: bool,
}

/// This node's view of the current leader (Raft heartbeat / discovery).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LeaderInfo {
    pub term: u64,
    pub leader: Option<String>,
}

impl ElectionState {
    /// Apply the Raft vote rule for a `RequestVote(term, candidate)`.
    /// Grants iff the term is current-or-newer and we haven't already voted for a
    /// *different* candidate this term. Newer terms reset the per-term vote.
    pub fn consider_vote(&mut self, term: u64, candidate: &str) -> VoteReply {
        if term < self.current_term {
            return VoteReply { term: self.current_term, granted: false };
        }
        if term > self.current_term {
            self.current_term = term;
            self.voted_for = None;
            self.is_leader = false; // step down for the newer term
            self.leader = None;
        }
        let granted = match &self.voted_for {
            None => { self.voted_for = Some(candidate.to_string()); true }
            Some(c) => c == candidate,
        };
        VoteReply { term: self.current_term, granted }
    }

    /// **PreVote** (Raft §9.6) — a *non-binding* poll that does **not** mutate term or
    /// vote. A candidate runs this before a real election: it bumps its term only after a
    /// quorum says it *would* vote, so a partitioned node that can't reach a majority
    /// never inflates its term and never disrupts the live leader when it rejoins.
    ///
    /// Grant iff the prospective term is current-or-newer **and** we are not ourselves a
    /// live leader (a leader won't endorse its own challenger). Read-only by design.
    pub fn consider_pre_vote(&self, term: u64) -> VoteReply {
        let granted = term >= self.current_term && !self.is_leader;
        VoteReply { term: self.current_term, granted }
    }

    /// Begin a candidacy: bump to a new term and vote for ourselves.
    pub fn start_candidacy(&mut self, self_id: &str) -> u64 {
        self.current_term += 1;
        self.voted_for = Some(self_id.to_string());
        self.is_leader = false;
        self.leader = None;
        self.current_term
    }

    /// Observe a peer's higher term and step back to follower for it.
    pub fn observe_term(&mut self, term: u64) {
        if term > self.current_term {
            self.current_term = term;
            self.voted_for = None;
            self.is_leader = false;
            self.leader = None;
        }
    }

    /// Record that this node won the election and is now leader.
    pub fn become_leader(&mut self, self_id: &str) {
        self.is_leader = true;
        self.leader = Some(self_id.to_string());
    }

    /// Record an externally-observed leader for a term ≥ ours (discovery).
    pub fn observe_leader(&mut self, term: u64, leader: &str) {
        if term >= self.current_term {
            self.current_term = term;
            self.leader = Some(leader.to_string());
            self.is_leader = false;
        }
    }

    pub fn leader_info(&self) -> LeaderInfo {
        LeaderInfo { term: self.current_term, leader: self.leader.clone() }
    }
}

/// Votes needed for a majority of `members` nodes.
pub fn quorum(members: usize) -> usize {
    members / 2 + 1
}

/// Did `granted_votes` reach a majority of `members`?
pub fn has_quorum(granted_votes: usize, members: usize) -> bool {
    members > 0 && granted_votes >= quorum(members)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_one_vote_per_term() {
        let mut s = ElectionState::default();
        // First candidate in term 1 gets the vote.
        assert!(s.consider_vote(1, "A").granted);
        // A second, different candidate in the same term is denied.
        assert!(!s.consider_vote(1, "B").granted);
        // Re-asking for the same candidate is idempotently granted.
        assert!(s.consider_vote(1, "A").granted);
    }

    #[test]
    fn test_newer_term_resets_vote() {
        let mut s = ElectionState::default();
        assert!(s.consider_vote(1, "A").granted);
        // A higher term resets per-term voting → B can win term 2.
        assert!(s.consider_vote(2, "B").granted);
        assert_eq!(s.current_term, 2);
    }

    #[test]
    fn test_stale_term_rejected() {
        let mut s = ElectionState::default();
        s.consider_vote(5, "A");
        let r = s.consider_vote(3, "B"); // stale
        assert!(!r.granted);
        assert_eq!(r.term, 5);
    }

    #[test]
    fn test_leader_and_stepdown() {
        let mut s = ElectionState::default();
        s.start_candidacy("A");
        s.become_leader("A");
        assert!(s.is_leader);
        assert_eq!(s.leader_info().leader.as_deref(), Some("A"));
        // A higher-term vote request makes the leader step down.
        s.consider_vote(s.current_term + 1, "B");
        assert!(!s.is_leader);
        assert_eq!(s.leader_info().leader, None);
    }

    #[test]
    fn test_observe_leader_repoint() {
        let mut s = ElectionState::default();
        s.observe_leader(7, "http://leader:9100");
        assert_eq!(s.current_term, 7);
        assert_eq!(s.leader_info().leader.as_deref(), Some("http://leader:9100"));
        assert!(!s.is_leader);
    }

    #[test]
    fn test_quorum_math() {
        assert_eq!(quorum(1), 1);
        assert_eq!(quorum(2), 2);
        assert_eq!(quorum(3), 2);
        assert_eq!(quorum(5), 3);
        assert!(has_quorum(2, 3));   // majority of 3
        assert!(!has_quorum(1, 3));  // minority can't elect → no split-brain
        assert!(!has_quorum(2, 5));
        assert!(has_quorum(3, 5));
    }

    #[test]
    fn test_pre_vote_is_non_binding() {
        let mut s = ElectionState::default();
        s.consider_vote(5, "A"); // term=5, voted_for=A
        // A pre-vote at term 6 is granted but must NOT change term or recorded vote.
        let pv = s.consider_pre_vote(6);
        assert!(pv.granted);
        assert_eq!(s.current_term, 5);
        assert_eq!(s.voted_for.as_deref(), Some("A"));
    }

    #[test]
    fn test_live_leader_refuses_pre_vote() {
        let mut s = ElectionState::default();
        s.start_candidacy("self");
        s.become_leader("self");
        // A healthy leader won't endorse a challenger, even at a higher term.
        assert!(!s.consider_pre_vote(s.current_term + 1).granted);
    }

    #[test]
    fn test_pre_vote_rejects_stale_term() {
        let mut s = ElectionState::default();
        s.consider_vote(5, "A");
        assert!(!s.consider_pre_vote(4).granted); // older term → no
        assert!(s.consider_pre_vote(5).granted);  // current → yes (non-leader)
    }

    #[test]
    fn test_two_candidates_cannot_both_win_a_term() {
        // Members {A, B, C}, quorum 2. A and B both run in term 1; C is the
        // tiebreaker and grants only the first asker.
        let members = 3;
        let mut c = ElectionState::default();
        let a_from_c = c.consider_vote(1, "A").granted; // C asked by A first → true
        let b_from_c = c.consider_vote(1, "B").granted; // C already voted A → false

        let a_votes = 1 + a_from_c as usize; // A: self + C = 2
        let b_votes = 1 + b_from_c as usize; // B: self only = 1

        assert!(has_quorum(a_votes, members));   // A wins (2 ≥ 2)
        assert!(!has_quorum(b_votes, members));  // B cannot → no split-brain
        // Exactly one leader in the term.
        assert!(has_quorum(a_votes, members) ^ has_quorum(b_votes, members));
    }
}

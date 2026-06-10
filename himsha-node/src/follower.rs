//! ZK-native follower (read replica).
//!
//! A follower replicates a primary node's state **without trusting its account
//! values**: it pulls each block over JSON-RPC and *independently re-derives* every
//! state transition by re-executing the block's transactions through the same
//! executor. In native mode that re-execution *is* the verification; under the
//! `zkvm` feature the executor verifies the RISC Zero receipt instead. Either way the
//! follower only accepts state it recomputed itself.
//!
//! This is the read-decentralization half of the ZK-native design (Option B):
//! anyone can run a follower and serve trust-minimized reads. It does not produce
//! blocks and does not broadcast Bitcoin settlements (custody stays with the primary
//! / a future FROST committee).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use himsha_runtime::{
    account::{AccountInfo, StoredAccount},
    transaction::Block,
};
use himsha_vm::{
    executor::{ExecutionInput, ProgramExecutor},
    registry::ProgramRegistry,
};
use serde_json::json;
use tracing::{info, warn};

use crate::{settlement, state::NodeState};

/// Promotion decision: promote once the primary has been unreachable for
/// `max_misses` consecutive polls (when failover is enabled).
pub fn should_promote(consecutive_misses: u32, max_misses: Option<u32>) -> bool {
    matches!(max_misses, Some(max) if max > 0 && consecutive_misses >= max)
}

/// Split-brain-safe election: a standby takes over only when the primary is down
/// **and** no higher-priority standby is still alive. With each standby configured
/// to defer to all higher-priority peers, exactly one (the highest-priority live
/// node) promotes — no two sequencers at once.
pub fn wins_election(primary_unreachable: bool, higher_priority_peer_alive: bool) -> bool {
    primary_unreachable && !higher_priority_peer_alive
}

pub struct Follower {
    state: NodeState,
    registry: Arc<Mutex<ProgramRegistry>>,
    /// Follow target; mutable so the node can **re-point** to a newly-elected leader.
    primary_url: Mutex<String>,
    /// Higher-priority standbys this node must yield to (crash-safe mode). Empty = top.
    higher_peers: Vec<String>,
    /// Quorum-election config (partition-safe mode): shared election state, the full
    /// electable member set (URLs), and this node's id. When set, takes precedence.
    election: Option<Arc<Mutex<crate::election::ElectionState>>>,
    members: Vec<String>,
    self_id: String,
    client: reqwest::Client,
}

impl Follower {
    pub fn new(
        state: NodeState,
        registry: Arc<Mutex<ProgramRegistry>>,
        primary_url: String,
    ) -> Self {
        Self {
            state,
            registry,
            primary_url: Mutex::new(primary_url),
            higher_peers: Vec::new(),
            election: None,
            members: Vec::new(),
            self_id: String::new(),
            client: reqwest::Client::new(),
        }
    }

    /// Configure the higher-priority standby peers this node defers to in an election.
    pub fn with_higher_peers(mut self, peers: Vec<String>) -> Self {
        self.higher_peers = peers;
        self
    }

    /// Configure partition-safe quorum election: shared state, member set, self id.
    pub fn with_election(
        mut self,
        election: Arc<Mutex<crate::election::ElectionState>>,
        members: Vec<String>,
        self_id: String,
    ) -> Self {
        self.election = Some(election);
        self.members = members;
        self.self_id = self_id;
        self
    }

    /// Poll the primary forever, replicating new blocks (pure replica, never promotes).
    pub async fn run(self, poll: Duration) {
        info!(
            "follower replicating from primary {} (poll {poll:?})",
            self.primary_url.lock().unwrap()
        );
        let _ = self.run_until_promote(poll, None).await;
    }

    /// Replicate from the primary until it becomes unreachable for `max_misses`
    /// consecutive polls, then return `true` to signal this node should promote
    /// itself to sequencer. With `max_misses == None` it replicates forever and
    /// never promotes. Returns `true` only on a promotion decision.
    pub async fn run_until_promote(&self, poll: Duration, max_misses: Option<u32>) -> bool {
        info!(
            "follower replicating from {} (poll {poll:?}, failover={:?})",
            self.primary_url.lock().unwrap().clone(),
            max_misses
        );
        let mut misses: u32 = 0;
        loop {
            match self.sync_once().await {
                Ok(_) => misses = 0,
                Err(e) => {
                    misses += 1;
                    warn!("follower sync failed ({misses} consecutive): {e}");
                }
            }
            if should_promote(misses, max_misses) {
                // Liveness first: if a leader already exists among members, re-point to
                // it and resume following — no election needed (heartbeat via getLeader).
                if let Some((term, leader)) = self.discover_leader().await {
                    let current = self.primary_url.lock().unwrap().clone();
                    if leader != current && leader != self.self_id {
                        info!("re-pointing follow target to leader {leader} (term {term})");
                        *self.primary_url.lock().unwrap() = leader.clone();
                        if let Some(e) = &self.election {
                            e.lock().unwrap().observe_leader(term, &leader);
                        }
                        misses = 0;
                        tokio::time::sleep(poll).await;
                        continue;
                    }
                }

                let promote = if self.election.is_some() && !self.members.is_empty() {
                    // Partition-safe: must win a majority quorum to promote.
                    self.elect().await
                } else {
                    // Crash-safe fallback: yield to any higher-priority standby alive.
                    let higher_alive = self.any_higher_peer_alive().await;
                    wins_election(true, higher_alive)
                };
                if promote {
                    if let Some(e) = &self.election {
                        e.lock().unwrap().become_leader(&self.self_id);
                    }
                    warn!("promoting to sequencer");
                    return true;
                }
                info!("not promoting this round (no quorum / deferring) — retrying");
            }
            tokio::time::sleep(poll).await;
        }
    }

    /// Run one quorum election round (Raft-style). Returns true if this node won a
    /// majority of the member set and may promote. Partition-safe: a minority side
    /// can never reach majority.
    async fn elect(&self) -> bool {
        let election = match &self.election {
            Some(e) => e,
            None => return false,
        };
        if self.members.is_empty() {
            return false;
        }

        // Staggered candidacy (deterministic per-node jitter) reduces split-vote
        // livelock — a poor-man's randomized election timeout.
        let jitter = self.self_id.bytes().map(|b| b as u64).sum::<u64>() % 300 + 20;
        tokio::time::sleep(Duration::from_millis(jitter)).await;

        // PreVote (Raft §9.6): a non-binding poll at term+1 that does NOT bump our term.
        // We only proceed to a real election if a quorum would vote for us — so a
        // partitioned node that can't reach a majority never inflates its term and never
        // disrupts the live leader when the partition heals.
        let pv_term = election.lock().unwrap().current_term + 1;
        let mut pre_votes = 1usize; // we'd vote for ourselves
        for m in &self.members {
            if m == &self.self_id {
                continue;
            }
            if let Ok(v) = self
                .rpc_to(m, "himsha_preVote", json!([pv_term, self.self_id]))
                .await
            {
                if v.get("granted").and_then(|g| g.as_bool()) == Some(true) {
                    pre_votes += 1;
                } else if let Some(t) = v.get("term").and_then(|t| t.as_u64()) {
                    election.lock().unwrap().observe_term(t);
                }
            }
        }
        if !crate::election::has_quorum(pre_votes, self.members.len()) {
            info!(
                "pre-vote {pre_votes}/{} — no quorum; not inflating term",
                self.members.len()
            );
            return false;
        }

        let term = election.lock().unwrap().start_candidacy(&self.self_id);
        let mut votes = 1usize; // vote for self
        for m in &self.members {
            if m == &self.self_id {
                continue;
            }
            if let Ok(v) = self
                .rpc_to(m, "himsha_requestVote", json!([term, self.self_id]))
                .await
            {
                if v.get("granted").and_then(|g| g.as_bool()) == Some(true) {
                    votes += 1;
                } else if let Some(t) = v.get("term").and_then(|t| t.as_u64()) {
                    // A peer at a higher term → step down for it.
                    election.lock().unwrap().observe_term(t);
                }
            }
        }
        let won = crate::election::has_quorum(votes, self.members.len());
        if won {
            warn!(
                "won election term {term} with {votes}/{} votes",
                self.members.len()
            );
        } else {
            info!(
                "election term {term}: {votes}/{} votes — no quorum",
                self.members.len()
            );
        }
        won
    }

    /// True if any configured higher-priority standby answers `isNodeReady`.
    async fn any_higher_peer_alive(&self) -> bool {
        for url in &self.higher_peers {
            if let Ok(v) = self.rpc_to(url, "himsha_isNodeReady", json!([])).await {
                if v.as_bool() == Some(true) {
                    return true;
                }
            }
        }
        false
    }

    /// Catch up from the local slot to the primary's tip.
    pub async fn sync_once(&self) -> Result<()> {
        let target = self.rpc_u64("himsha_getSlot").await?;
        let mut local = self.state.current_slot()?;
        while local < target {
            let next = local + 1;
            match self.fetch_block(next).await? {
                Some(block) => {
                    self.apply_block(&block)?;
                    info!(
                        "follower replicated block slot={} txs={}",
                        block.slot,
                        block.transactions.len()
                    );
                }
                None => break, // not available yet; try again next poll
            }
            local = next;
        }
        Ok(())
    }

    /// Re-execute and persist one block independently, then advance the local slot.
    pub fn apply_block(&self, block: &Block) -> Result<()> {
        for tx in &block.transactions {
            self.apply_transaction(tx, block.timestamp)?;
        }
        self.state
            .save_block(block.slot, serde_json::to_vec(block)?)
            .map_err(|e| anyhow!("save_block: {e}"))?;
        // Keep the replica's tx index in step so its explorer/lookup RPCs match.
        for tx in &block.transactions {
            let _ = self.state.index_transaction(&tx.message_hash(), block.slot);
        }
        // Advance the local slot to match (slots are sequential).
        while self.state.current_slot()? < block.slot {
            self.state
                .advance_slot()
                .map_err(|e| anyhow!("advance_slot: {e}"))?;
        }
        Ok(())
    }

    fn apply_transaction(
        &self,
        tx: &himsha_runtime::transaction::RuntimeTransaction,
        _ts: u64,
    ) -> Result<()> {
        for instr in &tx.message.instructions {
            // Build the instruction's accounts from *our own* replicated state.
            let mut accounts: Vec<AccountInfo> = Vec::with_capacity(instr.accounts.len());
            for meta in &instr.accounts {
                let mut acc = match self.state.load_account(&meta.pubkey) {
                    Ok(Some(stored)) => stored.into_account(meta.pubkey),
                    _ => AccountInfo::new(meta.pubkey, instr.program_id, 0, 0),
                };
                acc.is_signer = meta.is_signer;
                acc.is_writable = meta.is_writable;
                accounts.push(acc);
            }

            let reg = self.registry.lock().unwrap();
            let executor = ProgramExecutor::new(&reg);
            let input = ExecutionInput {
                accounts,
                instruction_data: instr.data.clone(),
                timestamp: tx.message.timestamp,
            };

            // Independent re-derivation: native re-execution, or receipt verification
            // under the `zkvm` feature. We accept only what we recompute.
            let mut transition = executor
                .execute_program(&instr.program_id, input, vec![])
                .map_err(|e| anyhow!("re-execute failed: {e}"))?;

            // Gate on the receipt: only persist a transition whose receipt commits
            // to the accounts we recomputed.
            transition
                .verify()
                .map_err(|e| anyhow!("invalid execution receipt: {e}"))?;

            // Match the primary's post-execution settlement clearing (no broadcast).
            if instr.program_id == himsha_runtime::program_ids::lending_program() {
                settlement::drain_lending(&mut transition.updated_accounts, None);
            }

            for account in &transition.updated_accounts {
                let stored = StoredAccount::from(account);
                self.state
                    .save_account(&account.key, &stored)
                    .map_err(|e| anyhow!("save_account: {e}"))?;
            }
        }
        Ok(())
    }

    async fn rpc_u64(&self, method: &str) -> Result<u64> {
        let v = self.rpc(method, json!([])).await?;
        v.as_u64().ok_or_else(|| anyhow!("{method}: expected u64"))
    }

    async fn fetch_block(&self, slot: u64) -> Result<Option<Block>> {
        let v = self.rpc("himsha_getBlock", json!([slot])).await?;
        if v.is_null() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_value(v)?))
    }

    async fn rpc(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let url = self.primary_url.lock().unwrap().clone();
        self.rpc_to(&url, method, params).await
    }

    /// Probe members' `getLeader` and return the highest-term advertised leader, if any.
    async fn discover_leader(&self) -> Option<(u64, String)> {
        let mut best: Option<(u64, String)> = None;
        for m in &self.members {
            if m == &self.self_id {
                continue;
            }
            if let Ok(v) = self.rpc_to(m, "himsha_getLeader", json!([])).await {
                if let Some(leader) = v.get("leader").and_then(|l| l.as_str()) {
                    let term = v.get("term").and_then(|t| t.as_u64()).unwrap_or(0);
                    if best.as_ref().map(|(bt, _)| term > *bt).unwrap_or(true) {
                        best = Some((term, leader.to_string()));
                    }
                }
            }
        }
        best
    }

    async fn rpc_to(
        &self,
        url: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
        let resp: serde_json::Value = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;
        if let Some(err) = resp.get("error") {
            return Err(anyhow!("rpc error: {err}"));
        }
        Ok(resp
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use himsha_runtime::{
        account::AccountInfo,
        account::AccountMeta,
        instruction::Instruction,
        program_ids,
        pubkey::Pubkey,
        transaction::{Block, Message, RuntimeTransaction},
    };

    fn registry() -> Arc<Mutex<ProgramRegistry>> {
        let mut reg = ProgramRegistry::new();
        for id in program_ids::builtins() {
            reg.register(id, Vec::new(), [0u8; 32]);
        }
        Arc::new(Mutex::new(reg))
    }

    #[test]
    fn test_wins_election_avoids_split_brain() {
        // Highest-priority standby (no higher peers alive) takes over.
        assert!(wins_election(true, false));
        // A higher-priority standby is alive → defer (prevents two sequencers).
        assert!(!wins_election(true, true));
        // Primary still up → never promote.
        assert!(!wins_election(false, false));
        assert!(!wins_election(false, true));
    }

    #[test]
    fn test_should_promote_logic() {
        assert!(!should_promote(5, None)); // failover disabled → never
        assert!(!should_promote(1, Some(3))); // below threshold
        assert!(!should_promote(2, Some(3)));
        assert!(should_promote(3, Some(3))); // reached threshold
        assert!(should_promote(9, Some(3))); // beyond threshold
        assert!(!should_promote(0, Some(0))); // 0 threshold is a no-op guard
    }

    #[test]
    fn test_follower_replicates_transfer_by_reexecution() {
        // Temp DB for the follower replica.
        let path =
            std::env::temp_dir().join(format!("himsha-follower-{}.redb", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let state = NodeState::open(path.to_str().unwrap()).unwrap();

        // Seed the follower with a funded account (as if an earlier block created it).
        let sys = program_ids::system_program();
        let from = Pubkey::from_seed(b"from");
        let to = Pubkey::from_seed(b"to");
        let mut from_acc = AccountInfo::new(from, sys, 1_000, 0);
        state
            .save_account(&from, &StoredAccount::from(&from_acc))
            .unwrap();

        // A block containing a system Transfer of 250, exactly as the primary would emit.
        let data =
            borsh::to_vec(&himsha_system_program::SystemInstruction::Transfer { lamports: 250 })
                .unwrap();
        let ix = Instruction::new(
            sys,
            vec![
                AccountMeta::writable(from, true), // signer
                AccountMeta::writable(to, false),
            ],
            data,
        );
        let msg = Message::new(vec![from], vec![ix], 0);
        let tx = RuntimeTransaction::unsigned(msg);
        let block = Block::new(1, 0, vec![tx], 0);

        let follower = Follower::new(state.clone(), registry(), "http://unused".into());
        follower.apply_block(&block).unwrap();

        // The follower re-derived the balances itself.
        let from_after = state.load_account(&from).unwrap().unwrap();
        let to_after = state.load_account(&to).unwrap().unwrap();
        assert_eq!(from_after.lamports, 750);
        assert_eq!(to_after.lamports, 250);
        assert_eq!(state.current_slot().unwrap(), 1);

        let _ = &mut from_acc;
        let _ = std::fs::remove_file(&path);
    }
}

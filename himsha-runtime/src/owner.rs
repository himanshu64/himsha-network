//! Owner-gated writes — "only the owning program may write an account".
//!
//! Programs receive cloned [`AccountInfo`]s and mutate them freely in memory, so
//! ownership is enforced *after the fact*: compare each account's pre-execution
//! snapshot with its post-execution state and reject the transition unless every
//! change is legal for the program that produced it. The rule (Solana's write
//! semantics, validated post-hoc):
//!
//!   - the **owning** program may change anything (data, lamports, even reassign
//!     `owner` — that's how the ATA program hands a fresh account to the token
//!     program);
//!   - any program may **credit** lamports to any account;
//!   - a **blank** account (no lamports, no data, no UTXO anchor — the shape the
//!     node lazily materializes for never-seen pubkeys) is claimed by its first
//!     writer: the write is allowed and the account's `owner` becomes the writing
//!     program unless it explicitly assigned a different one;
//!   - everything else is [`ProgramError::IllegalOwnerWrite`].
//!
//! Nested CPI makes a single post-hoc diff insufficient: when the vault invokes
//! the money market, the *vault's* top-level diff shows a market account (owned
//! by the money market) changing — legal, but only because the money market did
//! it. So validation runs at **every** invocation boundary, innermost first, and
//! each boundary records the post-states it approved in a thread-local trail.
//! An outer boundary accepts a change it couldn't otherwise justify only if the
//! account's final state is exactly one an inner callee was already validated
//! to produce. [`begin_execution`] clears the trail at each top-level dispatch.
//!
//! Deployed (non-built-in) programs execute as standalone zkVM guests and cannot
//! CPI, so the node validates their output with an empty trail — every change
//! must be directly legal for the program itself.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::{account::AccountInfo, error::ProgramError, pubkey::Pubkey};

thread_local! {
    /// Post-states approved at inner CPI boundaries on this thread, keyed by
    /// account. Consulted (and appended to) by [`validate_write`]; cleared at
    /// each top-level execution by [`begin_execution`].
    static APPROVED: RefCell<HashMap<Pubkey, AccountInfo>> = RefCell::new(HashMap::new());
}

/// Reset the per-execution state — the CPI approval trail *and* the compute
/// meter. Call once at the start of every top-level program execution (the
/// dispatcher and zkVM guest do this; CPI re-entries must not).
pub fn begin_execution() {
    APPROVED.with(|m| m.borrow_mut().clear());
    crate::compute::reset();
}

/// The persisted fields of two snapshots of the same account are identical.
/// (`is_signer` / `is_writable` are transient per-instruction flags and ignored.)
fn same_state(a: &AccountInfo, b: &AccountInfo) -> bool {
    a.lamports == b.lamports
        && a.data == b.data
        && a.owner == b.owner
        && a.executable == b.executable
        && a.utxo == b.utxo
}

/// An account nobody has claimed yet — the shape the node materializes for a
/// pubkey it has never stored. Its `owner` is whatever the materializer chose,
/// so ownership is decided by the first program to actually write it.
fn is_blank(a: &AccountInfo) -> bool {
    a.lamports == 0 && a.data.is_empty() && !a.executable && a.utxo.is_none()
}

/// Everything except the lamport balance is unchanged, and lamports did not
/// decrease — the one mutation any program may make to any account.
fn credit_only(pre: &AccountInfo, post: &AccountInfo) -> bool {
    post.lamports >= pre.lamports
        && post.data == pre.data
        && post.owner == pre.owner
        && post.executable == pre.executable
        && post.utxo == pre.utxo
}

/// Validate one account's transition under `program_id` and record the approved
/// post-state in the trail. May rewrite `post.owner` (first-writer claim).
pub fn validate_write(
    program_id: &Pubkey,
    pre: &AccountInfo,
    post: &mut AccountInfo,
) -> Result<(), ProgramError> {
    if same_state(pre, post) {
        return Ok(());
    }
    if pre.owner == *program_id {
        record(post);
        return Ok(());
    }
    if credit_only(pre, post) {
        record(post);
        return Ok(());
    }
    if is_blank(pre) {
        // First writer claims the account, unless it explicitly assigned the
        // account elsewhere (e.g. ATA creation assigns to the token program).
        if post.owner == pre.owner {
            post.owner = *program_id;
        }
        record(post);
        return Ok(());
    }
    // Not ours, not a credit, not a claim — only legal if an inner CPI callee
    // was already validated to have produced exactly this state.
    let approved = APPROVED.with(|m| {
        m.borrow()
            .get(&post.key)
            .map(|a| same_state(a, post))
            .unwrap_or(false)
    });
    if approved {
        return Ok(());
    }
    Err(ProgramError::IllegalOwnerWrite)
}

/// Validate a whole account table (positionally paired pre/post snapshots).
pub fn validate_writes(
    program_id: &Pubkey,
    before: &[AccountInfo],
    after: &mut [AccountInfo],
) -> Result<(), ProgramError> {
    if before.len() != after.len() {
        return Err(ProgramError::IllegalOwnerWrite);
    }
    for (pre, post) in before.iter().zip(after.iter_mut()) {
        validate_write(program_id, pre, post)?;
    }
    Ok(())
}

fn record(post: &AccountInfo) {
    APPROVED.with(|m| m.borrow_mut().insert(post.key, post.clone()));
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn prog(seed: &[u8]) -> Pubkey {
        Pubkey::from_seed(seed)
    }

    fn acct(seed: &[u8], owner: Pubkey, lamports: u64, space: usize) -> AccountInfo {
        AccountInfo::new(Pubkey::from_seed(seed), owner, lamports, space)
    }

    #[test]
    fn owner_may_mutate_anything() {
        begin_execution();
        let p = prog(b"p");
        let pre = acct(b"a", p, 10, 4);
        let mut post = pre.clone();
        post.data = vec![9; 8];
        post.lamports = 0;
        post.owner = prog(b"other"); // owner may even reassign
        assert!(validate_write(&p, &pre, &mut post).is_ok());
    }

    #[test]
    fn non_owner_data_write_rejected() {
        begin_execution();
        let p = prog(b"p");
        let pre = acct(b"a", prog(b"other"), 10, 4);
        let mut post = pre.clone();
        post.data[0] = 1;
        assert_eq!(
            validate_write(&p, &pre, &mut post),
            Err(ProgramError::IllegalOwnerWrite)
        );
    }

    #[test]
    fn non_owner_lamport_debit_rejected_credit_allowed() {
        begin_execution();
        let p = prog(b"p");
        let pre = acct(b"a", prog(b"other"), 10, 0);

        let mut credit = pre.clone();
        credit.lamports = 15;
        assert!(validate_write(&p, &pre, &mut credit).is_ok());

        let mut debit = pre.clone();
        debit.lamports = 5;
        assert_eq!(
            validate_write(&p, &pre, &mut debit),
            Err(ProgramError::IllegalOwnerWrite)
        );
    }

    #[test]
    fn blank_account_claimed_by_first_writer() {
        begin_execution();
        let p = prog(b"p");
        let pre = acct(b"fresh", prog(b"materializer"), 0, 0);
        let mut post = pre.clone();
        post.data = vec![1, 2, 3];
        validate_write(&p, &pre, &mut post).unwrap();
        assert_eq!(post.owner, p, "first writer claims the blank account");
    }

    #[test]
    fn blank_claim_keeps_explicit_assignment() {
        begin_execution();
        let p = prog(b"p");
        let token = prog(b"token");
        let pre = acct(b"fresh", prog(b"materializer"), 0, 0);
        let mut post = pre.clone();
        post.data = vec![1];
        post.owner = token; // writer explicitly handed it to another program
        validate_write(&p, &pre, &mut post).unwrap();
        assert_eq!(post.owner, token);
    }

    #[test]
    fn inner_cpi_approval_lets_outer_accept() {
        begin_execution();
        let outer = prog(b"outer");
        let inner = prog(b"inner");
        let pre = acct(b"a", inner, 10, 4);

        // Inner callee (the owner) mutates and is validated → recorded.
        let mut mid = pre.clone();
        mid.data[0] = 7;
        validate_write(&inner, &pre, &mut mid).unwrap();

        // Outer program doesn't own the account, but the final state matches
        // what the inner callee was approved to produce.
        let mut post = mid.clone();
        assert!(validate_write(&outer, &pre, &mut post).is_ok());

        // A state the inner callee did NOT produce is still rejected.
        let mut forged = pre.clone();
        forged.data[0] = 99;
        assert_eq!(
            validate_write(&outer, &pre, &mut forged),
            Err(ProgramError::IllegalOwnerWrite)
        );
    }

    #[test]
    fn begin_execution_clears_the_trail() {
        begin_execution();
        let inner = prog(b"inner");
        let pre = acct(b"a", inner, 10, 4);
        let mut mid = pre.clone();
        mid.data[0] = 7;
        validate_write(&inner, &pre, &mut mid).unwrap();

        begin_execution(); // new top-level execution → stale approvals gone
        let outer = prog(b"outer");
        let mut post = mid.clone();
        assert_eq!(
            validate_write(&outer, &pre, &mut post),
            Err(ProgramError::IllegalOwnerWrite)
        );
    }

    #[test]
    fn length_mismatch_rejected() {
        begin_execution();
        let p = prog(b"p");
        let before = vec![acct(b"a", p, 0, 0)];
        let mut after: Vec<AccountInfo> = vec![];
        assert_eq!(
            validate_writes(&p, &before, &mut after),
            Err(ProgramError::IllegalOwnerWrite)
        );
    }
}

//! Cross-Program Invocation (CPI).
//!
//! A program receives a flat `&mut [AccountInfo]` window. To invoke another
//! program it must hand that callee the subset of accounts the callee expects,
//! in the order the callee expects, run the callee, and reflect any mutations
//! back into its own window.
//!
//! [`invoke_indexed`] is that primitive. The caller passes:
//!   - its own `accounts` slice,
//!   - the `indices` into that slice naming the callee's accounts (ordered),
//!   - the callee's instruction `data`,
//!   - the callee's `process` function.
//!
//! In native execution the callee is just another Rust `process()` (e.g.
//! `himsha_token_program::process`). Under the zkVM model the same (indices, data,
//! program-id) shape becomes a cross-program syscall — the interface is identical,
//! only the executor differs. Keeping CPI index-based (rather than by `Pubkey`)
//! avoids depending on duplicate accounts and matches how a guest would reference
//! its account table.

use std::cell::Cell;

use crate::{account::AccountInfo, error::ProgramError, owner, pubkey::Pubkey};

/// Maximum nested cross-program-invocation depth. Bounds recursion so a program
/// (or a cycle of programs) can't blow the native stack via unbounded CPI.
pub const MAX_CPI_DEPTH: u32 = 4;

thread_local! {
    /// Current CPI nesting depth on this execution thread.
    static CPI_DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// RAII guard that bounds CPI nesting. `enter` increments the depth (erroring if it
/// would exceed [`MAX_CPI_DEPTH`]) and `drop` always decrements it — so the depth
/// self-balances back to zero after each top-level execution with no explicit reset,
/// even when a nested call errors and unwinds.
struct CpiDepthGuard;

impl CpiDepthGuard {
    fn enter() -> Result<Self, ProgramError> {
        let next = CPI_DEPTH.with(|d| d.get()) + 1;
        if next > MAX_CPI_DEPTH {
            return Err(ProgramError::CpiDepthExceeded);
        }
        CPI_DEPTH.with(|d| d.set(next));
        Ok(CpiDepthGuard)
    }
}

impl Drop for CpiDepthGuard {
    fn drop(&mut self) {
        CPI_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Invoke `process` with the sub-window of `accounts` named by `indices`.
///
/// `callee_program_id` identifies the program `process` implements; the window
/// is owner-validated against it (see [`crate::owner`]) before any mutation is
/// written back into `accounts`. On callee error or an illegal write, nothing
/// is written back.
///
/// Returns [`ProgramError::NotEnoughAccounts`] if any index is out of bounds.
pub fn invoke_indexed<F>(
    accounts: &mut [AccountInfo],
    indices: &[usize],
    data: &[u8],
    callee_program_id: &Pubkey,
    process: F,
) -> Result<(), ProgramError>
where
    F: FnOnce(&mut [AccountInfo], &[u8]) -> Result<(), ProgramError>,
{
    // Bounds-check first so a bad index never panics mid-copy.
    for &i in indices {
        if i >= accounts.len() {
            return Err(ProgramError::NotEnoughAccounts);
        }
    }

    invoke_signed_indexed(accounts, indices, &[], data, callee_program_id, process)
}

/// Like [`invoke_indexed`], but additionally marks the window accounts at
/// `signer_window_indices` (positions *within the callee's window*, 0-based) as
/// signers for the duration of the call.
///
/// This is the analogue of Solana's `invoke_signed`: a program "signs" on behalf
/// of an authority it controls (e.g. a pool/market vault authority) that did not
/// itself sign the outer transaction. The signer flag is scoped to the window
/// and is not written back to the parent accounts.
pub fn invoke_signed_indexed<F>(
    accounts: &mut [AccountInfo],
    indices: &[usize],
    signer_window_indices: &[usize],
    data: &[u8],
    callee_program_id: &Pubkey,
    process: F,
) -> Result<(), ProgramError>
where
    F: FnOnce(&mut [AccountInfo], &[u8]) -> Result<(), ProgramError>,
{
    for &i in indices {
        if i >= accounts.len() {
            return Err(ProgramError::NotEnoughAccounts);
        }
    }

    // Bound CPI nesting depth; the guard decrements on drop (including on unwind).
    let _depth_guard = CpiDepthGuard::enter()?;

    // Charge the compute budget: a fixed cost per invocation plus a per-account
    // cost for the window we're about to clone in and write back. Bounds total
    // CPI fan-out width (depth is bounded separately above).
    crate::compute::charge(
        crate::compute::CPI_BASE_COST + crate::compute::PER_ACCOUNT_COST * indices.len() as u64,
    )?;

    let mut window: Vec<AccountInfo> = indices.iter().map(|&i| accounts[i].clone()).collect();

    // A window that names the same parent account writable in two slots would let
    // the callee debit one copy and credit the other from the same balance, and
    // the last write back wins — minting balance from nothing. Reject it.
    crate::account::reject_duplicate_writable(&window)?;

    for &w in signer_window_indices {
        if let Some(a) = window.get_mut(w) {
            a.is_signer = true;
        }
    }
    process(&mut window, data)?;

    // Owner-gate the callee's mutations against the untouched parent state
    // before anything is written back (records approvals for outer boundaries).
    for (k, &i) in indices.iter().enumerate() {
        owner::validate_write(callee_program_id, &accounts[i], &mut window[k])?;
    }

    // Write back mutations, but never persist the synthetic signer flag.
    for (mut slot, &i) in window.into_iter().zip(indices) {
        slot.is_signer = accounts[i].is_signer;
        accounts[i] = slot;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn callee_id() -> Pubkey {
        Pubkey::from_seed(b"test-callee-program")
    }

    fn acc(seed: &str) -> AccountInfo {
        AccountInfo::new(Pubkey::from_seed(seed.as_bytes()), callee_id(), 0, 8)
    }

    // A trivial callee: write the instruction byte into accounts[1].data[0],
    // and bump accounts[0].lamports — lets us assert mutations propagate back.
    fn callee(accounts: &mut [AccountInfo], data: &[u8]) -> Result<(), ProgramError> {
        if accounts.len() < 2 {
            return Err(ProgramError::NotEnoughAccounts);
        }
        accounts[0].lamports += 1;
        accounts[1].data[0] = data[0];
        Ok(())
    }

    #[test]
    fn writes_back_to_parent_indices() {
        let mut accounts = vec![acc("a"), acc("b"), acc("c"), acc("d")];
        // Callee should see parent[2] as its [0] and parent[0] as its [1].
        invoke_indexed(&mut accounts, &[2, 0], &[0x42], &callee_id(), callee).unwrap();
        assert_eq!(
            accounts[2].lamports, 1,
            "callee[0] -> parent[2] lamports bumped"
        );
        assert_eq!(
            accounts[0].data[0], 0x42,
            "callee[1] -> parent[0] data written"
        );
        // Untouched accounts stay put.
        assert_eq!(accounts[1].lamports, 0);
        assert_eq!(accounts[3].data[0], 0);
    }

    #[test]
    fn duplicate_writable_window_index_errors() {
        // Passing the same parent index twice yields a window with the same key
        // writable in two slots — must be rejected before the callee runs, and the
        // parent must be left untouched.
        let mut accounts = vec![acc("a"), acc("b")];
        let before = accounts[0].lamports;
        let r = invoke_indexed(&mut accounts, &[0, 0], &[0x42], &callee_id(), callee);
        assert_eq!(r, Err(ProgramError::DuplicateWritableAccount));
        assert_eq!(
            accounts[0].lamports, before,
            "parent untouched after rejection"
        );
    }

    #[test]
    fn out_of_bounds_index_errors() {
        let mut accounts = vec![acc("a")];
        assert_eq!(
            invoke_indexed(&mut accounts, &[0, 5], &[0], &callee_id(), callee),
            Err(ProgramError::NotEnoughAccounts),
        );
    }

    #[test]
    fn callee_error_does_not_write_back() {
        let mut accounts = vec![acc("a")];
        // Only one account in the window -> callee returns NotEnoughAccounts,
        // and parent must be left untouched.
        let before = accounts[0].lamports;
        let r = invoke_indexed(&mut accounts, &[0], &[0x42], &callee_id(), callee);
        assert_eq!(r, Err(ProgramError::NotEnoughAccounts));
        assert_eq!(accounts[0].lamports, before);
    }

    #[test]
    fn callee_writing_account_it_does_not_own_is_rejected() {
        crate::owner::begin_execution();
        // Window accounts owned by some *other* program: the callee's data write
        // must be rejected and nothing written back to the parent.
        let other = Pubkey::from_seed(b"other-program");
        let mut a = acc("a");
        let mut b = acc("b");
        a.owner = other;
        b.owner = other;
        let mut accounts = vec![a, b];
        let r = invoke_indexed(&mut accounts, &[0, 1], &[0x42], &callee_id(), callee);
        assert_eq!(r, Err(ProgramError::IllegalOwnerWrite));
        assert_eq!(accounts[1].data[0], 0, "parent untouched after rejection");
        // (accounts[0] only received a lamport credit — that alone is legal, but
        // the whole window is rejected atomically before write-back.)
        assert_eq!(accounts[0].lamports, 0);
    }

    // A callee that re-invokes itself forever — depth must cut it off.
    fn recurse(accounts: &mut [AccountInfo], data: &[u8]) -> Result<(), ProgramError> {
        invoke_indexed(accounts, &[0], data, &callee_id(), recurse)
    }

    #[test]
    fn cpi_fanout_is_compute_bounded() {
        // A program that issues a flat sequence of CPIs (no depth growth) is bounded
        // by the compute meter: after enough invocations the budget is exhausted.
        crate::owner::begin_execution(); // resets the meter
        let mut accounts = vec![acc("a"), acc("b")];
        let noop = |_: &mut [AccountInfo], _: &[u8]| Ok(());
        // Each call charges CPI_BASE_COST + 2*PER_ACCOUNT_COST = 1200 units.
        let per_call = crate::compute::CPI_BASE_COST + 2 * crate::compute::PER_ACCOUNT_COST;
        let affordable = crate::compute::COMPUTE_BUDGET / per_call;
        for _ in 0..affordable {
            invoke_indexed(&mut accounts, &[0, 1], &[], &callee_id(), noop).unwrap();
        }
        // The next invocation tips over the budget.
        assert_eq!(
            invoke_indexed(&mut accounts, &[0, 1], &[], &callee_id(), noop),
            Err(ProgramError::ComputeExhausted),
        );
    }

    #[test]
    fn cpi_depth_is_bounded() {
        let mut accounts = vec![acc("a")];
        assert_eq!(
            invoke_indexed(&mut accounts, &[0], &[0], &callee_id(), recurse),
            Err(ProgramError::CpiDepthExceeded),
        );
        // Depth fully unwound (RAII) → a normal one-level CPI still works afterward.
        let ok = invoke_indexed(
            &mut accounts,
            &[0],
            &[0],
            &callee_id(),
            |a: &mut [AccountInfo], _| {
                a[0].lamports += 1;
                Ok(())
            },
        );
        assert!(ok.is_ok());
    }
}

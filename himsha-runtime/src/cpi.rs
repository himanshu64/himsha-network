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

use crate::{account::AccountInfo, error::ProgramError};

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
/// Mutations the callee makes to its window are written back into `accounts`
/// at the same indices on success. On callee error, nothing is written back.
///
/// Returns [`ProgramError::NotEnoughAccounts`] if any index is out of bounds.
pub fn invoke_indexed<F>(
    accounts: &mut [AccountInfo],
    indices: &[usize],
    data: &[u8],
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

    invoke_signed_indexed(accounts, indices, &[], data, process)
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

    let mut window: Vec<AccountInfo> = indices.iter().map(|&i| accounts[i].clone()).collect();
    for &w in signer_window_indices {
        if let Some(a) = window.get_mut(w) {
            a.is_signer = true;
        }
    }
    process(&mut window, data)?;

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
    use crate::pubkey::Pubkey;

    fn acc(seed: &str) -> AccountInfo {
        AccountInfo::new(Pubkey::from_seed(seed.as_bytes()), Pubkey::default(), 0, 8)
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
        invoke_indexed(&mut accounts, &[2, 0], &[0x42], callee).unwrap();
        assert_eq!(accounts[2].lamports, 1, "callee[0] -> parent[2] lamports bumped");
        assert_eq!(accounts[0].data[0], 0x42, "callee[1] -> parent[0] data written");
        // Untouched accounts stay put.
        assert_eq!(accounts[1].lamports, 0);
        assert_eq!(accounts[3].data[0], 0);
    }

    #[test]
    fn out_of_bounds_index_errors() {
        let mut accounts = vec![acc("a")];
        assert_eq!(
            invoke_indexed(&mut accounts, &[0, 5], &[0], callee),
            Err(ProgramError::NotEnoughAccounts),
        );
    }

    #[test]
    fn callee_error_does_not_write_back() {
        let mut accounts = vec![acc("a")];
        // Only one account in the window -> callee returns NotEnoughAccounts,
        // and parent must be left untouched.
        let before = accounts[0].lamports;
        let r = invoke_indexed(&mut accounts, &[0], &[0x42], callee);
        assert_eq!(r, Err(ProgramError::NotEnoughAccounts));
        assert_eq!(accounts[0].lamports, before);
    }

    // A callee that re-invokes itself forever — depth must cut it off.
    fn recurse(accounts: &mut [AccountInfo], data: &[u8]) -> Result<(), ProgramError> {
        invoke_indexed(accounts, &[0], data, recurse)
    }

    #[test]
    fn cpi_depth_is_bounded() {
        let mut accounts = vec![acc("a")];
        assert_eq!(
            invoke_indexed(&mut accounts, &[0], &[0], recurse),
            Err(ProgramError::CpiDepthExceeded),
        );
        // Depth fully unwound (RAII) → a normal one-level CPI still works afterward.
        let ok = invoke_indexed(&mut accounts, &[0], &[0], |a: &mut [AccountInfo], _| {
            a[0].lamports += 1;
            Ok(())
        });
        assert!(ok.is_ok());
    }
}

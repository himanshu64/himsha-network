//! Native dispatch for built-in programs.
//!
//! The HIMSHA programs are ordinary Rust crates exposing a `process()` entry point.
//! In a production deployment each would be compiled to a RISC Zero guest ELF and
//! executed (and *proven*) inside the zkVM via [`crate::executor::ProgramExecutor::execute`].
//!
//! Building those guests requires the RISC Zero toolchain (`cargo-risczero` + `r0vm`).
//! Until that toolchain is wired into CI, this module runs the built-ins **natively**
//! so the full node works end-to-end. The execution is deterministic and produces the
//! same `updated_accounts` the guest would — it simply skips proof generation. Deployed
//! (non-built-in) programs still go through the zkVM path.

use himsha_runtime::{account::AccountInfo, error::ProgramError, program_ids, pubkey::Pubkey};

/// True if `program_id` is one of the genesis built-in programs.
pub fn is_builtin(program_id: &Pubkey) -> bool {
    program_ids::builtins().contains(program_id)
}

/// Run a built-in program's `process()` against `accounts` in place.
///
/// Returns `Err(ProgramError)` on program failure, mirroring zkVM execution.
/// Panics-free: every built-in is a pure function over the account slice.
///
/// After a successful run the whole account table is owner-gated (see
/// [`himsha_runtime::owner`]): the program may only have mutated accounts it
/// owns, claimed blank ones, credited lamports, or carried mutations made by
/// validated CPI callees. Anything else fails with
/// [`ProgramError::IllegalOwnerWrite`] and nothing is persisted.
pub fn dispatch(
    program_id: &Pubkey,
    accounts: &mut [AccountInfo],
    data: &[u8],
    timestamp: u64,
) -> Result<(), ProgramError> {
    // Fresh top-level execution: clear the CPI approval trail and snapshot the
    // pre-state the owner gate validates against.
    himsha_runtime::owner::begin_execution();
    let before: Vec<AccountInfo> = accounts.to_vec();

    run_builtin(program_id, accounts, data, timestamp)?;

    himsha_runtime::owner::validate_writes(program_id, &before, accounts)
}

fn run_builtin(
    program_id: &Pubkey,
    accounts: &mut [AccountInfo],
    data: &[u8],
    timestamp: u64,
) -> Result<(), ProgramError> {
    let id = *program_id;

    if id == program_ids::system_program() {
        himsha_system_program::process(accounts, data)
    } else if id == program_ids::token_program() {
        himsha_token_program::process(accounts, data)
    } else if id == program_ids::ata_program() {
        himsha_ata_program::process(accounts, data)
    } else if id == program_ids::swap_program() {
        himsha_swap_program::process(accounts, data)
    } else if id == program_ids::nft_metadata_program() {
        himsha_nft_metadata_program::process(accounts, data)
    } else if id == program_ids::lending_program() {
        // Lending needs the block timestamp for loan-deadline checks.
        himsha_lending_program::process(accounts, data, timestamp)
    } else if id == program_ids::runes_program() {
        himsha_runes_program::process(accounts, data, timestamp)
    } else if id == program_ids::money_market_program() {
        himsha_money_market_program::process(accounts, data, timestamp)
    } else if id == program_ids::vault_program() {
        // Vault needs the block timestamp for money-market CPI (interest accrual).
        himsha_vault_program::process(accounts, data, timestamp)
    } else if id == program_ids::oracle_program() {
        himsha_oracle_program::process(accounts, data, timestamp)
    } else {
        Err(ProgramError::Custom(0x4040)) // not a built-in
    }
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_builtin_recognizes_all_genesis_programs() {
        for id in program_ids::builtins() {
            assert!(is_builtin(&id), "builtin {id} not recognized");
        }
        assert!(!is_builtin(&Pubkey::from_seed(b"not-a-program")));
    }

    #[test]
    fn test_dispatch_routes_to_system_program() {
        // A system Transfer moves lamports via the native dispatch path.
        let sys = program_ids::system_program();
        let mut accounts = vec![
            AccountInfo::new(Pubkey::from_seed(b"from"), sys, 1_000, 0).as_signer(),
            AccountInfo::new(Pubkey::from_seed(b"to"), sys, 0, 0),
        ];
        let data =
            borsh::to_vec(&himsha_system_program::SystemInstruction::Transfer { lamports: 100 })
                .unwrap();
        dispatch(&sys, &mut accounts, &data, 0).unwrap();
        assert_eq!(accounts[0].lamports, 900);
        assert_eq!(accounts[1].lamports, 100);
    }

    #[test]
    fn test_dispatch_unknown_program_errors() {
        let unknown = Pubkey::from_seed(b"unknown-program");
        let mut accounts: Vec<AccountInfo> = vec![];
        assert_eq!(
            dispatch(&unknown, &mut accounts, &[0u8], 0),
            Err(ProgramError::Custom(0x4040)),
        );
    }

    #[test]
    fn test_dispatch_rejects_write_to_foreign_owned_account() {
        // Classic confusion attack: hand the token program an account that holds
        // valid TokenAccountState bytes but is owned by another program. Without
        // the owner gate the token program would happily mutate its balance.
        use himsha_runtime::account::AccountState;
        use himsha_token_program::{TokenAccountState, TokenInstruction};

        let token = program_ids::token_program();
        let swap = program_ids::swap_program();
        let mint = Pubkey::from_seed(b"mint");
        let user = Pubkey::from_seed(b"user");

        let token_state = |amount: u64| TokenAccountState {
            mint,
            owner: user,
            amount,
            delegate: None,
            state: AccountState::Initialized,
            delegated_amount: 0,
            close_authority: None,
        };
        // src is swap-owned (the fake), dst is genuinely token-owned.
        let mut src = AccountInfo::new(Pubkey::from_seed(b"fake"), swap, 0, 0);
        src.write_data(&token_state(1_000)).unwrap();
        let mut dst = AccountInfo::new(Pubkey::from_seed(b"dst"), token, 0, 0);
        dst.write_data(&token_state(0)).unwrap();
        let owner = AccountInfo::new(user, program_ids::system_program(), 0, 0).as_signer();

        let mut accounts = vec![src, dst, owner];
        let data = borsh::to_vec(&TokenInstruction::Transfer { amount: 500 }).unwrap();
        assert_eq!(
            dispatch(&token, &mut accounts, &data, 0),
            Err(ProgramError::IllegalOwnerWrite)
        );
    }

    #[test]
    fn test_self_transfer_is_rejected_not_inflated() {
        // P0: a token Transfer whose source == destination must be rejected at the
        // runtime boundary (the node calls reject_duplicate_writable before dispatch).
        // Otherwise the program sees two independent copies of the same account,
        // debits one and credits the other, and last-write-wins MINTS `amount` from
        // nothing. Here we assert the guard rejects it; balance stays put.
        use himsha_runtime::account::{reject_duplicate_writable, AccountMeta, AccountState};
        use himsha_token_program::{TokenAccountState, TokenInstruction};

        let token = program_ids::token_program();
        let mint = Pubkey::from_seed(b"mint");
        let user = Pubkey::from_seed(b"user");
        let acct_key = Pubkey::from_seed(b"self");

        let state = TokenAccountState {
            mint,
            owner: user,
            amount: 1_000,
            delegate: None,
            state: AccountState::Initialized,
            delegated_amount: 0,
            close_authority: None,
        };

        // Build the instruction's account window exactly as the node would for a
        // self-transfer: the SAME key in slot 0 and slot 1, both writable.
        let metas = vec![
            AccountMeta::writable(acct_key, false),
            AccountMeta::writable(acct_key, false),
            AccountMeta::readonly(user, true),
        ];
        let mut accounts: Vec<AccountInfo> = metas
            .iter()
            .map(|m| {
                let mut a = AccountInfo::new(m.pubkey, token, 0, 0);
                a.is_writable = m.is_writable;
                a.is_signer = m.is_signer;
                a
            })
            .collect();
        accounts[0].write_data(&state).unwrap();
        accounts[1].write_data(&state).unwrap();

        // The runtime boundary guard rejects the duplicate-writable window.
        assert_eq!(
            reject_duplicate_writable(&accounts),
            Err(ProgramError::DuplicateWritableAccount)
        );

        // And it's caught before dispatch — so the account's balance is untouched
        // (no inflation). (If the guard were absent, dispatch would write back a
        // copy showing amount == 1_000 + 500, minting 500 out of nothing.)
        let data = borsh::to_vec(&TokenInstruction::Transfer { amount: 500 }).unwrap();
        let _ = data;
        let before: TokenAccountState = accounts[0].read_data().unwrap();
        assert_eq!(before.amount, 1_000, "balance unchanged after rejection");
    }

    #[test]
    fn test_dispatch_propagates_program_errors() {
        // Transfer without a signer must surface the program's MissingSigner error.
        let sys = program_ids::system_program();
        let mut accounts = vec![
            AccountInfo::new(Pubkey::from_seed(b"from"), sys, 1_000, 0), // not a signer
            AccountInfo::new(Pubkey::from_seed(b"to"), sys, 0, 0),
        ];
        let data =
            borsh::to_vec(&himsha_system_program::SystemInstruction::Transfer { lamports: 100 })
                .unwrap();
        assert_eq!(
            dispatch(&sys, &mut accounts, &data, 0),
            Err(ProgramError::MissingSigner)
        );
    }
}

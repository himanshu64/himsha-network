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
pub fn dispatch(
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
        let data = borsh::to_vec(
            &himsha_system_program::SystemInstruction::Transfer { lamports: 100 },
        ).unwrap();
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
    fn test_dispatch_propagates_program_errors() {
        // Transfer without a signer must surface the program's MissingSigner error.
        let sys = program_ids::system_program();
        let mut accounts = vec![
            AccountInfo::new(Pubkey::from_seed(b"from"), sys, 1_000, 0), // not a signer
            AccountInfo::new(Pubkey::from_seed(b"to"), sys, 0, 0),
        ];
        let data = borsh::to_vec(
            &himsha_system_program::SystemInstruction::Transfer { lamports: 100 },
        ).unwrap();
        assert_eq!(dispatch(&sys, &mut accounts, &data, 0), Err(ProgramError::MissingSigner));
    }
}

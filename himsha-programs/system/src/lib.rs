//! HIMSHA System Program
//!
//! Responsibilities:
//!   - Creating new accounts (with optional Bitcoin UTXO anchor)
//!   - Transferring lamports between accounts
//!   - Reassigning account ownership to another program
//!   - Allocating additional data space in an account
//!
//! This program runs inside the RISC Zero zkVM guest.  The host
//! calls it via `ProgramExecutor::execute` and the resulting
//! `ExecutionOutput` updates the node's account database.

use borsh::{BorshDeserialize, BorshSerialize};
use himsha_runtime::{
    account::{AccountInfo, AccountMeta},
    error::ProgramError,
    instruction::Instruction,
    pubkey::Pubkey,
    utxo::UtxoMeta,
};

// ---- Instruction variants ----

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum SystemInstruction {
    /// Allocate a new account with `space` bytes of data, owned by `owner`.
    CreateAccount {
        lamports: u64,
        space: u64,
        owner: Pubkey,
    },
    /// Same as `CreateAccount` but anchors the new account to a Bitcoin UTXO.
    CreateAccountWithAnchor {
        utxo: UtxoMeta,
        space: u64,
        owner: Pubkey,
    },
    /// Move `lamports` from signer (accounts[0]) to recipient (accounts[1]).
    Transfer { lamports: u64 },
    /// Reassign `accounts[0]` to a different owning program.
    Assign { owner: Pubkey },
    /// Grow `accounts[0]`'s data buffer to at least `space` bytes.
    Allocate { space: u64 },
}

// ---- Instruction builders (used by SDK / CLI) ----

pub fn create_account(
    payer: Pubkey,
    new_account: Pubkey,
    lamports: u64,
    space: u64,
    owner: Pubkey,
) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::system_program(),
        vec![
            AccountMeta::writable(payer, true),
            AccountMeta::writable(new_account, true),
        ],
        &SystemInstruction::CreateAccount {
            lamports,
            space,
            owner,
        },
    )
}

pub fn create_account_with_anchor(
    payer: Pubkey,
    new_account: Pubkey,
    utxo: UtxoMeta,
    space: u64,
    owner: Pubkey,
) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::system_program(),
        vec![
            AccountMeta::writable(payer, true),
            AccountMeta::writable(new_account, true),
        ],
        &SystemInstruction::CreateAccountWithAnchor { utxo, space, owner },
    )
}

pub fn transfer(from: Pubkey, to: Pubkey, lamports: u64) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::system_program(),
        vec![
            AccountMeta::writable(from, true),
            AccountMeta::writable(to, false),
        ],
        &SystemInstruction::Transfer { lamports },
    )
}

pub fn assign(account: Pubkey, owner: Pubkey) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::system_program(),
        vec![AccountMeta::writable(account, true)],
        &SystemInstruction::Assign { owner },
    )
}

pub fn allocate(account: Pubkey, space: u64) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::system_program(),
        vec![AccountMeta::writable(account, true)],
        &SystemInstruction::Allocate { space },
    )
}

// ---- Core processing logic (runs inside zkVM guest) ----

pub fn process(accounts: &mut [AccountInfo], instruction_data: &[u8]) -> Result<(), ProgramError> {
    let ix = SystemInstruction::try_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstruction)?;

    match ix {
        SystemInstruction::CreateAccount {
            lamports,
            space,
            owner,
        } => {
            ensure_signers(accounts, 0)?;
            let payer = &mut accounts[0];
            deduct_lamports(payer, lamports)?;

            let new_acc = &mut accounts[1];
            if new_acc.lamports != 0 || !new_acc.data.is_empty() {
                return Err(ProgramError::AlreadyInitialized);
            }
            new_acc.lamports = lamports;
            new_acc.data = vec![0u8; space as usize];
            new_acc.owner = owner;
        }

        SystemInstruction::CreateAccountWithAnchor { utxo, space, owner } => {
            ensure_signers(accounts, 0)?;
            let _payer = &mut accounts[0];
            // No lamport transfer — UTXO value covers rent

            let new_acc = &mut accounts[1];
            if new_acc.lamports != 0 || !new_acc.data.is_empty() {
                return Err(ProgramError::AlreadyInitialized);
            }
            new_acc.data = vec![0u8; space as usize];
            new_acc.owner = owner;
            new_acc.utxo = Some(utxo);
        }

        SystemInstruction::Transfer { lamports } => {
            ensure_signers(accounts, 0)?;
            if accounts.len() < 2 {
                return Err(ProgramError::NotEnoughAccounts);
            }
            let _from_lamps = accounts[0].lamports;
            deduct_lamports(&mut accounts[0], lamports)?;
            accounts[1].lamports = accounts[1]
                .lamports
                .checked_add(lamports)
                .ok_or(ProgramError::Overflow)?;
        }

        SystemInstruction::Assign { owner } => {
            ensure_signers(accounts, 0)?;
            accounts[0].owner = owner;
        }

        SystemInstruction::Allocate { space } => {
            ensure_signers(accounts, 0)?;
            let current = accounts[0].data.len() as u64;
            if space > current {
                accounts[0].data.resize(space as usize, 0);
            }
        }
    }

    Ok(())
}

// ---- helpers ----

fn ensure_signers(accounts: &[AccountInfo], index: usize) -> Result<(), ProgramError> {
    // The node verifies Schnorr signatures and marks signer accounts before
    // execution; here we enforce that the required account actually signed.
    match accounts.get(index) {
        Some(acc) if acc.is_signer => Ok(()),
        Some(_) => Err(ProgramError::MissingSigner),
        None => Err(ProgramError::NotEnoughAccounts),
    }
}

fn deduct_lamports(account: &mut AccountInfo, amount: u64) -> Result<(), ProgramError> {
    account.lamports = account
        .lamports
        .checked_sub(amount)
        .ok_or(ProgramError::InsufficientFunds)?;
    Ok(())
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use himsha_runtime::{account::AccountInfo, pubkey::Pubkey};

    fn make_account(key: &str, owner: Pubkey, lamports: u64, space: usize) -> AccountInfo {
        AccountInfo::new(Pubkey::from_seed(key.as_bytes()), owner, lamports, space)
    }

    fn system_id() -> Pubkey {
        himsha_runtime::program_ids::system_program()
    }

    // ---- CreateAccount ----

    #[test]
    fn test_create_account_basic() {
        let _payer = Pubkey::from_seed(b"payer");
        let _new_acc = Pubkey::from_seed(b"new");
        let owner = Pubkey::from_seed(b"prog");
        let mut accounts = vec![
            make_account("payer", system_id(), 5_000_000, 0).as_signer(),
            make_account("new", system_id(), 0, 0),
        ];
        let ix = borsh::to_vec(&SystemInstruction::CreateAccount {
            lamports: 1_000_000,
            space: 128,
            owner,
        })
        .unwrap();
        process(&mut accounts, &ix).unwrap();
        assert_eq!(accounts[1].lamports, 1_000_000);
        assert_eq!(accounts[1].data.len(), 128);
        assert_eq!(accounts[1].owner, owner);
    }

    #[test]
    fn test_create_account_insufficient_lamports() {
        let owner = Pubkey::from_seed(b"prog");
        let mut accounts = vec![
            make_account("payer", system_id(), 500, 0).as_signer(), // only 500 lamports
            make_account("new", system_id(), 0, 0),
        ];
        let ix = borsh::to_vec(&SystemInstruction::CreateAccount {
            lamports: 1_000_000,
            space: 64,
            owner,
        })
        .unwrap();
        assert_eq!(
            process(&mut accounts, &ix),
            Err(ProgramError::InsufficientFunds)
        );
    }

    #[test]
    fn test_create_account_already_initialized() {
        let owner = Pubkey::from_seed(b"prog");
        let mut accounts = vec![
            make_account("payer", system_id(), 5_000_000, 0).as_signer(),
            make_account("new", owner, 1_000_000, 64), // already has lamports
        ];
        let ix = borsh::to_vec(&SystemInstruction::CreateAccount {
            lamports: 500_000,
            space: 64,
            owner,
        })
        .unwrap();
        assert_eq!(
            process(&mut accounts, &ix),
            Err(ProgramError::AlreadyInitialized)
        );
    }

    // ---- Transfer ----

    #[test]
    fn test_transfer_basic() {
        let mut accounts = vec![
            make_account("from", system_id(), 5_000_000, 0).as_signer(),
            make_account("to", system_id(), 0, 0),
        ];
        let ix = borsh::to_vec(&SystemInstruction::Transfer {
            lamports: 1_000_000,
        })
        .unwrap();
        process(&mut accounts, &ix).unwrap();
        assert_eq!(accounts[0].lamports, 4_000_000);
        assert_eq!(accounts[1].lamports, 1_000_000);
    }

    #[test]
    fn test_transfer_exact_balance() {
        let mut accounts = vec![
            make_account("from", system_id(), 1_000_000, 0).as_signer(),
            make_account("to", system_id(), 0, 0),
        ];
        let ix = borsh::to_vec(&SystemInstruction::Transfer {
            lamports: 1_000_000,
        })
        .unwrap();
        process(&mut accounts, &ix).unwrap();
        assert_eq!(accounts[0].lamports, 0);
        assert_eq!(accounts[1].lamports, 1_000_000);
    }

    #[test]
    fn test_transfer_insufficient() {
        let mut accounts = vec![
            make_account("from", system_id(), 500, 0).as_signer(),
            make_account("to", system_id(), 0, 0),
        ];
        let ix = borsh::to_vec(&SystemInstruction::Transfer { lamports: 1_000 }).unwrap();
        assert_eq!(
            process(&mut accounts, &ix),
            Err(ProgramError::InsufficientFunds)
        );
    }

    // ---- Assign ----

    #[test]
    fn test_assign_changes_owner() {
        let new_owner = Pubkey::from_seed(b"new-owner");
        let mut accounts = vec![make_account("acc", system_id(), 0, 0).as_signer()];
        let ix = borsh::to_vec(&SystemInstruction::Assign { owner: new_owner }).unwrap();
        process(&mut accounts, &ix).unwrap();
        assert_eq!(accounts[0].owner, new_owner);
    }

    // ---- Allocate ----

    #[test]
    fn test_allocate_grows_data() {
        let mut accounts = vec![make_account("acc", system_id(), 0, 0).as_signer()];
        let ix = borsh::to_vec(&SystemInstruction::Allocate { space: 256 }).unwrap();
        process(&mut accounts, &ix).unwrap();
        assert_eq!(accounts[0].data.len(), 256);
    }

    #[test]
    fn test_allocate_does_not_shrink() {
        let mut accounts = vec![
            make_account("acc", system_id(), 0, 512).as_signer(), // already 512 bytes
        ];
        let ix = borsh::to_vec(&SystemInstruction::Allocate { space: 128 }).unwrap();
        process(&mut accounts, &ix).unwrap();
        // Should stay at 512, not shrink
        assert_eq!(accounts[0].data.len(), 512);
    }

    // ---- CreateAccountWithAnchor ----

    #[test]
    fn test_create_account_with_anchor() {
        let owner = Pubkey::from_seed(b"prog");
        let utxo = UtxoMeta::new([0xaau8; 32], 1);
        let mut accounts = vec![
            make_account("payer", system_id(), 5_000_000, 0).as_signer(),
            make_account("new", system_id(), 0, 0),
        ];
        let ix = borsh::to_vec(&SystemInstruction::CreateAccountWithAnchor {
            utxo,
            space: 64,
            owner,
        })
        .unwrap();
        process(&mut accounts, &ix).unwrap();
        assert_eq!(accounts[1].utxo, Some(utxo));
        assert_eq!(accounts[1].data.len(), 64);
    }

    // ---- Invalid instruction ----

    #[test]
    fn test_invalid_instruction_data() {
        let mut accounts = vec![make_account("acc", system_id(), 0, 0).as_signer()];
        assert_eq!(
            process(&mut accounts, &[0xff, 0xff, 0xff]),
            Err(ProgramError::InvalidInstruction)
        );
    }
}

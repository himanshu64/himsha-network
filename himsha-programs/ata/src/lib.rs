//! HIMSHA Associated Token Account (ATA) Program
//!
//! Derives a deterministic token account address from (owner, mint).
//! This means every user has exactly one "canonical" account for each token.
//!
//! Address derivation:
//!   PDA([owner_key, token_program_id, mint_key], ata_program_id)

use borsh::{BorshDeserialize, BorshSerialize};
use himsha_runtime::{
    account::{AccountInfo, AccountMeta},
    cpi,
    error::ProgramError,
    instruction::Instruction,
    pubkey::Pubkey,
};
use himsha_token_program::TokenInstruction;

// ---- address derivation ----

/// Derive the ATA address for (owner, mint).  Deterministic and unique.
pub fn get_associated_token_address(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    let (pda, _) = Pubkey::find_program_address(
        &[
            owner.as_ref(),
            himsha_runtime::program_ids::token_program().as_ref(),
            mint.as_ref(),
        ],
        &himsha_runtime::program_ids::ata_program(),
    );
    pda
}

// ---- instructions ----

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum AtaInstruction {
    /// Create the ATA for `owner` + `mint` if it does not yet exist.
    /// accounts[0] = payer (signer, writable), accounts[1] = ATA (writable),
    /// accounts[2] = owner, accounts[3] = mint.
    Create,
}

pub fn create_associated_token_account(payer: Pubkey, owner: Pubkey, mint: Pubkey) -> Instruction {
    let ata = get_associated_token_address(&owner, &mint);
    Instruction::with_args(
        himsha_runtime::program_ids::ata_program(),
        vec![
            AccountMeta::writable(payer, true),
            AccountMeta::writable(ata, false),
            AccountMeta::readonly(owner, false),
            AccountMeta::readonly(mint, false),
        ],
        &AtaInstruction::Create,
    )
}

// ---- processing ----

pub fn process(accounts: &mut [AccountInfo], data: &[u8]) -> Result<(), ProgramError> {
    let ix = AtaInstruction::try_from_slice(data).map_err(|_| ProgramError::InvalidInstruction)?;

    match ix {
        AtaInstruction::Create => {
            if accounts.len() < 4 {
                return Err(ProgramError::NotEnoughAccounts);
            }

            let ata_key = accounts[1].key;
            let owner = accounts[2].key;
            let mint_key = accounts[3].key;

            // Verify the derived address matches accounts[1]
            let expected = get_associated_token_address(&owner, &mint_key);
            if expected != ata_key {
                return Err(ProgramError::InvalidAccountData);
            }

            // If already initialized, succeed idempotently
            if !accounts[1].data.is_empty() {
                return Ok(());
            }

            // Initialize the token account via CPI into the token program. The
            // ATA arrives blank, so the owner gate's first-writer rule hands it
            // to the token program — exactly where token accounts must live.
            let init_data = borsh::to_vec(&TokenInstruction::InitializeAccount)
                .map_err(|_| ProgramError::BorshError)?;
            cpi::invoke_indexed(
                accounts,
                &[1, 3],
                &init_data,
                &himsha_runtime::program_ids::token_program(),
                himsha_token_program::process,
            )?;
        }
    }
    Ok(())
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use himsha_runtime::account::AccountState;
    use himsha_token_program::{MintState, TokenAccountState};

    fn ata_prog() -> Pubkey {
        himsha_runtime::program_ids::ata_program()
    }

    fn acct(key: Pubkey, space: usize) -> AccountInfo {
        AccountInfo::new(key, ata_prog(), 0, space)
    }

    fn mint_account(mint_key: Pubkey, initialized: bool) -> AccountInfo {
        let mut a = AccountInfo::new(
            mint_key,
            himsha_runtime::program_ids::token_program(),
            0,
            128,
        );
        a.write_data(&MintState {
            is_initialized: initialized,
            ..Default::default()
        })
        .unwrap();
        a
    }

    #[test]
    fn test_address_is_deterministic_and_unique() {
        let owner = Pubkey::from_seed(b"owner");
        let mint = Pubkey::from_seed(b"mint");
        let a = get_associated_token_address(&owner, &mint);
        assert_eq!(a, get_associated_token_address(&owner, &mint)); // deterministic
        assert_ne!(
            a,
            get_associated_token_address(&Pubkey::from_seed(b"owner2"), &mint)
        );
        assert_ne!(
            a,
            get_associated_token_address(&owner, &Pubkey::from_seed(b"mint2"))
        );
    }

    #[test]
    fn test_create_initializes_ata() {
        let owner = Pubkey::from_seed(b"owner");
        let mint = Pubkey::from_seed(b"mint");
        let ata = get_associated_token_address(&owner, &mint);
        let mut accounts = vec![
            acct(Pubkey::from_seed(b"payer"), 0), // [0] payer
            acct(ata, 0),                         // [1] ata (empty)
            acct(owner, 0),                       // [2] owner
            mint_account(mint, true),             // [3] mint (initialized)
        ];
        let data = borsh::to_vec(&AtaInstruction::Create).unwrap();
        process(&mut accounts, &data).unwrap();

        let st: TokenAccountState = accounts[1].read_data().unwrap();
        assert_eq!(st.state, AccountState::Initialized);
        assert_eq!(st.mint, mint);
        // The owner gate's first-writer rule hands the fresh ATA to the token
        // program, so subsequent token-program writes to it are legal.
        assert_eq!(
            accounts[1].owner,
            himsha_runtime::program_ids::token_program()
        );
    }

    #[test]
    fn test_create_wrong_address_fails() {
        let owner = Pubkey::from_seed(b"owner");
        let mint = Pubkey::from_seed(b"mint");
        let mut accounts = vec![
            acct(Pubkey::from_seed(b"payer"), 0),
            acct(Pubkey::from_seed(b"not-the-ata"), 0), // wrong derived key
            acct(owner, 0),
            mint_account(mint, true),
        ];
        let data = borsh::to_vec(&AtaInstruction::Create).unwrap();
        assert_eq!(
            process(&mut accounts, &data),
            Err(ProgramError::InvalidAccountData)
        );
    }

    #[test]
    fn test_create_not_enough_accounts() {
        let mut accounts = vec![acct(Pubkey::from_seed(b"a"), 0)];
        let data = borsh::to_vec(&AtaInstruction::Create).unwrap();
        assert_eq!(
            process(&mut accounts, &data),
            Err(ProgramError::NotEnoughAccounts)
        );
    }

    #[test]
    fn test_create_idempotent_when_already_initialized() {
        let owner = Pubkey::from_seed(b"owner");
        let mint = Pubkey::from_seed(b"mint");
        let ata = get_associated_token_address(&owner, &mint);
        let mut ata_acc = acct(ata, 0);
        ata_acc.data = vec![1, 2, 3]; // non-empty → treated as already created
        let mut accounts = vec![
            acct(Pubkey::from_seed(b"payer"), 0),
            ata_acc,
            acct(owner, 0),
            mint_account(mint, true),
        ];
        let data = borsh::to_vec(&AtaInstruction::Create).unwrap();
        process(&mut accounts, &data).unwrap();
        assert_eq!(accounts[1].data, vec![1, 2, 3]); // unchanged
    }

    #[test]
    fn test_create_uninitialized_mint_fails() {
        let owner = Pubkey::from_seed(b"owner");
        let mint = Pubkey::from_seed(b"mint");
        let ata = get_associated_token_address(&owner, &mint);
        let mut accounts = vec![
            acct(Pubkey::from_seed(b"payer"), 0),
            acct(ata, 0),
            acct(owner, 0),
            mint_account(mint, false), // mint not initialized
        ];
        let data = borsh::to_vec(&AtaInstruction::Create).unwrap();
        assert_eq!(
            process(&mut accounts, &data),
            Err(ProgramError::NotInitialized)
        );
    }
}

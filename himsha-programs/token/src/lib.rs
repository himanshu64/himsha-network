//! HIMSHA Token Program — fungible tokens on Bitcoin via HIMSHA Network.
//!
//! Compatible with the SPL Token interface so existing Solana tooling
//! can be adapted easily.  State is stored in Borsh-encoded accounts.
//!
//! Account layout:
//!   Mint account   → `MintState`
//!   Token account  → `TokenAccountState`

use borsh::{BorshDeserialize, BorshSerialize};
use himsha_runtime::{
    account::{AccountInfo, AccountMeta, AccountState},
    error::ProgramError,
    instruction::Instruction,
    pubkey::Pubkey,
};

// ---- on-chain state ----

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct MintState {
    pub mint_authority: Option<Pubkey>,
    pub supply: u64,
    pub decimals: u8,
    pub is_initialized: bool,
    pub freeze_authority: Option<Pubkey>,
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct TokenAccountState {
    pub mint: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
    pub delegate: Option<Pubkey>,
    pub state: AccountState,
    pub delegated_amount: u64,
    pub close_authority: Option<Pubkey>,
}

// ---- instructions ----

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum TokenInstruction {
    /// Initialize a new token mint.  accounts[0] = mint (writable).
    InitializeMint {
        decimals: u8,
        mint_authority: Pubkey,
        freeze_authority: Option<Pubkey>,
    },
    /// Initialize a token account.  accounts[0] = token account, accounts[1] = mint.
    InitializeAccount,
    /// Mint `amount` new tokens.  accounts[0] = mint, accounts[1] = destination, accounts[2] = authority (signer).
    MintTo { amount: u64 },
    /// Transfer tokens.  accounts[0] = source, accounts[1] = destination, accounts[2] = owner/delegate (signer).
    Transfer { amount: u64 },
    /// Burn tokens.  accounts[0] = token account (writable), accounts[1] = mint, accounts[2] = owner (signer).
    Burn { amount: u64 },
    /// Approve a delegate.  accounts[0] = source, accounts[1] = delegate, accounts[2] = owner (signer).
    Approve { amount: u64 },
    /// Revoke delegate.  accounts[0] = source, accounts[1] = owner (signer).
    Revoke,
    /// Freeze account.  accounts[0] = token account, accounts[1] = freeze authority (signer).
    FreezeAccount,
    /// Thaw frozen account.
    ThawAccount,
    /// Close account and reclaim lamports.  accounts[0] = source, accounts[1] = destination, accounts[2] = owner.
    CloseAccount,
}

// ---- instruction builders ----

pub fn initialize_mint(
    mint: Pubkey,
    authority: Pubkey,
    decimals: u8,
    freeze_auth: Option<Pubkey>,
) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::token_program(),
        vec![AccountMeta::writable(mint, false)],
        &TokenInstruction::InitializeMint {
            decimals,
            mint_authority: authority,
            freeze_authority: freeze_auth,
        },
    )
}

pub fn initialize_account(token_account: Pubkey, mint: Pubkey, owner: Pubkey) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::token_program(),
        vec![
            AccountMeta::writable(token_account, false),
            AccountMeta::readonly(mint, false),
            AccountMeta::readonly(owner, false),
        ],
        &TokenInstruction::InitializeAccount,
    )
}

pub fn mint_to(mint: Pubkey, destination: Pubkey, authority: Pubkey, amount: u64) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::token_program(),
        vec![
            AccountMeta::writable(mint, false),
            AccountMeta::writable(destination, false),
            AccountMeta::readonly(authority, true),
        ],
        &TokenInstruction::MintTo { amount },
    )
}

pub fn transfer(source: Pubkey, destination: Pubkey, owner: Pubkey, amount: u64) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::token_program(),
        vec![
            AccountMeta::writable(source, false),
            AccountMeta::writable(destination, false),
            AccountMeta::readonly(owner, true),
        ],
        &TokenInstruction::Transfer { amount },
    )
}

pub fn burn(token_account: Pubkey, mint: Pubkey, owner: Pubkey, amount: u64) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::token_program(),
        vec![
            AccountMeta::writable(token_account, false),
            AccountMeta::writable(mint, false),
            AccountMeta::readonly(owner, true),
        ],
        &TokenInstruction::Burn { amount },
    )
}

// ---- processing (runs inside zkVM) ----

pub fn process(accounts: &mut [AccountInfo], data: &[u8]) -> Result<(), ProgramError> {
    let ix =
        TokenInstruction::try_from_slice(data).map_err(|_| ProgramError::InvalidInstruction)?;

    match ix {
        TokenInstruction::InitializeMint {
            decimals,
            mint_authority,
            freeze_authority,
        } => {
            let mint_acc = &mut accounts[0];
            let mut mint: MintState = mint_acc.read_data().unwrap_or_default();
            if mint.is_initialized {
                return Err(ProgramError::AlreadyInitialized);
            }
            mint.decimals = decimals;
            mint.mint_authority = Some(mint_authority);
            mint.freeze_authority = freeze_authority;
            mint.is_initialized = true;
            mint_acc.write_data(&mint)?;
        }

        TokenInstruction::InitializeAccount => {
            let mint_key = accounts[1].key;
            let mint_data = accounts[1].data.clone();
            let owner = accounts.get(2).map(|a| a.key).unwrap_or_default();

            // An empty / freshly-created mint account deserializes to the default
            // (uninitialized) state rather than erroring, so the caller gets the
            // more precise `NotInitialized` instead of `InvalidAccountData`.
            let mint: MintState = MintState::try_from_slice(&mint_data).unwrap_or_default();
            if !mint.is_initialized {
                return Err(ProgramError::NotInitialized);
            }

            let token_acc = &mut accounts[0];
            let mut state: TokenAccountState = token_acc.read_data().unwrap_or_default();
            if state.state != AccountState::Uninitialized {
                return Err(ProgramError::AlreadyInitialized);
            }
            state.mint = mint_key;
            state.owner = owner;
            state.state = AccountState::Initialized;
            token_acc.write_data(&state)?;
        }

        TokenInstruction::MintTo { amount } => {
            // accounts[2] = mint authority, must sign (a CPI caller signs for a
            // program-owned authority via cpi::invoke_signed_indexed).
            if accounts.len() < 3 || !accounts[2].is_signer {
                return Err(ProgramError::MissingSigner);
            }
            let mut mint: MintState = accounts[0].read_data()?;
            let mut dest: TokenAccountState = accounts[1].read_data()?;

            if dest.mint != accounts[0].key {
                return Err(ProgramError::InvalidAccountData);
            }
            if dest.state == AccountState::Frozen {
                return Err(ProgramError::Unauthorized);
            }

            mint.supply = mint
                .supply
                .checked_add(amount)
                .ok_or(ProgramError::Overflow)?;
            dest.amount = dest
                .amount
                .checked_add(amount)
                .ok_or(ProgramError::Overflow)?;

            accounts[0].write_data(&mint)?;
            accounts[1].write_data(&dest)?;
        }

        TokenInstruction::Transfer { amount } => {
            // accounts[2] = owner/delegate, must sign.
            if accounts.len() < 3 || !accounts[2].is_signer {
                return Err(ProgramError::MissingSigner);
            }
            let mut source: TokenAccountState = accounts[0].read_data()?;
            let mut dest: TokenAccountState = accounts[1].read_data()?;

            if source.mint != dest.mint {
                return Err(ProgramError::InvalidAccountData);
            }
            if source.state == AccountState::Frozen || dest.state == AccountState::Frozen {
                return Err(ProgramError::Unauthorized);
            }
            source.amount = source
                .amount
                .checked_sub(amount)
                .ok_or(ProgramError::InsufficientFunds)?;
            dest.amount = dest
                .amount
                .checked_add(amount)
                .ok_or(ProgramError::Overflow)?;

            accounts[0].write_data(&source)?;
            accounts[1].write_data(&dest)?;
        }

        TokenInstruction::Burn { amount } => {
            // accounts[2] = owner, must sign.
            if accounts.len() < 3 || !accounts[2].is_signer {
                return Err(ProgramError::MissingSigner);
            }
            let mut token: TokenAccountState = accounts[0].read_data()?;
            let mut mint: MintState = accounts[1].read_data()?;

            if token.state == AccountState::Frozen {
                return Err(ProgramError::Unauthorized);
            }
            token.amount = token
                .amount
                .checked_sub(amount)
                .ok_or(ProgramError::InsufficientFunds)?;
            mint.supply = mint
                .supply
                .checked_sub(amount)
                .ok_or(ProgramError::InsufficientFunds)?;

            accounts[0].write_data(&token)?;
            accounts[1].write_data(&mint)?;
        }

        TokenInstruction::Approve { amount } => {
            let mut token: TokenAccountState = accounts[0].read_data()?;
            token.delegate = Some(accounts[1].key);
            token.delegated_amount = amount;
            accounts[0].write_data(&token)?;
        }

        TokenInstruction::Revoke => {
            let mut token: TokenAccountState = accounts[0].read_data()?;
            token.delegate = None;
            token.delegated_amount = 0;
            accounts[0].write_data(&token)?;
        }

        TokenInstruction::FreezeAccount => {
            let mut token: TokenAccountState = accounts[0].read_data()?;
            token.state = AccountState::Frozen;
            accounts[0].write_data(&token)?;
        }

        TokenInstruction::ThawAccount => {
            let mut token: TokenAccountState = accounts[0].read_data()?;
            if token.state != AccountState::Frozen {
                return Err(ProgramError::InvalidAccountData);
            }
            token.state = AccountState::Initialized;
            accounts[0].write_data(&token)?;
        }

        TokenInstruction::CloseAccount => {
            let token: TokenAccountState = accounts[0].read_data()?;
            if token.amount != 0 {
                return Err(ProgramError::InsufficientFunds);
            }
            accounts[1].lamports += accounts[0].lamports;
            accounts[0].lamports = 0;
            accounts[0].data.clear();
        }
    }

    Ok(())
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use himsha_runtime::{account::AccountInfo, pubkey::Pubkey};

    fn token_prog() -> Pubkey {
        himsha_runtime::program_ids::token_program()
    }

    fn mint_account(key: &str) -> AccountInfo {
        AccountInfo::new(
            Pubkey::from_seed(key.as_bytes()),
            token_prog(),
            2_000_000,
            128,
        )
    }
    fn token_account(key: &str) -> AccountInfo {
        AccountInfo::new(
            Pubkey::from_seed(key.as_bytes()),
            token_prog(),
            2_000_000,
            256,
        )
    }

    // ---- InitializeMint ----

    #[test]
    fn test_initialize_mint_basic() {
        let authority = Pubkey::from_seed(b"auth");
        let mut accounts = vec![mint_account("mint")];
        let ix = borsh::to_vec(&TokenInstruction::InitializeMint {
            decimals: 6,
            mint_authority: authority,
            freeze_authority: None,
        })
        .unwrap();
        process(&mut accounts, &ix).unwrap();
        let mint: MintState = accounts[0].read_data().unwrap();
        assert!(mint.is_initialized);
        assert_eq!(mint.decimals, 6);
        assert_eq!(mint.mint_authority, Some(authority));
        assert_eq!(mint.freeze_authority, None);
        assert_eq!(mint.supply, 0);
    }

    #[test]
    fn test_initialize_mint_with_freeze() {
        let auth = Pubkey::from_seed(b"auth");
        let freeze = Pubkey::from_seed(b"freeze");
        let mut accounts = vec![mint_account("mint")];
        let ix = borsh::to_vec(&TokenInstruction::InitializeMint {
            decimals: 8,
            mint_authority: auth,
            freeze_authority: Some(freeze),
        })
        .unwrap();
        process(&mut accounts, &ix).unwrap();
        let mint: MintState = accounts[0].read_data().unwrap();
        assert_eq!(mint.freeze_authority, Some(freeze));
    }

    #[test]
    fn test_initialize_mint_already_initialized() {
        let auth = Pubkey::from_seed(b"auth");
        let mut accounts = vec![mint_account("mint")];
        let ix = borsh::to_vec(&TokenInstruction::InitializeMint {
            decimals: 6,
            mint_authority: auth,
            freeze_authority: None,
        })
        .unwrap();
        process(&mut accounts, &ix).unwrap();
        // Second init should fail
        assert_eq!(
            process(&mut accounts, &ix),
            Err(ProgramError::AlreadyInitialized)
        );
    }

    // ---- InitializeAccount ----

    fn setup_mint_and_account() -> (Vec<AccountInfo>, Pubkey) {
        let auth = Pubkey::from_seed(b"auth");
        let owner = Pubkey::from_seed(b"owner");
        let mut mint_acc = mint_account("mint");
        let _mint_key = mint_acc.key;
        let state = MintState {
            mint_authority: Some(auth),
            supply: 0,
            decimals: 6,
            is_initialized: true,
            freeze_authority: None,
        };
        mint_acc.write_data(&state).unwrap();

        let mut token_acc = token_account("token");
        token_acc.key = Pubkey::from_seed(b"token");

        (
            vec![
                token_acc,
                mint_acc,
                AccountInfo::new(owner, token_prog(), 0, 0),
            ],
            auth,
        )
    }

    #[test]
    fn test_initialize_account() {
        let (mut accounts, _) = setup_mint_and_account();
        let ix = borsh::to_vec(&TokenInstruction::InitializeAccount).unwrap();
        process(&mut accounts, &ix).unwrap();
        let state: TokenAccountState = accounts[0].read_data().unwrap();
        assert_eq!(state.state, AccountState::Initialized);
        assert_eq!(state.amount, 0);
    }

    #[test]
    fn test_initialize_account_uninitialized_mint() {
        let mut accounts = vec![
            token_account("token"),
            mint_account("mint"), // NOT initialized
            AccountInfo::new(Pubkey::from_seed(b"owner"), token_prog(), 0, 0),
        ];
        let ix = borsh::to_vec(&TokenInstruction::InitializeAccount).unwrap();
        assert_eq!(
            process(&mut accounts, &ix),
            Err(ProgramError::NotInitialized)
        );
    }

    // ---- MintTo ----

    #[test]
    fn test_mint_to_increases_supply() {
        let (mut accounts, auth) = setup_mint_and_account();
        let init_ix = borsh::to_vec(&TokenInstruction::InitializeAccount).unwrap();
        process(&mut accounts, &init_ix).unwrap();

        // Rearrange for MintTo: [mint, token, authority]
        let mint_acc = accounts[1].clone();
        let token_acc = accounts[0].clone();
        let auth_acc = AccountInfo::new(auth, token_prog(), 0, 0).as_signer();
        let mut mint_accounts = vec![mint_acc, token_acc, auth_acc];

        let mint_ix = borsh::to_vec(&TokenInstruction::MintTo { amount: 1_000_000 }).unwrap();
        process(&mut mint_accounts, &mint_ix).unwrap();

        let mint: MintState = mint_accounts[0].read_data().unwrap();
        let token: TokenAccountState = mint_accounts[1].read_data().unwrap();
        assert_eq!(mint.supply, 1_000_000);
        assert_eq!(token.amount, 1_000_000);
    }

    // ---- Transfer ----

    #[test]
    fn test_transfer_tokens() {
        let mint_key = Pubkey::from_seed(b"mint");
        let mut src = token_account("src");
        let mut dst = token_account("dst");

        // Pre-set both as initialized with same mint
        let src_state = TokenAccountState {
            mint: mint_key,
            owner: Pubkey::from_seed(b"alice"),
            amount: 500,
            state: AccountState::Initialized,
            ..Default::default()
        };
        let dst_state = TokenAccountState {
            mint: mint_key,
            owner: Pubkey::from_seed(b"bob"),
            amount: 0,
            state: AccountState::Initialized,
            ..Default::default()
        };
        src.write_data(&src_state).unwrap();
        dst.write_data(&dst_state).unwrap();

        let owner = AccountInfo::new(Pubkey::from_seed(b"alice"), token_prog(), 0, 0).as_signer();
        let mut accounts = vec![src, dst, owner];
        let ix = borsh::to_vec(&TokenInstruction::Transfer { amount: 200 }).unwrap();
        process(&mut accounts, &ix).unwrap();

        let new_src: TokenAccountState = accounts[0].read_data().unwrap();
        let new_dst: TokenAccountState = accounts[1].read_data().unwrap();
        assert_eq!(new_src.amount, 300);
        assert_eq!(new_dst.amount, 200);
    }

    #[test]
    fn test_transfer_insufficient_balance() {
        let mint_key = Pubkey::from_seed(b"mint");
        let mut src = token_account("src");
        let mut dst = token_account("dst");
        let src_state = TokenAccountState {
            mint: mint_key,
            amount: 10,
            state: AccountState::Initialized,
            ..Default::default()
        };
        let dst_state = TokenAccountState {
            mint: mint_key,
            amount: 0,
            state: AccountState::Initialized,
            ..Default::default()
        };
        src.write_data(&src_state).unwrap();
        dst.write_data(&dst_state).unwrap();
        let owner = AccountInfo::new(Pubkey::default(), token_prog(), 0, 0).as_signer();
        let mut accounts = vec![src, dst, owner];
        let ix = borsh::to_vec(&TokenInstruction::Transfer { amount: 100 }).unwrap();
        assert_eq!(
            process(&mut accounts, &ix),
            Err(ProgramError::InsufficientFunds)
        );
    }

    #[test]
    fn test_transfer_frozen_source() {
        let mint_key = Pubkey::from_seed(b"mint");
        let mut src = token_account("src");
        let mut dst = token_account("dst");
        let src_state = TokenAccountState {
            mint: mint_key,
            amount: 500,
            state: AccountState::Frozen,
            ..Default::default()
        };
        let dst_state = TokenAccountState {
            mint: mint_key,
            amount: 0,
            state: AccountState::Initialized,
            ..Default::default()
        };
        src.write_data(&src_state).unwrap();
        dst.write_data(&dst_state).unwrap();
        let owner = AccountInfo::new(Pubkey::default(), token_prog(), 0, 0).as_signer();
        let mut accounts = vec![src, dst, owner];
        let ix = borsh::to_vec(&TokenInstruction::Transfer { amount: 100 }).unwrap();
        assert_eq!(process(&mut accounts, &ix), Err(ProgramError::Unauthorized));
    }

    // ---- Burn ----

    #[test]
    fn test_burn_reduces_supply() {
        let mint_key = Pubkey::from_seed(b"mint");
        let mut token_acc = token_account("token");
        let mut mint_acc = mint_account("mint");
        mint_acc.key = mint_key;

        let mint_state = MintState {
            supply: 1_000,
            is_initialized: true,
            ..Default::default()
        };
        let tok_state = TokenAccountState {
            mint: mint_key,
            amount: 1_000,
            state: AccountState::Initialized,
            ..Default::default()
        };
        mint_acc.write_data(&mint_state).unwrap();
        token_acc.write_data(&tok_state).unwrap();

        let owner = AccountInfo::new(Pubkey::default(), token_prog(), 0, 0).as_signer();
        let mut accounts = vec![token_acc, mint_acc, owner];
        let ix = borsh::to_vec(&TokenInstruction::Burn { amount: 400 }).unwrap();
        process(&mut accounts, &ix).unwrap();

        let new_mint: MintState = accounts[1].read_data().unwrap();
        let new_tok: TokenAccountState = accounts[0].read_data().unwrap();
        assert_eq!(new_mint.supply, 600);
        assert_eq!(new_tok.amount, 600);
    }

    #[test]
    fn test_burn_more_than_balance() {
        let mint_key = Pubkey::from_seed(b"mint");
        let mut token_acc = token_account("token");
        let mut mint_acc = mint_account("mint");
        mint_acc.key = mint_key;
        let mint_state = MintState {
            supply: 100,
            is_initialized: true,
            ..Default::default()
        };
        let tok_state = TokenAccountState {
            mint: mint_key,
            amount: 50,
            state: AccountState::Initialized,
            ..Default::default()
        };
        mint_acc.write_data(&mint_state).unwrap();
        token_acc.write_data(&tok_state).unwrap();
        let owner = AccountInfo::new(Pubkey::default(), token_prog(), 0, 0).as_signer();
        let mut accounts = vec![token_acc, mint_acc, owner];
        let ix = borsh::to_vec(&TokenInstruction::Burn { amount: 100 }).unwrap();
        assert_eq!(
            process(&mut accounts, &ix),
            Err(ProgramError::InsufficientFunds)
        );
    }

    // ---- FreezeAccount / ThawAccount ----

    #[test]
    fn test_freeze_and_thaw() {
        let mint_key = Pubkey::from_seed(b"mint");
        let mut token_acc = token_account("token");
        let freeze_auth = Pubkey::from_seed(b"freeze");
        let tok_state = TokenAccountState {
            mint: mint_key,
            amount: 100,
            state: AccountState::Initialized,
            ..Default::default()
        };
        token_acc.write_data(&tok_state).unwrap();

        let auth_acc = AccountInfo::new(freeze_auth, token_prog(), 0, 0);

        // Freeze
        let mut accounts = vec![token_acc.clone(), auth_acc.clone()];
        let freeze_ix = borsh::to_vec(&TokenInstruction::FreezeAccount).unwrap();
        process(&mut accounts, &freeze_ix).unwrap();
        let state: TokenAccountState = accounts[0].read_data().unwrap();
        assert_eq!(state.state, AccountState::Frozen);

        // Thaw
        let thaw_ix = borsh::to_vec(&TokenInstruction::ThawAccount).unwrap();
        process(&mut accounts, &thaw_ix).unwrap();
        let state2: TokenAccountState = accounts[0].read_data().unwrap();
        assert_eq!(state2.state, AccountState::Initialized);
    }

    #[test]
    fn test_thaw_non_frozen_fails() {
        let mint_key = Pubkey::from_seed(b"mint");
        let mut token_acc = token_account("token");
        let tok_state = TokenAccountState {
            mint: mint_key,
            amount: 0,
            state: AccountState::Initialized,
            ..Default::default()
        };
        token_acc.write_data(&tok_state).unwrap();
        let auth_acc = AccountInfo::new(Pubkey::default(), token_prog(), 0, 0);
        let mut accounts = vec![token_acc, auth_acc];
        let ix = borsh::to_vec(&TokenInstruction::ThawAccount).unwrap();
        assert_eq!(
            process(&mut accounts, &ix),
            Err(ProgramError::InvalidAccountData)
        );
    }

    // ---- Approve / Revoke ----

    #[test]
    fn test_approve_and_revoke() {
        let mint_key = Pubkey::from_seed(b"mint");
        let delegate = Pubkey::from_seed(b"delegate");
        let mut token_acc = token_account("token");
        let tok_state = TokenAccountState {
            mint: mint_key,
            amount: 1000,
            state: AccountState::Initialized,
            ..Default::default()
        };
        token_acc.write_data(&tok_state).unwrap();

        let delegate_acc = AccountInfo::new(delegate, token_prog(), 0, 0);
        let owner_acc = AccountInfo::new(Pubkey::default(), token_prog(), 0, 0);

        // Approve
        let mut accounts = vec![token_acc.clone(), delegate_acc, owner_acc];
        let approve_ix = borsh::to_vec(&TokenInstruction::Approve { amount: 500 }).unwrap();
        process(&mut accounts, &approve_ix).unwrap();
        let state: TokenAccountState = accounts[0].read_data().unwrap();
        assert_eq!(state.delegate, Some(delegate));
        assert_eq!(state.delegated_amount, 500);

        // Revoke
        let revoke_ix = borsh::to_vec(&TokenInstruction::Revoke).unwrap();
        process(&mut accounts, &revoke_ix).unwrap();
        let state2: TokenAccountState = accounts[0].read_data().unwrap();
        assert_eq!(state2.delegate, None);
        assert_eq!(state2.delegated_amount, 0);
    }
}

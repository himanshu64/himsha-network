//! HIMSHA Runes Program — Bitcoin Runes-style fungible tokens.
//!
//! Models the core of the Bitcoin Runes protocol on the HIMSHA account model:
//!
//!   - **Etch**     — define a new rune (name, symbol, divisibility, optional
//!                    premine, optional open-mint *terms*).
//!   - **Mint**     — anyone mints `terms.amount` while the open-mint cap and
//!                    height window allow it.
//!   - **Transfer** — move balance between two rune-balance accounts (the HIMSHA
//!                    analogue of a Runestone *edict*).
//!   - **Burn**     — permanently destroy balance, reducing circulating supply.
//!
//! State design (borsh, mirrors the token program):
//!   `RuneEtching`  — one per rune, stored in the rune account.
//!   `RuneBalance`  — per-holder balance account (like an ATA).
//!
//! Differences from real Bitcoin Runes:
//!   - No Runestone OP_RETURN parsing — instructions are explicit.
//!   - "Block height" windows use `Message.timestamp` passed by the node.
//!   - Cenotaph / edict-pointer semantics are out of scope for this PoC.

use borsh::{BorshDeserialize, BorshSerialize};
use himsha_runtime::{
    account::{AccountInfo, AccountMeta},
    error::ProgramError,
    instruction::Instruction,
    pubkey::Pubkey,
};

// ---- on-chain state ----

/// Open-mint terms. When present, anyone may `Mint` until `cap` mints happen
/// (or, if set, outside the `[start, end]` timestamp window).
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MintTerms {
    /// Amount minted per `Mint` call (in base units).
    pub amount: u64,
    /// Maximum number of mints allowed. 0 means unlimited.
    pub cap: u64,
    /// Number of mints performed so far.
    pub mints: u64,
    /// Optional unix-timestamp open height (0 = no lower bound).
    pub start: u64,
    /// Optional unix-timestamp close height (0 = no upper bound).
    pub end: u64,
}

/// A single etched rune.
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct RuneEtching {
    /// Spaced display name, e.g. "UNCOMMON•GOODS".
    pub name: String,
    /// Single-char currency symbol, e.g. '¤'. Stored as u32 codepoint.
    pub symbol: u32,
    /// Decimal places (0..=38 in real Runes; capped here).
    pub divisibility: u8,
    /// Authority allowed to set/replace mint terms (the etcher).
    pub etcher: Pubkey,
    /// Amount minted to the etcher at creation.
    pub premine: u64,
    /// Optional open-mint terms.
    pub terms: Option<MintTerms>,
    /// Total minted so far (premine + all mints), before burns.
    pub minted: u64,
    /// Total burned so far.
    pub burned: u64,
    pub is_etched: bool,
}

impl RuneEtching {
    /// Circulating supply = everything minted minus everything burned.
    pub fn circulating(&self) -> u64 {
        self.minted.saturating_sub(self.burned)
    }
}

/// A per-holder balance of one rune.
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct RuneBalance {
    pub rune: Pubkey,
    pub owner: Pubkey,
    pub amount: u64,
    pub is_initialized: bool,
}

// ---- instructions ----

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum RuneInstruction {
    /// Etch (create) a new rune.
    /// accounts[0] = rune account (writable),
    /// accounts[1] = etcher balance account (writable, receives premine),
    /// accounts[2] = etcher (signer).
    Etch {
        name: String,
        symbol: u32,
        divisibility: u8,
        premine: u64,
        terms: Option<MintTerms>,
    },

    /// Open-mint: mint `terms.amount` to the destination balance.
    /// accounts[0] = rune account (writable),
    /// accounts[1] = destination balance (writable),
    /// accounts[2] = minter (signer).
    Mint,

    /// Transfer `amount` between two balance accounts of the same rune.
    /// accounts[0] = source balance (writable),
    /// accounts[1] = destination balance (writable),
    /// accounts[2] = owner (signer).
    Transfer { amount: u64 },

    /// Burn `amount` from a balance account, reducing circulating supply.
    /// accounts[0] = rune account (writable),
    /// accounts[1] = source balance (writable),
    /// accounts[2] = owner (signer).
    Burn { amount: u64 },

    /// Initialize an empty balance account for `rune`.
    /// accounts[0] = balance account (writable), accounts[1] = owner.
    InitBalance,
}

const MAX_DIVISIBILITY: u8 = 38;

// ---- instruction builders ----

pub fn etch(
    rune: Pubkey, etcher_balance: Pubkey, etcher: Pubkey,
    name: String, symbol: u32, divisibility: u8, premine: u64, terms: Option<MintTerms>,
) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::runes_program(),
        vec![
            AccountMeta::writable(rune, false),
            AccountMeta::writable(etcher_balance, false),
            AccountMeta::readonly(etcher, true),
        ],
        &RuneInstruction::Etch { name, symbol, divisibility, premine, terms },
    )
}

pub fn mint(rune: Pubkey, destination: Pubkey, minter: Pubkey) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::runes_program(),
        vec![
            AccountMeta::writable(rune, false),
            AccountMeta::writable(destination, false),
            AccountMeta::readonly(minter, true),
        ],
        &RuneInstruction::Mint,
    )
}

pub fn transfer(source: Pubkey, destination: Pubkey, owner: Pubkey, amount: u64) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::runes_program(),
        vec![
            AccountMeta::writable(source, false),
            AccountMeta::writable(destination, false),
            AccountMeta::readonly(owner, true),
        ],
        &RuneInstruction::Transfer { amount },
    )
}

pub fn burn(rune: Pubkey, source: Pubkey, owner: Pubkey, amount: u64) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::runes_program(),
        vec![
            AccountMeta::writable(rune, false),
            AccountMeta::writable(source, false),
            AccountMeta::readonly(owner, true),
        ],
        &RuneInstruction::Burn { amount },
    )
}

pub fn init_balance(balance: Pubkey, owner: Pubkey) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::runes_program(),
        vec![
            AccountMeta::writable(balance, false),
            AccountMeta::readonly(owner, false),
        ],
        &RuneInstruction::InitBalance,
    )
}

// ---- processing ----

pub fn process(accounts: &mut [AccountInfo], data: &[u8], timestamp: u64) -> Result<(), ProgramError> {
    let ix = RuneInstruction::try_from_slice(data)
        .map_err(|_| ProgramError::InvalidInstruction)?;

    match ix {
        RuneInstruction::Etch { name, symbol, divisibility, premine, terms } => {
            if accounts.len() < 3 { return Err(ProgramError::NotEnoughAccounts); }
            accounts[2].require_signer()?; // etcher/minter/owner must sign
            if name.is_empty() { return Err(ProgramError::InvalidInstruction); }
            if divisibility > MAX_DIVISIBILITY { return Err(ProgramError::InvalidInstruction); }

            let rune_key = accounts[0].key;
            let etcher = accounts[2].key;

            let mut rune: RuneEtching = accounts[0].read_data().unwrap_or_default();
            if rune.is_etched { return Err(ProgramError::AlreadyInitialized); }

            rune.name = name;
            rune.symbol = symbol;
            rune.divisibility = divisibility;
            rune.etcher = etcher;
            rune.premine = premine;
            rune.terms = terms;
            rune.minted = premine;
            rune.burned = 0;
            rune.is_etched = true;
            accounts[0].write_data(&rune)?;

            // Credit the premine to the etcher's balance account.
            if premine > 0 {
                let mut bal: RuneBalance = accounts[1].read_data().unwrap_or_default();
                bal.rune = rune_key;
                bal.owner = etcher;
                bal.amount = bal.amount.checked_add(premine).ok_or(ProgramError::Overflow)?;
                bal.is_initialized = true;
                accounts[1].write_data(&bal)?;
            }
        }

        RuneInstruction::Mint => {
            if accounts.len() < 3 { return Err(ProgramError::NotEnoughAccounts); }
            accounts[2].require_signer()?; // etcher/minter/owner must sign

            let rune_key = accounts[0].key;
            let mut rune: RuneEtching = accounts[0].read_data()?;
            if !rune.is_etched { return Err(ProgramError::NotInitialized); }

            let mut terms = rune.terms.clone().ok_or(ProgramError::Unauthorized)?;

            // Enforce the mint cap.
            if terms.cap != 0 && terms.mints >= terms.cap {
                return Err(ProgramError::Unauthorized);
            }
            // Enforce the open window (timestamps; 0 = unbounded).
            if terms.start != 0 && timestamp < terms.start { return Err(ProgramError::Unauthorized); }
            if terms.end != 0 && timestamp > terms.end { return Err(ProgramError::LoanExpired); }

            let amount = terms.amount;
            terms.mints = terms.mints.checked_add(1).ok_or(ProgramError::Overflow)?;
            rune.terms = Some(terms);
            rune.minted = rune.minted.checked_add(amount).ok_or(ProgramError::Overflow)?;
            accounts[0].write_data(&rune)?;

            let mut bal: RuneBalance = accounts[1].read_data().unwrap_or_default();
            bal.rune = rune_key;
            if bal.owner == Pubkey::default() { bal.owner = accounts[2].key; }
            bal.amount = bal.amount.checked_add(amount).ok_or(ProgramError::Overflow)?;
            bal.is_initialized = true;
            accounts[1].write_data(&bal)?;
        }

        RuneInstruction::Transfer { amount } => {
            if accounts.len() < 3 { return Err(ProgramError::NotEnoughAccounts); }
            accounts[2].require_signer()?; // etcher/minter/owner must sign

            let mut src: RuneBalance = accounts[0].read_data()?;
            if !src.is_initialized { return Err(ProgramError::NotInitialized); }
            let mut dst: RuneBalance = accounts[1].read_data().unwrap_or_default();

            // Destination must hold the same rune (or be fresh).
            if dst.is_initialized && dst.rune != src.rune {
                return Err(ProgramError::InvalidAccountData);
            }

            src.amount = src.amount.checked_sub(amount).ok_or(ProgramError::InsufficientFunds)?;
            dst.rune = src.rune;
            if dst.owner == Pubkey::default() { dst.owner = accounts[1].key; }
            dst.amount = dst.amount.checked_add(amount).ok_or(ProgramError::Overflow)?;
            dst.is_initialized = true;

            accounts[0].write_data(&src)?;
            accounts[1].write_data(&dst)?;
        }

        RuneInstruction::Burn { amount } => {
            if accounts.len() < 3 { return Err(ProgramError::NotEnoughAccounts); }
            accounts[2].require_signer()?; // etcher/minter/owner must sign

            let mut rune: RuneEtching = accounts[0].read_data()?;
            let mut src: RuneBalance = accounts[1].read_data()?;
            if !src.is_initialized { return Err(ProgramError::NotInitialized); }

            src.amount = src.amount.checked_sub(amount).ok_or(ProgramError::InsufficientFunds)?;
            rune.burned = rune.burned.checked_add(amount).ok_or(ProgramError::Overflow)?;

            accounts[0].write_data(&rune)?;
            accounts[1].write_data(&src)?;
        }

        RuneInstruction::InitBalance => {
            if accounts.len() < 2 { return Err(ProgramError::NotEnoughAccounts); }
            let owner = accounts[1].key;
            let mut bal: RuneBalance = accounts[0].read_data().unwrap_or_default();
            if bal.is_initialized { return Err(ProgramError::AlreadyInitialized); }
            bal.owner = owner;
            bal.is_initialized = true;
            accounts[0].write_data(&bal)?;
        }
    }

    Ok(())
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn prog() -> Pubkey { himsha_runtime::program_ids::runes_program() }

    fn acc(seed: &str, space: usize) -> AccountInfo {
        AccountInfo::new(Pubkey::from_seed(seed.as_bytes()), prog(), 0, space)
    }

    fn etch_accounts() -> Vec<AccountInfo> {
        // accounts[2] (etcher/minter/owner) signs every rune action.
        vec![acc("rune", 512), acc("etcher-bal", 256), acc("etcher", 0).as_signer()]
    }

    fn etched(premine: u64, terms: Option<MintTerms>) -> Vec<AccountInfo> {
        let mut accounts = etch_accounts();
        let ix = borsh::to_vec(&RuneInstruction::Etch {
            name: "UNCOMMON•GOODS".into(),
            symbol: '¤' as u32,
            divisibility: 2,
            premine,
            terms,
        }).unwrap();
        process(&mut accounts, &ix, 1_000).unwrap();
        accounts
    }

    #[test]
    fn test_etch_with_premine() {
        let accounts = etched(1_000, None);
        let rune: RuneEtching = accounts[0].read_data().unwrap();
        assert!(rune.is_etched);
        assert_eq!(rune.minted, 1_000);
        assert_eq!(rune.circulating(), 1_000);
        let bal: RuneBalance = accounts[1].read_data().unwrap();
        assert_eq!(bal.amount, 1_000);
    }

    #[test]
    fn test_etch_twice_fails() {
        let mut accounts = etched(0, None);
        let ix = borsh::to_vec(&RuneInstruction::Etch {
            name: "X".into(), symbol: 0, divisibility: 0, premine: 0, terms: None,
        }).unwrap();
        assert_eq!(process(&mut accounts, &ix, 1_000), Err(ProgramError::AlreadyInitialized));
    }

    #[test]
    fn test_etch_divisibility_too_high() {
        let mut accounts = etch_accounts();
        let ix = borsh::to_vec(&RuneInstruction::Etch {
            name: "X".into(), symbol: 0, divisibility: 39, premine: 0, terms: None,
        }).unwrap();
        assert_eq!(process(&mut accounts, &ix, 1_000), Err(ProgramError::InvalidInstruction));
    }

    #[test]
    fn test_open_mint_respects_cap() {
        let terms = MintTerms { amount: 100, cap: 2, mints: 0, start: 0, end: 0 };
        let mut accounts = etched(0, Some(terms));
        // reuse balance slot index 1 as the mint destination
        let mint_ix = borsh::to_vec(&RuneInstruction::Mint).unwrap();
        process(&mut accounts, &mint_ix, 2_000).unwrap();
        process(&mut accounts, &mint_ix, 2_000).unwrap();
        // third mint exceeds cap
        assert_eq!(process(&mut accounts, &mint_ix, 2_000), Err(ProgramError::Unauthorized));

        let rune: RuneEtching = accounts[0].read_data().unwrap();
        assert_eq!(rune.minted, 200);
        let bal: RuneBalance = accounts[1].read_data().unwrap();
        assert_eq!(bal.amount, 200);
    }

    #[test]
    fn test_mint_without_terms_fails() {
        let mut accounts = etched(0, None);
        let mint_ix = borsh::to_vec(&RuneInstruction::Mint).unwrap();
        assert_eq!(process(&mut accounts, &mint_ix, 2_000), Err(ProgramError::Unauthorized));
    }

    #[test]
    fn test_mint_outside_window_fails() {
        let terms = MintTerms { amount: 100, cap: 0, mints: 0, start: 5_000, end: 9_000 };
        let mut accounts = etched(0, Some(terms));
        let mint_ix = borsh::to_vec(&RuneInstruction::Mint).unwrap();
        // before start
        assert_eq!(process(&mut accounts, &mint_ix, 1_000), Err(ProgramError::Unauthorized));
        // after end
        assert_eq!(process(&mut accounts, &mint_ix, 10_000), Err(ProgramError::LoanExpired));
        // inside window
        process(&mut accounts, &mint_ix, 6_000).unwrap();
    }

    #[test]
    fn test_transfer_and_insufficient() {
        let mut accounts = etched(500, None);
        // accounts: [rune, etcher-bal(500), etcher]; add a destination balance.
        accounts.push(acc("dst", 256));
        // transfer 200 from etcher-bal (idx1) -> dst (idx3): build a 3-account window
        let mut window = vec![accounts[1].clone(), accounts[3].clone(), accounts[2].clone()];
        let ix = borsh::to_vec(&RuneInstruction::Transfer { amount: 200 }).unwrap();
        process(&mut window, &ix, 1_000).unwrap();
        let src: RuneBalance = window[0].read_data().unwrap();
        let dst: RuneBalance = window[1].read_data().unwrap();
        assert_eq!(src.amount, 300);
        assert_eq!(dst.amount, 200);

        // overspend fails
        let ix = borsh::to_vec(&RuneInstruction::Transfer { amount: 9_999 }).unwrap();
        assert_eq!(process(&mut window, &ix, 1_000), Err(ProgramError::InsufficientFunds));
    }

    #[test]
    fn test_burn_reduces_circulating() {
        let mut accounts = etched(1_000, None);
        let ix = borsh::to_vec(&RuneInstruction::Burn { amount: 400 }).unwrap();
        process(&mut accounts, &ix, 1_000).unwrap();
        let rune: RuneEtching = accounts[0].read_data().unwrap();
        assert_eq!(rune.burned, 400);
        assert_eq!(rune.circulating(), 600);
        let bal: RuneBalance = accounts[1].read_data().unwrap();
        assert_eq!(bal.amount, 600);
    }
}

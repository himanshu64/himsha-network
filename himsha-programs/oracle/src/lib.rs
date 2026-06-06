//! HIMSHA Oracle — signed price feeds.
//!
//! An authorized **oracle authority** posts prices into a `PriceFeed` account; any
//! other program (e.g. the money market) reads that feed and rejects prices older
//! than its configured staleness window. ZK proves "we used exactly the price the
//! oracle signed" — it can't prove the price is *true*, so the oracle is a separate
//! trust component (see docs/use-cases and the oracle/consensus discussion).
//!
//! `price` is a fixed-point value; the consumer defines the scale (the money market
//! uses `PRICE_SCALE = 1e6`, i.e. price of 1 collateral unit in borrow-asset units).

use borsh::{BorshDeserialize, BorshSerialize};
use himsha_runtime::{
    account::{AccountInfo, AccountMeta},
    error::ProgramError,
    instruction::Instruction,
    pubkey::Pubkey,
};

// ---- state ----

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct PriceFeed {
    /// The only key allowed to update this feed.
    pub authority: Pubkey,
    /// Latest fixed-point price.
    pub price: u64,
    /// Unix timestamp the price was last published.
    pub publish_ts: u64,
    pub is_initialized: bool,
}

impl PriceFeed {
    /// True if the price was published within `max_staleness` seconds of `now`
    /// and is non-zero.
    pub fn is_fresh(&self, now: u64, max_staleness: u64) -> bool {
        self.price != 0 && now.saturating_sub(self.publish_ts) <= max_staleness
    }
}

// ---- instructions ----

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum OracleInstruction {
    /// Create a feed owned by `authority`.
    /// [0] = feed (writable), [1] = authority (signer).
    InitFeed,

    /// Publish a new price (authority only).
    /// [0] = feed (writable), [1] = authority (signer).
    UpdatePrice { price: u64 },
}

// ---- builders ----

pub fn init_feed(feed: Pubkey, authority: Pubkey) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::oracle_program(),
        vec![
            AccountMeta::writable(feed, false),
            AccountMeta::readonly(authority, true),
        ],
        &OracleInstruction::InitFeed,
    )
}

pub fn update_price(feed: Pubkey, authority: Pubkey, price: u64) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::oracle_program(),
        vec![
            AccountMeta::writable(feed, false),
            AccountMeta::readonly(authority, true),
        ],
        &OracleInstruction::UpdatePrice { price },
    )
}

// ---- processing ----

pub fn process(accounts: &mut [AccountInfo], data: &[u8], timestamp: u64) -> Result<(), ProgramError> {
    let ix = OracleInstruction::try_from_slice(data)
        .map_err(|_| ProgramError::InvalidInstruction)?;

    match ix {
        OracleInstruction::InitFeed => {
            if accounts.len() < 2 { return Err(ProgramError::NotEnoughAccounts); }
            accounts[1].require_signer()?; // authority
            let authority = accounts[1].key;
            let mut feed: PriceFeed = accounts[0].read_data().unwrap_or_default();
            if feed.is_initialized { return Err(ProgramError::AlreadyInitialized); }
            feed.authority = authority;
            feed.is_initialized = true;
            accounts[0].write_data(&feed)?;
        }

        OracleInstruction::UpdatePrice { price } => {
            if accounts.len() < 2 { return Err(ProgramError::NotEnoughAccounts); }
            accounts[1].require_signer()?; // authority
            let mut feed: PriceFeed = accounts[0].read_data()?;
            if !feed.is_initialized { return Err(ProgramError::NotInitialized); }
            if accounts[1].key != feed.authority { return Err(ProgramError::Unauthorized); }
            if price == 0 { return Err(ProgramError::InvalidInstruction); }
            feed.price = price;
            feed.publish_ts = timestamp;
            accounts[0].write_data(&feed)?;
        }
    }
    Ok(())
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn prog() -> Pubkey { himsha_runtime::program_ids::oracle_program() }
    fn feed_acct() -> AccountInfo { AccountInfo::new(Pubkey::from_seed(b"feed"), prog(), 0, 128) }
    fn authority() -> AccountInfo {
        AccountInfo::new(Pubkey::from_seed(b"oracle-auth"), prog(), 0, 0).as_signer()
    }

    fn run(accounts: &mut [AccountInfo], ix: &OracleInstruction, ts: u64) -> Result<(), ProgramError> {
        process(accounts, &borsh::to_vec(ix).unwrap(), ts)
    }

    #[test]
    fn test_init_and_update() {
        let mut accounts = vec![feed_acct(), authority()];
        run(&mut accounts, &OracleInstruction::InitFeed, 0).unwrap();
        run(&mut accounts, &OracleInstruction::UpdatePrice { price: 95_000_000 }, 1_000).unwrap();
        let f: PriceFeed = accounts[0].read_data().unwrap();
        assert_eq!(f.price, 95_000_000);
        assert_eq!(f.publish_ts, 1_000);
        assert_eq!(f.authority, Pubkey::from_seed(b"oracle-auth"));
    }

    #[test]
    fn test_update_requires_authority_signer() {
        let mut accounts = vec![feed_acct(), authority()];
        run(&mut accounts, &OracleInstruction::InitFeed, 0).unwrap();
        // wrong key signs
        accounts[1] = AccountInfo::new(Pubkey::from_seed(b"intruder"), prog(), 0, 0).as_signer();
        assert_eq!(
            run(&mut accounts, &OracleInstruction::UpdatePrice { price: 1 }, 1),
            Err(ProgramError::Unauthorized),
        );
    }

    #[test]
    fn test_update_without_signature_fails() {
        let mut accounts = vec![feed_acct(), authority()];
        run(&mut accounts, &OracleInstruction::InitFeed, 0).unwrap();
        accounts[1].is_signer = false;
        assert_eq!(
            run(&mut accounts, &OracleInstruction::UpdatePrice { price: 1 }, 1),
            Err(ProgramError::MissingSigner),
        );
    }

    #[test]
    fn test_zero_price_rejected() {
        let mut accounts = vec![feed_acct(), authority()];
        run(&mut accounts, &OracleInstruction::InitFeed, 0).unwrap();
        assert_eq!(
            run(&mut accounts, &OracleInstruction::UpdatePrice { price: 0 }, 1),
            Err(ProgramError::InvalidInstruction),
        );
    }

    #[test]
    fn test_freshness() {
        let f = PriceFeed { authority: Pubkey::default(), price: 100, publish_ts: 1_000, is_initialized: true };
        assert!(f.is_fresh(1_050, 60));      // 50s old, window 60
        assert!(!f.is_fresh(1_200, 60));     // 200s old, stale
        let zero = PriceFeed { price: 0, ..f.clone() };
        assert!(!zero.is_fresh(1_000, 60));  // zero price never fresh
    }
}

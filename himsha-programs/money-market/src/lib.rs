//! HIMSHA Money Market — over-collateralized borrowing.
//!
//! A market pairs one **collateral** token with one **borrowable** token. Users:
//!   - **Supply**  collateral into the collateral vault,
//!   - **Borrow**  the borrowable token against it (up to the collateral factor),
//!   - **Repay**   debt,
//!   - **Withdraw** collateral (only while the position stays healthy).
//!
//! Token movements are real CPI calls into the token program. A position is
//! *healthy* when `debt <= collateral_value * collateral_factor`, where
//! `collateral_value = collateral * price / PRICE_SCALE`.
//!
//! Interest accrues on borrows via a utilization-based linear rate
//! (`rate = base + slope * utilization`) and a cumulative borrow index; debt
//! grows over time and is reconciled on every interaction.
//!
//! Out of scope for this cut (tracked as a follow-up):
//!   - liquidation of unhealthy positions → reserved threshold/bonus fields below

use borsh::{BorshDeserialize, BorshSerialize};
use himsha_oracle_program::PriceFeed;
use himsha_runtime::{
    account::{AccountInfo, AccountMeta},
    cpi,
    error::ProgramError,
    instruction::Instruction,
    pubkey::Pubkey,
};
use himsha_token_program::{process as token_process, TokenInstruction};

/// Fixed-point scale for `price` (price of 1 collateral unit in borrow-asset units).
pub const PRICE_SCALE: u128 = 1_000_000;
/// Basis-points denominator.
pub const BPS: u128 = 10_000;
/// Fixed-point scale for the cumulative borrow index (1.0 == `INDEX_SCALE`).
pub const INDEX_SCALE: u128 = 1_000_000_000_000;
/// Seconds in a (non-leap) year — the interest-rate quote period.
pub const SECONDS_PER_YEAR: u128 = 31_536_000;

// ---- on-chain state ----

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct MarketState {
    pub collateral_mint: Pubkey,
    pub borrow_mint: Pubkey,
    pub collateral_vault: Pubkey,
    pub borrow_vault: Pubkey,
    /// Max borrow as a fraction of collateral value, in bps (e.g. 7500 = 75%).
    pub collateral_factor_bps: u64,
    /// Health drops below 1 above this fraction, in bps (used by liquidation).
    pub liquidation_threshold_bps: u64,
    /// Extra collateral a liquidator receives, in bps (used by liquidation).
    pub liquidation_bonus_bps: u64,
    /// Price of 1 collateral unit in borrow-asset units, scaled by `PRICE_SCALE`.
    /// Synced from the oracle feed by `SyncPrice` (no longer a static admin value).
    pub price: u64,
    /// The oracle `PriceFeed` account this market reads prices from.
    pub oracle_feed: Pubkey,
    /// Unix timestamp `price` was last synced from the feed.
    pub price_updated_at: u64,
    /// Reject prices older than this many seconds.
    pub max_price_staleness: u64,
    pub total_collateral: u64,
    pub total_borrows: u64,
    /// Available borrow-asset liquidity (funded via `AddLiquidity`); drives utilization.
    pub total_cash: u64,
    /// Outstanding lender shares (cToken supply). A lender's claim on the pool is
    /// `shares * (total_cash + total_borrows) / total_lender_shares`, so the claim
    /// grows as borrowers pay interest — this is how the supply side earns yield.
    pub total_lender_shares: u64,
    // ---- interest-rate model (linear: rate = base + slope * utilization) ----
    /// Annual borrow rate at 0% utilization, in bps.
    pub base_rate_bps: u64,
    /// Additional annual borrow rate at 100% utilization, in bps.
    pub slope_bps: u64,
    /// Cumulative borrow index (scaled by `INDEX_SCALE`); grows with interest.
    pub borrow_index: u128,
    /// Unix timestamp interest was last accrued.
    pub last_accrual_ts: u64,
    pub is_initialized: bool,
}

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct Position {
    pub owner: Pubkey,
    pub market: Pubkey,
    pub collateral: u64,
    /// Debt as of `borrow_index_snapshot`; scaled to current on each interaction.
    pub debt: u64,
    /// Borrow index when `debt` was last reconciled.
    pub borrow_index_snapshot: u128,
    pub is_initialized: bool,
}

/// A liquidity provider's (lender's) claim on a market's supply side, denominated
/// in lender shares. Redeemable for a growing amount of the borrow asset as
/// interest accrues. See [`lender_share_value`].
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct LenderPosition {
    pub owner: Pubkey,
    pub market: Pubkey,
    pub shares: u64,
    pub is_initialized: bool,
}

/// Maximum borrowable (in borrow-asset units) for `collateral` at the market's
/// price and collateral factor.
pub fn max_borrow(market: &MarketState, collateral: u64) -> u128 {
    let value = (collateral as u128) * (market.price as u128) / PRICE_SCALE;
    value * (market.collateral_factor_bps as u128) / BPS
}

/// A position is healthy when its debt is within the borrowing power.
pub fn is_healthy(market: &MarketState, collateral: u64, debt: u64) -> bool {
    (debt as u128) <= max_borrow(market, collateral)
}

/// A position is liquidatable once its debt exceeds the *liquidation threshold*
/// (a band above the collateral factor): `debt > collateral_value * threshold`.
pub fn is_liquidatable(market: &MarketState, collateral: u64, debt: u64) -> bool {
    let value = (collateral as u128) * (market.price as u128) / PRICE_SCALE;
    let max_debt = value * (market.liquidation_threshold_bps as u128) / BPS;
    (debt as u128) > max_debt
}

/// Collateral (in collateral units) seized for repaying `repay_amount` of debt,
/// including the liquidation bonus. Caller must cap this at available collateral.
pub fn seize_collateral(market: &MarketState, repay_amount: u64) -> u128 {
    // 1 borrow unit is worth `PRICE_SCALE / price` collateral units.
    let base = (repay_amount as u128) * PRICE_SCALE / (market.price as u128);
    base * (BPS + market.liquidation_bonus_bps as u128) / BPS
}

/// Reject a price-dependent action when the cached oracle price is missing or
/// older than the market's staleness window.
pub fn ensure_fresh_price(market: &MarketState, now: u64) -> Result<(), ProgramError> {
    if market.price == 0 || now.saturating_sub(market.price_updated_at) > market.max_price_staleness
    {
        return Err(ProgramError::StalePrice);
    }
    Ok(())
}

/// Current utilization in bps: `total_borrows / (total_borrows + total_cash)`.
pub fn utilization_bps(market: &MarketState) -> u128 {
    let denom = market.total_borrows as u128 + market.total_cash as u128;
    // `checked_div` yields None when denom == 0 (an empty market) → 0% utilization.
    ((market.total_borrows as u128) * BPS)
        .checked_div(denom)
        .unwrap_or(0)
}

/// Annual borrow rate in bps under the linear model: `base + slope * utilization`.
pub fn borrow_rate_bps(market: &MarketState) -> u128 {
    market.base_rate_bps as u128 + (market.slope_bps as u128) * utilization_bps(market) / BPS
}

/// Accrue interest into `market` up to `now`, growing the borrow index and
/// `total_borrows` by the same factor `(1 + rate * Δt)` (simple interest per period).
pub fn accrue_market(market: &mut MarketState, now: u64) {
    if now <= market.last_accrual_ts {
        if market.last_accrual_ts == 0 {
            market.last_accrual_ts = now;
        }
        return;
    }
    let elapsed = (now - market.last_accrual_ts) as u128;
    market.last_accrual_ts = now;
    if market.total_borrows == 0 {
        return;
    }

    let annual_bps = borrow_rate_bps(market);
    if annual_bps == 0 {
        return;
    }

    // factor = 1 + annual_bps/BPS * elapsed/SECONDS_PER_YEAR, kept as num/den.
    let den = BPS * SECONDS_PER_YEAR;
    let num = den + annual_bps * elapsed;
    market.borrow_index = market.borrow_index.saturating_mul(num) / den;
    market.total_borrows = ((market.total_borrows as u128) * num / den) as u64;
}

/// Total value of the supply side (cash on hand plus debt owed to lenders).
pub fn pool_value(market: &MarketState) -> u128 {
    market.total_cash as u128 + market.total_borrows as u128
}

/// Underlying borrow-asset value of `shares` lender shares at the current
/// exchange rate. Zero when no shares exist.
pub fn lender_share_value(market: &MarketState, shares: u64) -> u64 {
    if market.total_lender_shares == 0 {
        return 0;
    }
    (shares as u128 * pool_value(market) / market.total_lender_shares as u128) as u64
}

/// Lender shares to mint for depositing `amount` of underlying (1:1 to bootstrap).
pub fn shares_for_deposit(market: &MarketState, amount: u64) -> u64 {
    let pool = pool_value(market);
    if market.total_lender_shares == 0 || pool == 0 {
        amount
    } else {
        (amount as u128 * market.total_lender_shares as u128 / pool) as u64
    }
}

/// Lender shares to burn to withdraw `amount` of underlying, rounded **up** so a
/// withdrawal never redeems more value than the shares burned represent.
pub fn shares_for_withdraw(market: &MarketState, amount: u64) -> u64 {
    let pool = pool_value(market);
    if pool == 0 {
        return 0;
    }
    ((amount as u128) * market.total_lender_shares as u128).div_ceil(pool) as u64
}

/// Reconcile a position's stored debt to the market's current borrow index.
pub fn accrue_position(pos: &mut Position, borrow_index: u128) {
    if pos.borrow_index_snapshot == 0 {
        pos.borrow_index_snapshot = borrow_index;
    } else if pos.debt > 0 && borrow_index > pos.borrow_index_snapshot {
        pos.debt = ((pos.debt as u128) * borrow_index / pos.borrow_index_snapshot) as u64;
        pos.borrow_index_snapshot = borrow_index;
    } else {
        pos.borrow_index_snapshot = borrow_index;
    }
}

// ---- instructions ----

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum MoneyMarketInstruction {
    /// Initialize a market.
    /// [0]=market (w), [1]=collateral_mint, [2]=borrow_mint,
    /// [3]=collateral_vault, [4]=borrow_vault, [5]=admin (signer), [6]=oracle_feed.
    /// `price` seeds an initial value; thereafter use `SyncPrice` from the feed.
    InitMarket {
        collateral_factor_bps: u64,
        liquidation_threshold_bps: u64,
        liquidation_bonus_bps: u64,
        price: u64,
        base_rate_bps: u64,
        slope_bps: u64,
        max_price_staleness: u64,
    },

    /// Provide borrow-asset liquidity (supply side); mints lender shares to the
    /// provider's lender position. The shares accrue borrower interest.
    /// [0]=market (w), [1]=lender_position (w), [2]=provider_borrow (w),
    /// [3]=borrow_vault (w), [4]=provider (signer).
    AddLiquidity { amount: u64 },

    /// Redeem lender shares for `amount` of the borrow asset (supply side exit).
    /// Burns the shares the `amount` is worth at the current exchange rate and
    /// pays out from available cash (capped by liquidity).
    /// [0]=market (w), [1]=lender_position (w), [2]=provider_borrow (w),
    /// [3]=borrow_vault (w), [4]=provider (signer).
    RemoveLiquidity { amount: u64 },

    /// Supply collateral.
    /// [0]=market (w), [1]=position (w), [2]=user_collateral (w),
    /// [3]=collateral_vault (w), [4]=user (signer).
    Supply { amount: u64 },

    /// Withdraw collateral (must remain healthy).
    /// [0]=market (w), [1]=position (w), [2]=user_collateral (w),
    /// [3]=collateral_vault (w), [4]=user (signer).
    Withdraw { amount: u64 },

    /// Borrow the borrowable token against supplied collateral.
    /// [0]=market (w), [1]=position (w), [2]=user_borrow (w),
    /// [3]=borrow_vault (w), [4]=user (signer).
    Borrow { amount: u64 },

    /// Repay debt.
    /// [0]=market (w), [1]=position (w), [2]=user_borrow (w),
    /// [3]=borrow_vault (w), [4]=user (signer).
    Repay { amount: u64 },

    /// Liquidate an unhealthy position: repay part of its debt, seize collateral
    /// (plus the liquidation bonus) to the liquidator.
    /// [0]=market (w), [1]=position (w), [2]=liquidator_borrow (w),
    /// [3]=borrow_vault (w), [4]=liquidator_collateral (w),
    /// [5]=collateral_vault (w), [6]=liquidator (signer).
    Liquidate { repay_amount: u64 },

    /// Sync the cached price from the oracle feed (the feed is authority-signed,
    /// so anyone may trigger a sync). [0]=market (w), [1]=oracle_feed.
    SyncPrice,
}

// ---- instruction builders ----

fn program() -> Pubkey {
    himsha_runtime::program_ids::money_market_program()
}

#[allow(clippy::too_many_arguments)]
pub fn init_market(
    market: Pubkey,
    collateral_mint: Pubkey,
    borrow_mint: Pubkey,
    collateral_vault: Pubkey,
    borrow_vault: Pubkey,
    admin: Pubkey,
    oracle_feed: Pubkey,
    collateral_factor_bps: u64,
    liquidation_threshold_bps: u64,
    liquidation_bonus_bps: u64,
    price: u64,
    base_rate_bps: u64,
    slope_bps: u64,
    max_price_staleness: u64,
) -> Instruction {
    Instruction::with_args(
        program(),
        vec![
            AccountMeta::writable(market, false),
            AccountMeta::readonly(collateral_mint, false),
            AccountMeta::readonly(borrow_mint, false),
            AccountMeta::writable(collateral_vault, false),
            AccountMeta::writable(borrow_vault, false),
            AccountMeta::readonly(admin, true),
            AccountMeta::readonly(oracle_feed, false),
        ],
        &MoneyMarketInstruction::InitMarket {
            collateral_factor_bps,
            liquidation_threshold_bps,
            liquidation_bonus_bps,
            price,
            base_rate_bps,
            slope_bps,
            max_price_staleness,
        },
    )
}

/// Sync the market's cached price from its oracle feed.
pub fn sync_price(market: Pubkey, oracle_feed: Pubkey) -> Instruction {
    Instruction::with_args(
        program(),
        vec![
            AccountMeta::writable(market, false),
            AccountMeta::readonly(oracle_feed, false),
        ],
        &MoneyMarketInstruction::SyncPrice,
    )
}

fn liquidity_ix(
    tag: MoneyMarketInstruction,
    market: Pubkey,
    lender_position: Pubkey,
    provider_borrow: Pubkey,
    borrow_vault: Pubkey,
    provider: Pubkey,
) -> Instruction {
    Instruction::with_args(
        program(),
        vec![
            AccountMeta::writable(market, false),
            AccountMeta::writable(lender_position, false),
            AccountMeta::writable(provider_borrow, false),
            AccountMeta::writable(borrow_vault, false),
            AccountMeta::readonly(provider, true),
        ],
        &tag,
    )
}

pub fn add_liquidity(
    market: Pubkey,
    lender_position: Pubkey,
    provider_borrow: Pubkey,
    borrow_vault: Pubkey,
    provider: Pubkey,
    amount: u64,
) -> Instruction {
    liquidity_ix(
        MoneyMarketInstruction::AddLiquidity { amount },
        market,
        lender_position,
        provider_borrow,
        borrow_vault,
        provider,
    )
}

pub fn remove_liquidity(
    market: Pubkey,
    lender_position: Pubkey,
    provider_borrow: Pubkey,
    borrow_vault: Pubkey,
    provider: Pubkey,
    amount: u64,
) -> Instruction {
    liquidity_ix(
        MoneyMarketInstruction::RemoveLiquidity { amount },
        market,
        lender_position,
        provider_borrow,
        borrow_vault,
        provider,
    )
}

fn vault_ix(
    tag: MoneyMarketInstruction,
    market: Pubkey,
    position: Pubkey,
    user_token: Pubkey,
    vault: Pubkey,
    user: Pubkey,
) -> Instruction {
    Instruction::with_args(
        program(),
        vec![
            AccountMeta::writable(market, false),
            AccountMeta::writable(position, false),
            AccountMeta::writable(user_token, false),
            AccountMeta::writable(vault, false),
            AccountMeta::readonly(user, true),
        ],
        &tag,
    )
}

pub fn supply(
    market: Pubkey,
    position: Pubkey,
    user_collateral: Pubkey,
    collateral_vault: Pubkey,
    user: Pubkey,
    amount: u64,
) -> Instruction {
    vault_ix(
        MoneyMarketInstruction::Supply { amount },
        market,
        position,
        user_collateral,
        collateral_vault,
        user,
    )
}
pub fn withdraw(
    market: Pubkey,
    position: Pubkey,
    user_collateral: Pubkey,
    collateral_vault: Pubkey,
    user: Pubkey,
    amount: u64,
) -> Instruction {
    vault_ix(
        MoneyMarketInstruction::Withdraw { amount },
        market,
        position,
        user_collateral,
        collateral_vault,
        user,
    )
}
pub fn borrow(
    market: Pubkey,
    position: Pubkey,
    user_borrow: Pubkey,
    borrow_vault: Pubkey,
    user: Pubkey,
    amount: u64,
) -> Instruction {
    vault_ix(
        MoneyMarketInstruction::Borrow { amount },
        market,
        position,
        user_borrow,
        borrow_vault,
        user,
    )
}
pub fn repay(
    market: Pubkey,
    position: Pubkey,
    user_borrow: Pubkey,
    borrow_vault: Pubkey,
    user: Pubkey,
    amount: u64,
) -> Instruction {
    vault_ix(
        MoneyMarketInstruction::Repay { amount },
        market,
        position,
        user_borrow,
        borrow_vault,
        user,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn liquidate(
    market: Pubkey,
    position: Pubkey,
    liquidator_borrow: Pubkey,
    borrow_vault: Pubkey,
    liquidator_collateral: Pubkey,
    collateral_vault: Pubkey,
    liquidator: Pubkey,
    repay_amount: u64,
) -> Instruction {
    Instruction::with_args(
        program(),
        vec![
            AccountMeta::writable(market, false),
            AccountMeta::writable(position, false),
            AccountMeta::writable(liquidator_borrow, false),
            AccountMeta::writable(borrow_vault, false),
            AccountMeta::writable(liquidator_collateral, false),
            AccountMeta::writable(collateral_vault, false),
            AccountMeta::readonly(liquidator, true),
        ],
        &MoneyMarketInstruction::Liquidate { repay_amount },
    )
}

// ---- processing ----

fn transfer(
    accounts: &mut [AccountInfo],
    src: usize,
    dst: usize,
    owner: usize,
    amount: u64,
) -> Result<(), ProgramError> {
    let ix = borsh::to_vec(&TokenInstruction::Transfer { amount })
        .map_err(|_| ProgramError::BorshError)?;
    cpi::invoke_indexed(accounts, &[src, dst, owner], &ix, token_process)
}

/// Transfer out of a vault the market controls. The `owner` account is the
/// market authority, which didn't sign the tx, so we sign for it (window index 2).
fn transfer_signed(
    accounts: &mut [AccountInfo],
    src: usize,
    dst: usize,
    owner: usize,
    amount: u64,
) -> Result<(), ProgramError> {
    let ix = borsh::to_vec(&TokenInstruction::Transfer { amount })
        .map_err(|_| ProgramError::BorshError)?;
    cpi::invoke_signed_indexed(accounts, &[src, dst, owner], &[2], &ix, token_process)
}

fn load_position(
    acc: &AccountInfo,
    market_key: Pubkey,
    owner: Pubkey,
) -> Result<Position, ProgramError> {
    let mut pos: Position = acc.read_data().unwrap_or_default();
    if !pos.is_initialized {
        pos.owner = owner;
        pos.market = market_key;
        pos.is_initialized = true;
    } else if pos.market != market_key {
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(pos)
}

fn load_lender_position(
    acc: &AccountInfo,
    market_key: Pubkey,
    owner: Pubkey,
) -> Result<LenderPosition, ProgramError> {
    let mut pos: LenderPosition = acc.read_data().unwrap_or_default();
    if !pos.is_initialized {
        pos.owner = owner;
        pos.market = market_key;
        pos.is_initialized = true;
    } else if pos.market != market_key || pos.owner != owner {
        return Err(ProgramError::InvalidAccountData);
    }
    Ok(pos)
}

pub fn process(
    accounts: &mut [AccountInfo],
    data: &[u8],
    timestamp: u64,
) -> Result<(), ProgramError> {
    let ix = MoneyMarketInstruction::try_from_slice(data)
        .map_err(|_| ProgramError::InvalidInstruction)?;

    match ix {
        MoneyMarketInstruction::InitMarket {
            collateral_factor_bps,
            liquidation_threshold_bps,
            liquidation_bonus_bps,
            price,
            base_rate_bps,
            slope_bps,
            max_price_staleness,
        } => {
            if accounts.len() < 7 {
                return Err(ProgramError::NotEnoughAccounts);
            }
            accounts[5].require_signer()?; // admin must sign
            if collateral_factor_bps > BPS as u64 || liquidation_threshold_bps > BPS as u64 {
                return Err(ProgramError::InvalidInstruction);
            }
            // Collateral factor must not exceed the liquidation threshold.
            if collateral_factor_bps > liquidation_threshold_bps {
                return Err(ProgramError::InvalidInstruction);
            }
            if price == 0 {
                return Err(ProgramError::InvalidInstruction);
            }

            let mut market: MarketState = accounts[0].read_data().unwrap_or_default();
            if market.is_initialized {
                return Err(ProgramError::AlreadyInitialized);
            }

            market.collateral_mint = accounts[1].key;
            market.borrow_mint = accounts[2].key;
            market.collateral_vault = accounts[3].key;
            market.borrow_vault = accounts[4].key;
            market.oracle_feed = accounts[6].key;
            market.collateral_factor_bps = collateral_factor_bps;
            market.liquidation_threshold_bps = liquidation_threshold_bps;
            market.liquidation_bonus_bps = liquidation_bonus_bps;
            market.price = price; // initial seed; refreshed via SyncPrice
            market.price_updated_at = timestamp;
            market.max_price_staleness = max_price_staleness;
            market.base_rate_bps = base_rate_bps;
            market.slope_bps = slope_bps;
            market.borrow_index = INDEX_SCALE;
            market.last_accrual_ts = timestamp;
            market.is_initialized = true;
            accounts[0].write_data(&market)?;
        }

        MoneyMarketInstruction::SyncPrice => {
            if accounts.len() < 2 {
                return Err(ProgramError::NotEnoughAccounts);
            }
            let mut market: MarketState = accounts[0].read_data()?;
            if !market.is_initialized {
                return Err(ProgramError::NotInitialized);
            }
            // Must read the market's configured feed account.
            if accounts[1].key != market.oracle_feed {
                return Err(ProgramError::InvalidAccountData);
            }
            let feed: PriceFeed = accounts[1].read_data()?;
            if !feed.is_initialized || feed.price == 0 {
                return Err(ProgramError::StalePrice);
            }
            market.price = feed.price;
            market.price_updated_at = feed.publish_ts;
            accounts[0].write_data(&market)?;
        }

        MoneyMarketInstruction::AddLiquidity { amount } => {
            // [0]=market, [1]=lender_position, [2]=provider_borrow, [3]=borrow_vault, [4]=provider.
            if accounts.len() < 5 {
                return Err(ProgramError::NotEnoughAccounts);
            }
            let market_key = accounts[0].key;
            let provider = accounts[4].key;
            accounts[4].require_signer()?; // provider must sign
            if amount == 0 {
                return Err(ProgramError::InvalidInstruction);
            }
            let mut market: MarketState = accounts[0].read_data()?;
            if !market.is_initialized {
                return Err(ProgramError::NotInitialized);
            }
            accrue_market(&mut market, timestamp);
            let mut lender = load_lender_position(&accounts[1], market_key, provider)?;

            // Shares minted at the current exchange rate (1:1 to bootstrap).
            let shares = shares_for_deposit(&market, amount);

            // CPI: provider_borrow -> borrow_vault.
            transfer(accounts, 2, 3, 4, amount)?;

            market.total_cash = market
                .total_cash
                .checked_add(amount)
                .ok_or(ProgramError::Overflow)?;
            market.total_lender_shares = market
                .total_lender_shares
                .checked_add(shares)
                .ok_or(ProgramError::Overflow)?;
            lender.shares = lender
                .shares
                .checked_add(shares)
                .ok_or(ProgramError::Overflow)?;
            accounts[0].write_data(&market)?;
            accounts[1].write_data(&lender)?;
        }

        MoneyMarketInstruction::RemoveLiquidity { amount } => {
            // [0]=market, [1]=lender_position, [2]=provider_borrow, [3]=borrow_vault, [4]=provider.
            if accounts.len() < 5 {
                return Err(ProgramError::NotEnoughAccounts);
            }
            let market_key = accounts[0].key;
            let provider = accounts[4].key;
            accounts[4].require_signer()?; // provider must sign
            if amount == 0 {
                return Err(ProgramError::InvalidInstruction);
            }
            let mut market: MarketState = accounts[0].read_data()?;
            if !market.is_initialized {
                return Err(ProgramError::NotInitialized);
            }
            accrue_market(&mut market, timestamp);
            let mut lender = load_lender_position(&accounts[1], market_key, provider)?;

            // Burn the shares this `amount` of underlying is worth, capped at what
            // the lender holds. Only cash on hand can be paid out (borrowed funds
            // can't), so larger redemptions wait for borrowers to repay.
            let burn = shares_for_withdraw(&market, amount);
            if burn > lender.shares {
                return Err(ProgramError::InsufficientFunds);
            }
            if amount > market.total_cash {
                return Err(ProgramError::InsufficientLiquidity);
            }

            // CPI: borrow_vault -> provider_borrow (market signs as vault authority).
            transfer_signed(accounts, 3, 2, 0, amount)?;

            market.total_cash = market
                .total_cash
                .checked_sub(amount)
                .ok_or(ProgramError::InsufficientLiquidity)?;
            market.total_lender_shares = market
                .total_lender_shares
                .checked_sub(burn)
                .ok_or(ProgramError::Overflow)?;
            lender.shares = lender
                .shares
                .checked_sub(burn)
                .ok_or(ProgramError::InsufficientFunds)?;
            accounts[0].write_data(&market)?;
            accounts[1].write_data(&lender)?;
        }

        MoneyMarketInstruction::Supply { amount } => {
            if accounts.len() < 5 {
                return Err(ProgramError::NotEnoughAccounts);
            }
            let market_key = accounts[0].key;
            let user = accounts[4].key;
            accounts[4].require_signer()?; // user must sign
            let mut market: MarketState = accounts[0].read_data()?;
            if !market.is_initialized {
                return Err(ProgramError::NotInitialized);
            }
            accrue_market(&mut market, timestamp);
            let mut pos = load_position(&accounts[1], market_key, user)?;
            accrue_position(&mut pos, market.borrow_index);

            // CPI: user_collateral -> collateral_vault.
            transfer(accounts, 2, 3, 4, amount)?;

            pos.collateral = pos
                .collateral
                .checked_add(amount)
                .ok_or(ProgramError::Overflow)?;
            market.total_collateral = market
                .total_collateral
                .checked_add(amount)
                .ok_or(ProgramError::Overflow)?;
            accounts[0].write_data(&market)?;
            accounts[1].write_data(&pos)?;
        }

        MoneyMarketInstruction::Withdraw { amount } => {
            if accounts.len() < 5 {
                return Err(ProgramError::NotEnoughAccounts);
            }
            let market_key = accounts[0].key;
            let user = accounts[4].key;
            accounts[4].require_signer()?; // user must sign
            let mut market: MarketState = accounts[0].read_data()?;
            if !market.is_initialized {
                return Err(ProgramError::NotInitialized);
            }
            accrue_market(&mut market, timestamp);
            let mut pos = load_position(&accounts[1], market_key, user)?;
            if pos.owner != user {
                return Err(ProgramError::Unauthorized);
            }
            accrue_position(&mut pos, market.borrow_index);
            ensure_fresh_price(&market, timestamp)?; // collateral valuation needs a fresh price

            let remaining = pos
                .collateral
                .checked_sub(amount)
                .ok_or(ProgramError::InsufficientFunds)?;
            // Position must remain healthy after the withdrawal (against accrued debt).
            if !is_healthy(&market, remaining, pos.debt) {
                return Err(ProgramError::Undercollateralized);
            }

            // CPI: collateral_vault -> user_collateral (market signs as vault authority).
            transfer_signed(accounts, 3, 2, 0, amount)?;

            pos.collateral = remaining;
            market.total_collateral = market
                .total_collateral
                .checked_sub(amount)
                .ok_or(ProgramError::Overflow)?;
            accounts[0].write_data(&market)?;
            accounts[1].write_data(&pos)?;
        }

        MoneyMarketInstruction::Borrow { amount } => {
            if accounts.len() < 5 {
                return Err(ProgramError::NotEnoughAccounts);
            }
            let market_key = accounts[0].key;
            let user = accounts[4].key;
            accounts[4].require_signer()?; // user must sign
            let mut market: MarketState = accounts[0].read_data()?;
            if !market.is_initialized {
                return Err(ProgramError::NotInitialized);
            }
            accrue_market(&mut market, timestamp);
            let mut pos = load_position(&accounts[1], market_key, user)?;
            if pos.owner != user {
                return Err(ProgramError::Unauthorized);
            }
            accrue_position(&mut pos, market.borrow_index);
            ensure_fresh_price(&market, timestamp)?; // borrowing power needs a fresh price

            let new_debt = pos.debt.checked_add(amount).ok_or(ProgramError::Overflow)?;
            if !is_healthy(&market, pos.collateral, new_debt) {
                return Err(ProgramError::Undercollateralized);
            }
            // Liquidity must cover the draw.
            let new_cash = market
                .total_cash
                .checked_sub(amount)
                .ok_or(ProgramError::InsufficientLiquidity)?;

            // CPI: borrow_vault -> user_borrow (market signs as vault authority).
            transfer_signed(accounts, 3, 2, 0, amount).map_err(|e| {
                if e == ProgramError::InsufficientFunds {
                    ProgramError::InsufficientLiquidity
                } else {
                    e
                }
            })?;

            pos.debt = new_debt;
            market.total_cash = new_cash;
            market.total_borrows = market
                .total_borrows
                .checked_add(amount)
                .ok_or(ProgramError::Overflow)?;
            accounts[0].write_data(&market)?;
            accounts[1].write_data(&pos)?;
        }

        MoneyMarketInstruction::Repay { amount } => {
            if accounts.len() < 5 {
                return Err(ProgramError::NotEnoughAccounts);
            }
            let market_key = accounts[0].key;
            let user = accounts[4].key;
            accounts[4].require_signer()?; // user must sign
            let mut market: MarketState = accounts[0].read_data()?;
            if !market.is_initialized {
                return Err(ProgramError::NotInitialized);
            }
            accrue_market(&mut market, timestamp);
            let mut pos = load_position(&accounts[1], market_key, user)?;
            accrue_position(&mut pos, market.borrow_index);

            // Repay at most the (accrued) outstanding debt.
            let repay_amount = amount.min(pos.debt);

            // CPI: user_borrow -> borrow_vault.
            transfer(accounts, 2, 3, 4, repay_amount)?;

            pos.debt -= repay_amount;
            market.total_cash = market
                .total_cash
                .checked_add(repay_amount)
                .ok_or(ProgramError::Overflow)?;
            market.total_borrows = market.total_borrows.saturating_sub(repay_amount);
            accounts[0].write_data(&market)?;
            accounts[1].write_data(&pos)?;
        }

        MoneyMarketInstruction::Liquidate { repay_amount } => {
            // [0]=market, [1]=position, [2]=liquidator_borrow, [3]=borrow_vault,
            // [4]=liquidator_collateral, [5]=collateral_vault, [6]=liquidator.
            if accounts.len() < 7 {
                return Err(ProgramError::NotEnoughAccounts);
            }
            accounts[6].require_signer()?; // liquidator must sign
            let market_key = accounts[0].key;
            let mut market: MarketState = accounts[0].read_data()?;
            if !market.is_initialized {
                return Err(ProgramError::NotInitialized);
            }
            accrue_market(&mut market, timestamp);

            let mut pos: Position = accounts[1].read_data()?;
            if !pos.is_initialized || pos.market != market_key {
                return Err(ProgramError::InvalidAccountData);
            }
            accrue_position(&mut pos, market.borrow_index);
            ensure_fresh_price(&market, timestamp)?; // liquidation must use a fresh price

            // Only unhealthy (sub-threshold) positions may be liquidated.
            if !is_liquidatable(&market, pos.collateral, pos.debt) {
                return Err(ProgramError::Unauthorized);
            }

            // Repay at most the outstanding debt; seize the matching collateral
            // (with bonus), capped at what the position actually holds.
            let repaid = repay_amount.min(pos.debt);
            if repaid == 0 {
                return Err(ProgramError::InvalidInstruction);
            }
            let seized = (seize_collateral(&market, repaid) as u64).min(pos.collateral);

            // CPI: liquidator pays the debt into the vault.
            transfer(accounts, 2, 3, 6, repaid)?;
            // CPI: collateral_vault releases the seized collateral (market signs).
            transfer_signed(accounts, 5, 4, 0, seized)?;

            pos.debt -= repaid;
            pos.collateral -= seized;
            market.total_borrows = market.total_borrows.saturating_sub(repaid);
            market.total_cash = market
                .total_cash
                .checked_add(repaid)
                .ok_or(ProgramError::Overflow)?;
            market.total_collateral = market.total_collateral.saturating_sub(seized);
            accounts[0].write_data(&market)?;
            accounts[1].write_data(&pos)?;
        }
    }

    Ok(())
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use himsha_runtime::account::AccountState;
    use himsha_token_program::TokenAccountState;

    fn mm_prog() -> Pubkey {
        himsha_runtime::program_ids::money_market_program()
    }

    fn token_acct(key: &str, mint: Pubkey, owner: Pubkey, amount: u64) -> AccountInfo {
        let mut a = AccountInfo::new(
            Pubkey::from_seed(key.as_bytes()),
            himsha_runtime::program_ids::token_program(),
            0,
            256,
        );
        a.write_data(&TokenAccountState {
            mint,
            owner,
            amount,
            delegate: None,
            state: AccountState::Initialized,
            delegated_amount: 0,
            close_authority: None,
        })
        .unwrap();
        a
    }

    /// Market: collateral mint C, borrow mint B, 75% LTV, price 1.0 (1 C = 1 B),
    /// zero interest (use `market_acct_rates` for an interest-bearing market).
    fn market_acct(total_collateral: u64, total_borrows: u64, total_cash: u64) -> AccountInfo {
        market_acct_rates(total_collateral, total_borrows, total_cash, 0, 0)
    }

    fn market_acct_rates(
        total_collateral: u64,
        total_borrows: u64,
        total_cash: u64,
        base_rate_bps: u64,
        slope_bps: u64,
    ) -> AccountInfo {
        let mut a = AccountInfo::new(Pubkey::from_seed(b"market"), mm_prog(), 0, 512);
        a.write_data(&MarketState {
            collateral_mint: Pubkey::from_seed(b"mint-c"),
            borrow_mint: Pubkey::from_seed(b"mint-b"),
            collateral_vault: Pubkey::from_seed(b"vault-c"),
            borrow_vault: Pubkey::from_seed(b"vault-b"),
            collateral_factor_bps: 7500,
            liquidation_threshold_bps: 8000,
            liquidation_bonus_bps: 500,
            price: PRICE_SCALE as u64, // 1.0
            oracle_feed: Pubkey::from_seed(b"feed"),
            price_updated_at: 0,
            max_price_staleness: u64::MAX, // never stale in unit tests
            total_collateral,
            total_borrows,
            total_cash,
            total_lender_shares: 0,
            base_rate_bps,
            slope_bps,
            borrow_index: INDEX_SCALE,
            last_accrual_ts: 0,
            is_initialized: true,
        })
        .unwrap();
        a
    }

    /// A market with pre-existing lender shares (for redemption / yield tests).
    fn market_with_lenders(
        total_borrows: u64,
        total_cash: u64,
        total_lender_shares: u64,
    ) -> AccountInfo {
        let mut a = market_acct(0, total_borrows, total_cash);
        let mut m: MarketState = a.read_data().unwrap();
        m.total_lender_shares = total_lender_shares;
        a.write_data(&m).unwrap();
        a
    }

    fn lender_k() -> Pubkey {
        Pubkey::from_seed(b"lender")
    }

    fn lender_pos_acct(shares: u64) -> AccountInfo {
        let mut a = AccountInfo::new(Pubkey::from_seed(b"lender-pos"), mm_prog(), 0, 128);
        if shares != 0 {
            a.write_data(&LenderPosition {
                owner: lender_k(),
                market: market_k(),
                shares,
                is_initialized: true,
            })
            .unwrap();
        }
        a
    }

    fn lender() -> AccountInfo {
        AccountInfo::new(lender_k(), mm_prog(), 0, 0).as_signer()
    }

    fn lender_shares(a: &AccountInfo) -> u64 {
        a.read_data::<LenderPosition>().unwrap().shares
    }

    fn position_acct(collateral: u64, debt: u64) -> AccountInfo {
        let mut a = AccountInfo::new(Pubkey::from_seed(b"pos"), mm_prog(), 0, 256);
        if collateral != 0 || debt != 0 {
            a.write_data(&Position {
                owner: Pubkey::from_seed(b"user"),
                market: Pubkey::from_seed(b"market"),
                collateral,
                debt,
                borrow_index_snapshot: INDEX_SCALE,
                is_initialized: true,
            })
            .unwrap();
        }
        a
    }

    fn user() -> AccountInfo {
        AccountInfo::new(Pubkey::from_seed(b"user"), mm_prog(), 0, 0).as_signer()
    }

    fn bal(a: &AccountInfo) -> u64 {
        a.read_data::<TokenAccountState>().unwrap().amount
    }

    fn mint_c() -> Pubkey {
        Pubkey::from_seed(b"mint-c")
    }
    fn mint_b() -> Pubkey {
        Pubkey::from_seed(b"mint-b")
    }
    fn user_k() -> Pubkey {
        Pubkey::from_seed(b"user")
    }
    fn market_k() -> Pubkey {
        Pubkey::from_seed(b"market")
    }

    #[test]
    fn test_init_market() {
        let mut accounts = vec![
            AccountInfo::new(market_k(), mm_prog(), 0, 512),
            AccountInfo::new(mint_c(), mm_prog(), 0, 0),
            AccountInfo::new(mint_b(), mm_prog(), 0, 0),
            AccountInfo::new(Pubkey::from_seed(b"vault-c"), mm_prog(), 0, 0),
            AccountInfo::new(Pubkey::from_seed(b"vault-b"), mm_prog(), 0, 0),
            user(),
            AccountInfo::new(Pubkey::from_seed(b"feed"), mm_prog(), 0, 0), // [6] oracle feed
        ];
        let ix = borsh::to_vec(&MoneyMarketInstruction::InitMarket {
            collateral_factor_bps: 7500,
            liquidation_threshold_bps: 8000,
            liquidation_bonus_bps: 500,
            price: PRICE_SCALE as u64,
            base_rate_bps: 200,
            slope_bps: 1000,
            max_price_staleness: 600,
        })
        .unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        let m: MarketState = accounts[0].read_data().unwrap();
        assert!(m.is_initialized);
        assert_eq!(m.collateral_factor_bps, 7500);
        assert_eq!(m.oracle_feed, Pubkey::from_seed(b"feed"));
    }

    #[test]
    fn test_init_market_cf_above_threshold_fails() {
        let mut accounts = vec![
            AccountInfo::new(market_k(), mm_prog(), 0, 512),
            AccountInfo::new(mint_c(), mm_prog(), 0, 0),
            AccountInfo::new(mint_b(), mm_prog(), 0, 0),
            AccountInfo::new(Pubkey::from_seed(b"vault-c"), mm_prog(), 0, 0),
            AccountInfo::new(Pubkey::from_seed(b"vault-b"), mm_prog(), 0, 0),
            user(),
            AccountInfo::new(Pubkey::from_seed(b"feed"), mm_prog(), 0, 0), // [6] oracle feed
        ];
        let ix = borsh::to_vec(&MoneyMarketInstruction::InitMarket {
            collateral_factor_bps: 9000,
            liquidation_threshold_bps: 8000, // cf > threshold
            liquidation_bonus_bps: 500,
            price: PRICE_SCALE as u64,
            base_rate_bps: 200,
            slope_bps: 1000,
            max_price_staleness: 600,
        })
        .unwrap();
        assert_eq!(
            process(&mut accounts, &ix, 0),
            Err(ProgramError::InvalidInstruction)
        );
    }

    fn feed_acct(price: u64, publish_ts: u64) -> AccountInfo {
        let mut a = AccountInfo::new(
            Pubkey::from_seed(b"feed"),
            himsha_runtime::program_ids::oracle_program(),
            0,
            128,
        );
        a.write_data(&himsha_oracle_program::PriceFeed {
            authority: Pubkey::from_seed(b"oracle-auth"),
            price,
            publish_ts,
            is_initialized: true,
        })
        .unwrap();
        a
    }

    #[test]
    fn test_sync_price_from_feed() {
        // Market starts at 1.0; the feed publishes 2.0 → SyncPrice copies it in.
        let mut accounts = vec![market_acct(0, 0, 0), feed_acct(2 * PRICE_SCALE as u64, 500)];
        let ix = borsh::to_vec(&MoneyMarketInstruction::SyncPrice).unwrap();
        process(&mut accounts, &ix, 600).unwrap();
        let m: MarketState = accounts[0].read_data().unwrap();
        assert_eq!(m.price, 2 * PRICE_SCALE as u64);
        assert_eq!(m.price_updated_at, 500);
    }

    #[test]
    fn test_sync_price_wrong_feed_fails() {
        let mut wrong = feed_acct(PRICE_SCALE as u64, 1);
        wrong.key = Pubkey::from_seed(b"other-feed"); // not the market's configured feed
        let mut accounts = vec![market_acct(0, 0, 0), wrong];
        let ix = borsh::to_vec(&MoneyMarketInstruction::SyncPrice).unwrap();
        assert_eq!(
            process(&mut accounts, &ix, 1),
            Err(ProgramError::InvalidAccountData)
        );
    }

    #[test]
    fn test_borrow_with_stale_price_fails() {
        // Tighten the staleness window so the (ts=0) price is stale at ts=1000.
        let mut market = market_acct(1_000, 0, 10_000);
        let mut m: MarketState = market.read_data().unwrap();
        m.max_price_staleness = 60;
        m.price_updated_at = 0;
        market.write_data(&m).unwrap();

        let mut accounts = vec![
            market,
            position_acct(1_000, 0),
            token_acct("user-b", mint_b(), user_k(), 0),
            token_acct("vault-b", mint_b(), market_k(), 10_000),
            user(),
        ];
        let ix = borsh::to_vec(&MoneyMarketInstruction::Borrow { amount: 100 }).unwrap();
        assert_eq!(
            process(&mut accounts, &ix, 1_000),
            Err(ProgramError::StalePrice)
        );
    }

    #[test]
    fn test_supply_moves_collateral() {
        let mut accounts = vec![
            market_acct(0, 0, 0),
            position_acct(0, 0),
            token_acct("user-c", mint_c(), user_k(), 1_000),
            token_acct("vault-c", mint_c(), market_k(), 0),
            user(),
        ];
        let ix = borsh::to_vec(&MoneyMarketInstruction::Supply { amount: 1_000 }).unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        assert_eq!(bal(&accounts[2]), 0); // user collateral spent
        assert_eq!(bal(&accounts[3]), 1_000); // vault funded
        let pos: Position = accounts[1].read_data().unwrap();
        assert_eq!(pos.collateral, 1_000);
        assert_eq!(pos.owner, user_k());
    }

    #[test]
    fn test_borrow_within_ltv() {
        // 1000 collateral @ price 1.0, 75% LTV → can borrow up to 750.
        let mut accounts = vec![
            market_acct(1_000, 0, 10_000),
            position_acct(1_000, 0),
            token_acct("user-b", mint_b(), user_k(), 0),
            token_acct("vault-b", mint_b(), market_k(), 10_000),
            user(),
        ];
        let ix = borsh::to_vec(&MoneyMarketInstruction::Borrow { amount: 700 }).unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        assert_eq!(bal(&accounts[2]), 700); // user received borrow
        assert_eq!(bal(&accounts[3]), 9_300); // vault drained
        let pos: Position = accounts[1].read_data().unwrap();
        assert_eq!(pos.debt, 700);
    }

    #[test]
    fn test_borrow_exceeding_ltv_fails() {
        let mut accounts = vec![
            market_acct(1_000, 0, 10_000),
            position_acct(1_000, 0),
            token_acct("user-b", mint_b(), user_k(), 0),
            token_acct("vault-b", mint_b(), market_k(), 10_000),
            user(),
        ];
        // 800 > 750 max → undercollateralized; nothing should move.
        let ix = borsh::to_vec(&MoneyMarketInstruction::Borrow { amount: 800 }).unwrap();
        assert_eq!(
            process(&mut accounts, &ix, 0),
            Err(ProgramError::Undercollateralized)
        );
        assert_eq!(bal(&accounts[3]), 10_000);
    }

    #[test]
    fn test_borrow_illiquid_vault_fails() {
        let mut accounts = vec![
            market_acct(1_000, 0, 10_000),
            position_acct(1_000, 0),
            token_acct("user-b", mint_b(), user_k(), 0),
            token_acct("vault-b", mint_b(), market_k(), 100), // only 100 available
            user(),
        ];
        let ix = borsh::to_vec(&MoneyMarketInstruction::Borrow { amount: 700 }).unwrap();
        assert_eq!(
            process(&mut accounts, &ix, 0),
            Err(ProgramError::InsufficientLiquidity)
        );
    }

    #[test]
    fn test_repay_reduces_debt() {
        let mut accounts = vec![
            market_acct(1_000, 700, 9_300),
            position_acct(1_000, 700),
            token_acct("user-b", mint_b(), user_k(), 1_000),
            token_acct("vault-b", mint_b(), market_k(), 9_300),
            user(),
        ];
        let ix = borsh::to_vec(&MoneyMarketInstruction::Repay { amount: 500 }).unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        assert_eq!(bal(&accounts[2]), 500); // user paid 500
        assert_eq!(bal(&accounts[3]), 9_800); // vault repaid
        let pos: Position = accounts[1].read_data().unwrap();
        assert_eq!(pos.debt, 200);
    }

    #[test]
    fn test_repay_caps_at_debt() {
        let mut accounts = vec![
            market_acct(1_000, 200, 9_800),
            position_acct(1_000, 200),
            token_acct("user-b", mint_b(), user_k(), 1_000),
            token_acct("vault-b", mint_b(), market_k(), 9_800),
            user(),
        ];
        // Try to over-repay; only the 200 debt is taken.
        let ix = borsh::to_vec(&MoneyMarketInstruction::Repay { amount: 500 }).unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        assert_eq!(bal(&accounts[2]), 800); // only 200 moved
        let pos: Position = accounts[1].read_data().unwrap();
        assert_eq!(pos.debt, 0);
    }

    #[test]
    fn test_withdraw_unhealthy_fails() {
        // 1000 collateral, 700 debt. Withdrawing 200 leaves 800 collateral →
        // max borrow 600 < 700 debt → must fail.
        let mut accounts = vec![
            market_acct(1_000, 700, 9_300),
            position_acct(1_000, 700),
            token_acct("user-c", mint_c(), user_k(), 0),
            token_acct("vault-c", mint_c(), market_k(), 1_000),
            user(),
        ];
        let ix = borsh::to_vec(&MoneyMarketInstruction::Withdraw { amount: 200 }).unwrap();
        assert_eq!(
            process(&mut accounts, &ix, 0),
            Err(ProgramError::Undercollateralized)
        );
        assert_eq!(bal(&accounts[3]), 1_000); // nothing moved
    }

    #[test]
    fn test_withdraw_healthy_ok() {
        // 1000 collateral, 300 debt. Withdraw 500 → 500 left, max borrow 375 ≥ 300.
        let mut accounts = vec![
            market_acct(1_000, 300, 0),
            position_acct(1_000, 300),
            token_acct("user-c", mint_c(), user_k(), 0),
            token_acct("vault-c", mint_c(), market_k(), 1_000),
            user(),
        ];
        let ix = borsh::to_vec(&MoneyMarketInstruction::Withdraw { amount: 500 }).unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        assert_eq!(bal(&accounts[2]), 500); // user got collateral back
        let pos: Position = accounts[1].read_data().unwrap();
        assert_eq!(pos.collateral, 500);
    }

    // ---- interest accrual (utilization model) ----

    #[test]
    fn test_utilization_and_rate() {
        // borrows 700, cash 300 → utilization 70%.
        // rate = base(200) + slope(1000)*0.70 = 200 + 700 = 900 bps.
        let m: MarketState = market_acct_rates(1_000, 700, 300, 200, 1000)
            .read_data()
            .unwrap();
        assert_eq!(utilization_bps(&m), 7000);
        assert_eq!(borrow_rate_bps(&m), 900);
    }

    #[test]
    fn test_interest_accrues_over_one_year() {
        // 700 borrowed, 300 cash, base 2% + slope 10% → 9% APR at 70% util.
        // After 1 year the borrow grows by 9%: 700 -> 763.
        let mut accounts = vec![
            market_acct_rates(1_000, 700, 300, 200, 1000),
            position_acct(1_000, 700),
            token_acct("user-b", mint_b(), user_k(), 1_000),
            token_acct("vault-b", mint_b(), market_k(), 300),
            user(),
        ];
        // "Poke" the market with a zero repay one year later to force accrual.
        let ix = borsh::to_vec(&MoneyMarketInstruction::Repay { amount: 0 }).unwrap();
        process(&mut accounts, &ix, SECONDS_PER_YEAR as u64).unwrap();

        let m: MarketState = accounts[0].read_data().unwrap();
        let pos: Position = accounts[1].read_data().unwrap();
        assert_eq!(m.total_borrows, 763); // +9%
        assert_eq!(pos.debt, 763); // position debt reconciled to index
        assert_eq!(m.borrow_index, INDEX_SCALE * 10_900 / 10_000); // 1.09x
    }

    #[test]
    fn test_repay_clears_accrued_debt() {
        // Same setup; after a year debt is 763. Repaying 763 fully clears it.
        let mut accounts = vec![
            market_acct_rates(1_000, 700, 300, 200, 1000),
            position_acct(1_000, 700),
            token_acct("user-b", mint_b(), user_k(), 1_000),
            token_acct("vault-b", mint_b(), market_k(), 300),
            user(),
        ];
        let ix = borsh::to_vec(&MoneyMarketInstruction::Repay { amount: 10_000 }).unwrap();
        process(&mut accounts, &ix, SECONDS_PER_YEAR as u64).unwrap();

        let pos: Position = accounts[1].read_data().unwrap();
        assert_eq!(pos.debt, 0);
        assert_eq!(bal(&accounts[2]), 1_000 - 763); // user paid the accrued 763
        assert_eq!(bal(&accounts[3]), 300 + 763); // vault received it
    }

    /// 5-account window for AddLiquidity/RemoveLiquidity:
    /// [0]=market, [1]=lender_position, [2]=provider_borrow, [3]=borrow_vault, [4]=provider.
    fn liquidity_accounts(
        market: AccountInfo,
        lender_pos: AccountInfo,
        provider_bal: u64,
        vault_bal: u64,
    ) -> Vec<AccountInfo> {
        vec![
            market,
            lender_pos,
            token_acct("lender-b", mint_b(), lender_k(), provider_bal),
            token_acct("vault-b", mint_b(), market_k(), vault_bal),
            lender(),
        ]
    }

    #[test]
    fn test_add_liquidity_mints_shares_and_cash() {
        let mut accounts = liquidity_accounts(market_acct(0, 0, 0), lender_pos_acct(0), 5_000, 0);
        let ix = borsh::to_vec(&MoneyMarketInstruction::AddLiquidity { amount: 5_000 }).unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        assert_eq!(bal(&accounts[3]), 5_000); // vault funded
        let m: MarketState = accounts[0].read_data().unwrap();
        assert_eq!(m.total_cash, 5_000);
        assert_eq!(m.total_lender_shares, 5_000); // 1:1 bootstrap
        assert_eq!(lender_shares(&accounts[1]), 5_000);
    }

    #[test]
    fn test_remove_liquidity_round_trip() {
        // Add 5_000, then withdraw 2_000 back; shares burn 1:1 (no interest yet).
        let mut accounts = liquidity_accounts(market_acct(0, 0, 0), lender_pos_acct(0), 5_000, 0);
        process(
            &mut accounts,
            &borsh::to_vec(&MoneyMarketInstruction::AddLiquidity { amount: 5_000 }).unwrap(),
            0,
        )
        .unwrap();
        process(
            &mut accounts,
            &borsh::to_vec(&MoneyMarketInstruction::RemoveLiquidity { amount: 2_000 }).unwrap(),
            0,
        )
        .unwrap();
        assert_eq!(bal(&accounts[2]), 2_000); // lender got 2_000 back
        assert_eq!(bal(&accounts[3]), 3_000); // vault left with 3_000
        let m: MarketState = accounts[0].read_data().unwrap();
        assert_eq!(m.total_cash, 3_000);
        assert_eq!(m.total_lender_shares, 3_000);
        assert_eq!(lender_shares(&accounts[1]), 3_000);
    }

    #[test]
    fn test_lender_shares_earn_interest() {
        // 1_000 shares against a pool that grew to 1_063 (borrowers repaid with
        // interest → all cash, no debt). Each share is now worth >1.0 underlying.
        let m: MarketState = market_with_lenders(0, 1_063, 1_000).read_data().unwrap();
        assert_eq!(lender_share_value(&m, 1_000), 1_063);

        // Redeeming the full 1_063 burns all 1_000 shares.
        let mut accounts = liquidity_accounts(
            market_with_lenders(0, 1_063, 1_000),
            lender_pos_acct(1_000),
            0,
            1_063,
        );
        process(
            &mut accounts,
            &borsh::to_vec(&MoneyMarketInstruction::RemoveLiquidity { amount: 1_063 }).unwrap(),
            0,
        )
        .unwrap();
        assert_eq!(bal(&accounts[2]), 1_063); // lender redeemed 1_063 for 1_000 shares → yield
        assert_eq!(lender_shares(&accounts[1]), 0);
        let m2: MarketState = accounts[0].read_data().unwrap();
        assert_eq!(m2.total_lender_shares, 0);
    }

    #[test]
    fn test_remove_liquidity_capped_by_cash() {
        // Pool value 1_000 (300 cash + 700 lent out). Lender owns all 1_000 shares
        // but can only pull the 300 cash on hand; the rest waits for repayments.
        let mut accounts = liquidity_accounts(
            market_with_lenders(700, 300, 1_000),
            lender_pos_acct(1_000),
            0,
            300,
        );
        let ix = borsh::to_vec(&MoneyMarketInstruction::RemoveLiquidity { amount: 400 }).unwrap();
        assert_eq!(
            process(&mut accounts, &ix, 0),
            Err(ProgramError::InsufficientLiquidity)
        );
        assert_eq!(bal(&accounts[3]), 300); // vault untouched
    }

    #[test]
    fn test_interest_pushes_position_unhealthy() {
        // Borrow at the limit, then accrue interest so debt exceeds borrowing power.
        // 1000 collateral, 750 debt (exactly 75% LTV). After interest, debt > 750,
        // so any further borrow must be rejected as undercollateralized.
        let mut accounts = vec![
            market_acct_rates(1_000, 750, 250, 200, 1000),
            position_acct(1_000, 750),
            token_acct("user-b", mint_b(), user_k(), 0),
            token_acct("vault-b", mint_b(), market_k(), 250),
            user(),
        ];
        let ix = borsh::to_vec(&MoneyMarketInstruction::Borrow { amount: 1 }).unwrap();
        assert_eq!(
            process(&mut accounts, &ix, SECONDS_PER_YEAR as u64),
            Err(ProgramError::Undercollateralized),
        );
    }

    // ---- liquidation ----

    fn liquidator_k() -> Pubkey {
        Pubkey::from_seed(b"liquidator")
    }

    /// 7-account window for Liquidate:
    /// [0]=market, [1]=position, [2]=liq_borrow, [3]=borrow_vault,
    /// [4]=liq_collateral, [5]=collateral_vault, [6]=liquidator.
    fn liquidate_accounts(
        collateral: u64,
        debt: u64,
        vault_cash: u64,
        vault_collateral: u64,
        liq_borrow_bal: u64,
    ) -> Vec<AccountInfo> {
        vec![
            market_acct(collateral, debt, vault_cash),
            position_acct(collateral, debt),
            token_acct("liq-b", mint_b(), liquidator_k(), liq_borrow_bal),
            token_acct("vault-b", mint_b(), market_k(), vault_cash),
            token_acct("liq-c", mint_c(), liquidator_k(), 0),
            token_acct("vault-c", mint_c(), market_k(), vault_collateral),
            AccountInfo::new(liquidator_k(), mm_prog(), 0, 0).as_signer(),
        ]
    }

    #[test]
    fn test_liquidate_healthy_position_fails() {
        // 1000 collateral, 700 debt, threshold 80% → max_debt 800 ≥ 700 → healthy.
        let mut accounts = liquidate_accounts(1_000, 700, 300, 1_000, 1_000);
        let ix = borsh::to_vec(&MoneyMarketInstruction::Liquidate { repay_amount: 400 }).unwrap();
        assert_eq!(
            process(&mut accounts, &ix, 0),
            Err(ProgramError::Unauthorized)
        );
        assert_eq!(bal(&accounts[5]), 1_000); // collateral vault untouched
    }

    #[test]
    fn test_liquidate_unhealthy_seizes_collateral_with_bonus() {
        // 1000 collateral, 850 debt > 800 threshold → liquidatable.
        // Repay 400 → seize 400 * (1 + 5% bonus) = 420 collateral.
        let mut accounts = liquidate_accounts(1_000, 850, 150, 1_000, 1_000);
        let ix = borsh::to_vec(&MoneyMarketInstruction::Liquidate { repay_amount: 400 }).unwrap();
        process(&mut accounts, &ix, 0).unwrap();

        assert_eq!(bal(&accounts[2]), 600); // liquidator paid 400
        assert_eq!(bal(&accounts[3]), 550); // borrow vault received 400
        assert_eq!(bal(&accounts[4]), 420); // liquidator seized 420 collateral
        assert_eq!(bal(&accounts[5]), 580); // collateral vault released 420

        let pos: Position = accounts[1].read_data().unwrap();
        assert_eq!(pos.debt, 450);
        assert_eq!(pos.collateral, 580);
        let m: MarketState = accounts[0].read_data().unwrap();
        assert_eq!(m.total_borrows, 450);
        assert_eq!(m.total_cash, 550);
        assert_eq!(m.total_collateral, 580);
    }

    #[test]
    fn test_liquidate_seize_capped_at_collateral() {
        // Deeply underwater: 1000 collateral, 2000 debt. Repaying 1000 would seize
        // 1050 (with bonus) but only 1000 collateral exists → capped at 1000.
        let mut accounts = liquidate_accounts(1_000, 2_000, 0, 1_000, 2_000);
        let ix = borsh::to_vec(&MoneyMarketInstruction::Liquidate {
            repay_amount: 1_000,
        })
        .unwrap();
        process(&mut accounts, &ix, 0).unwrap();

        assert_eq!(bal(&accounts[4]), 1_000); // liquidator got all collateral
        assert_eq!(bal(&accounts[5]), 0); // vault drained
        let pos: Position = accounts[1].read_data().unwrap();
        assert_eq!(pos.collateral, 0);
        assert_eq!(pos.debt, 1_000); // 2000 - 1000 repaid
    }

    #[test]
    fn test_liquidate_after_interest_makes_unhealthy() {
        // Healthy at 760 debt (≤ 800 threshold). After a year of 9%+ interest the
        // debt crosses the threshold and becomes liquidatable.
        let mut accounts = vec![
            market_acct_rates(1_000, 760, 240, 200, 1000),
            position_acct(1_000, 760),
            token_acct("liq-b", mint_b(), liquidator_k(), 1_000),
            token_acct("vault-b", mint_b(), market_k(), 240),
            token_acct("liq-c", mint_c(), liquidator_k(), 0),
            token_acct("vault-c", mint_c(), market_k(), 1_000),
            AccountInfo::new(liquidator_k(), mm_prog(), 0, 0).as_signer(),
        ];
        let ix = borsh::to_vec(&MoneyMarketInstruction::Liquidate { repay_amount: 100 }).unwrap();
        process(&mut accounts, &ix, SECONDS_PER_YEAR as u64).unwrap();
        // Liquidation succeeded → some collateral was seized.
        assert!(bal(&accounts[4]) > 0);
    }
}

//! HIMSHA Yield Vault — automated yield over a strategy (ERC-4626-style).
//!
//! Depositors put an `asset` token into the vault and receive `share` tokens that
//! represent a proportional claim on the vault's total assets. As the underlying
//! strategy earns yield, the assets backing each share grow, so shares become
//! redeemable for more than was deposited.
//!
//! Accounting:
//!   shares_out = amount * total_shares / total_assets   (1:1 on the first deposit)
//!   assets_out = shares * total_assets / total_shares
//!
//! Yield enters via `Report`: a keeper (or the strategy) deposits earned tokens into
//! the vault's asset vault, then calls `Report`, which syncs `total_assets` to the
//! actual vault balance and mints performance-fee shares to the manager on the profit.
//!
//! Token movements (asset in/out, share mint/burn) are real CPI calls into the token
//! program. The vault signs for its own vault authority via `invoke_signed`.
//!
//! Strategy deployment: `Deploy` / `Undeploy` lend idle assets into a HIMSHA money
//! market's supply side and redeem them, via real CPI into the money-market program —
//! the vault signs as the lender. The vault holds money-market lender shares (a
//! cToken) whose redeemable value grows as borrowers pay interest, so the deployed
//! leg genuinely earns yield. `Report` re-prices the lender shares to current value
//! and books the gain. A depositor `Withdraw` auto-`Undeploy`s the shortfall from the
//! market when idle balance can't cover the redemption. See docs/use-cases/yield-vaults.md.

use borsh::{BorshDeserialize, BorshSerialize};
use himsha_runtime::{
    account::{AccountInfo, AccountMeta},
    cpi,
    error::ProgramError,
    instruction::Instruction,
    pubkey::Pubkey,
};
use himsha_token_program::{process as token_process, TokenAccountState, TokenInstruction};
use himsha_money_market_program::{
    process as mm_process, lender_share_value, LenderPosition, MarketState, MoneyMarketInstruction,
};

/// Basis-points denominator.
pub const BPS: u128 = 10_000;
/// Shares permanently locked on the first deposit (first-depositor inflation guard).
pub const MIN_SHARES: u64 = 1000;

// ---- state ----

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct VaultState {
    pub asset_mint:  Pubkey,
    pub share_mint:  Pubkey,
    /// Token account holding the vault's assets.
    pub asset_vault: Pubkey,
    /// Authority allowed to call `Report` (the keeper/manager).
    pub manager:     Pubkey,
    /// Cached assets under management. Invariant (right after Deploy/Undeploy/Report):
    /// `total_assets == idle + lender_share_value(market, lender_shares)`, where `idle`
    /// is the asset_vault balance. Between reports the deployed leg accrues yield that
    /// isn't reflected here until the next `Report`.
    pub total_assets: u64,
    /// Money-market lender shares (cToken) the vault holds from deploying idle assets
    /// into a market's supply side. Their redeemable value grows as borrowers pay
    /// interest — this is the vault's actual yield source.
    pub lender_shares: u64,
    /// Outstanding vault shares (includes the locked MIN_SHARES; ≥ share_mint.supply).
    pub total_shares: u64,
    /// Performance fee on reported profit, in bps.
    pub performance_fee_bps: u64,
    pub is_initialized: bool,
}

impl VaultState {
    /// Value of one share scaled by 1e9 (for display/inspection).
    pub fn share_price_1e9(&self) -> u64 {
        if self.total_shares == 0 { return 1_000_000_000; }
        ((self.total_assets as u128) * 1_000_000_000 / self.total_shares as u128) as u64
    }
}

// ---- instructions ----

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum VaultInstruction {
    /// Initialize a vault.
    /// [0]=vault (w), [1]=asset_mint, [2]=share_mint, [3]=asset_vault, [4]=manager (signer).
    InitVault { performance_fee_bps: u64 },

    /// Deposit `amount` assets, mint shares to the depositor.
    /// [0]=vault (w), [1]=user_asset (w), [2]=asset_vault (w),
    /// [3]=user_shares (w), [4]=share_mint (w), [5]=user (signer),
    /// [6]=mm_market (optional). When the vault holds lender shares, passing the
    /// market re-prices shares off live NAV so deposits don't under-price.
    Deposit { amount: u64, min_shares: u64 },

    /// Redeem `shares`, return assets to the depositor.
    /// [0]=vault (w), [1]=user_asset (w), [2]=asset_vault (w),
    /// [3]=user_shares (w), [4]=share_mint (w), [5]=user (signer).
    Withdraw { shares: u64, min_assets: u64 },

    /// Sync NAV (idle + current lender-share value) and mint performance-fee shares
    /// on any profit. Called by the keeper. The money market is required only when
    /// the vault holds lender shares.
    /// [0]=vault (w), [1]=manager (signer), [2]=asset_vault,
    /// [3]=share_mint (w), [4]=manager_shares (w), [5]=mm_market (optional).
    Report,

    /// Lend `amount` idle assets into a money market's supply side (CPI AddLiquidity),
    /// minting lender shares to the vault. Manager-gated. Moves funds idle→deployed;
    /// NAV is unchanged at deploy time.
    /// [0]=vault (w), [1]=asset_vault (w), [2]=mm_market (w), [3]=mm_lender_position (w),
    /// [4]=mm_borrow_vault (w), [5]=manager (signer).
    Deploy { amount: u64 },

    /// Redeem lender shares worth `amount` back from the money market (CPI
    /// RemoveLiquidity). Manager-gated. Moves funds deployed→idle.
    /// [0]=vault (w), [1]=asset_vault (w), [2]=mm_market (w), [3]=mm_lender_position (w),
    /// [4]=mm_borrow_vault (w), [5]=manager (signer).
    Undeploy { amount: u64 },
}

// ---- builders ----

fn program() -> Pubkey { himsha_runtime::program_ids::vault_program() }

pub fn init_vault(
    vault: Pubkey, asset_mint: Pubkey, share_mint: Pubkey, asset_vault: Pubkey,
    manager: Pubkey, performance_fee_bps: u64,
) -> Instruction {
    Instruction::with_args(
        program(),
        vec![
            AccountMeta::writable(vault, false),
            AccountMeta::readonly(asset_mint, false),
            AccountMeta::readonly(share_mint, false),
            AccountMeta::writable(asset_vault, false),
            AccountMeta::readonly(manager, true),
        ],
        &VaultInstruction::InitVault { performance_fee_bps },
    )
}

#[allow(clippy::too_many_arguments)]
fn user_ix(tag: VaultInstruction, vault: Pubkey, user_asset: Pubkey, asset_vault: Pubkey,
    user_shares: Pubkey, share_mint: Pubkey, user: Pubkey) -> Instruction {
    Instruction::with_args(
        program(),
        vec![
            AccountMeta::writable(vault, false),
            AccountMeta::writable(user_asset, false),
            AccountMeta::writable(asset_vault, false),
            AccountMeta::writable(user_shares, false),
            AccountMeta::writable(share_mint, false),
            AccountMeta::readonly(user, true),
        ],
        &tag,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn deposit(vault: Pubkey, user_asset: Pubkey, asset_vault: Pubkey, user_shares: Pubkey,
    share_mint: Pubkey, user: Pubkey, amount: u64, min_shares: u64) -> Instruction {
    user_ix(VaultInstruction::Deposit { amount, min_shares }, vault, user_asset, asset_vault, user_shares, share_mint, user)
}

#[allow(clippy::too_many_arguments)]
pub fn withdraw(vault: Pubkey, user_asset: Pubkey, asset_vault: Pubkey, user_shares: Pubkey,
    share_mint: Pubkey, user: Pubkey, shares: u64, min_assets: u64) -> Instruction {
    user_ix(VaultInstruction::Withdraw { shares, min_assets }, vault, user_asset, asset_vault, user_shares, share_mint, user)
}

pub fn report(vault: Pubkey, manager: Pubkey, asset_vault: Pubkey, share_mint: Pubkey, manager_shares: Pubkey) -> Instruction {
    Instruction::with_args(
        program(),
        vec![
            AccountMeta::writable(vault, false),
            AccountMeta::readonly(manager, true),
            AccountMeta::readonly(asset_vault, false),
            AccountMeta::writable(share_mint, false),
            AccountMeta::writable(manager_shares, false),
        ],
        &VaultInstruction::Report,
    )
}

/// `Report` for a vault that holds lender shares: pass the money market so the
/// deployed leg can be re-priced to its current value.
pub fn report_with_market(vault: Pubkey, manager: Pubkey, asset_vault: Pubkey, share_mint: Pubkey, manager_shares: Pubkey, mm_market: Pubkey) -> Instruction {
    Instruction::with_args(
        program(),
        vec![
            AccountMeta::writable(vault, false),
            AccountMeta::readonly(manager, true),
            AccountMeta::readonly(asset_vault, false),
            AccountMeta::writable(share_mint, false),
            AccountMeta::writable(manager_shares, false),
            AccountMeta::readonly(mm_market, false),
        ],
        &VaultInstruction::Report,
    )
}

#[allow(clippy::too_many_arguments)]
fn strategy_ix(tag: VaultInstruction, vault: Pubkey, asset_vault: Pubkey, mm_market: Pubkey,
    mm_lender_position: Pubkey, mm_borrow_vault: Pubkey, manager: Pubkey) -> Instruction {
    Instruction::with_args(
        program(),
        vec![
            AccountMeta::writable(vault, false),
            AccountMeta::writable(asset_vault, false),
            AccountMeta::writable(mm_market, false),
            AccountMeta::writable(mm_lender_position, false),
            AccountMeta::writable(mm_borrow_vault, false),
            AccountMeta::readonly(manager, true),
        ],
        &tag,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn deploy(vault: Pubkey, asset_vault: Pubkey, mm_market: Pubkey, mm_lender_position: Pubkey,
    mm_borrow_vault: Pubkey, manager: Pubkey, amount: u64) -> Instruction {
    strategy_ix(VaultInstruction::Deploy { amount }, vault, asset_vault, mm_market, mm_lender_position, mm_borrow_vault, manager)
}

#[allow(clippy::too_many_arguments)]
pub fn undeploy(vault: Pubkey, asset_vault: Pubkey, mm_market: Pubkey, mm_lender_position: Pubkey,
    mm_borrow_vault: Pubkey, manager: Pubkey, amount: u64) -> Instruction {
    strategy_ix(VaultInstruction::Undeploy { amount }, vault, asset_vault, mm_market, mm_lender_position, mm_borrow_vault, manager)
}

// ---- CPI helpers ----

fn balance(acc: &AccountInfo) -> Result<u64, ProgramError> {
    Ok(acc.read_data::<TokenAccountState>()?.amount)
}

fn token_transfer(accounts: &mut [AccountInfo], src: usize, dst: usize, owner: usize, amount: u64) -> Result<(), ProgramError> {
    let ix = borsh::to_vec(&TokenInstruction::Transfer { amount }).map_err(|_| ProgramError::BorshError)?;
    cpi::invoke_indexed(accounts, &[src, dst, owner], &ix, token_process)
}

/// Transfer out of a vault-owned account; the vault signs as authority (window idx 2).
fn token_transfer_signed(accounts: &mut [AccountInfo], src: usize, dst: usize, owner: usize, amount: u64) -> Result<(), ProgramError> {
    let ix = borsh::to_vec(&TokenInstruction::Transfer { amount }).map_err(|_| ProgramError::BorshError)?;
    cpi::invoke_signed_indexed(accounts, &[src, dst, owner], &[2], &ix, token_process)
}

/// Mint shares; the vault signs as the share-mint authority (window idx 2).
fn mint_signed(accounts: &mut [AccountInfo], mint: usize, dst: usize, authority: usize, amount: u64) -> Result<(), ProgramError> {
    let ix = borsh::to_vec(&TokenInstruction::MintTo { amount }).map_err(|_| ProgramError::BorshError)?;
    cpi::invoke_signed_indexed(accounts, &[mint, dst, authority], &[2], &ix, token_process)
}

fn burn(accounts: &mut [AccountInfo], token: usize, mint: usize, owner: usize, amount: u64) -> Result<(), ProgramError> {
    let ix = borsh::to_vec(&TokenInstruction::Burn { amount }).map_err(|_| ProgramError::BorshError)?;
    cpi::invoke_indexed(accounts, &[token, mint, owner], &ix, token_process)
}

/// CPI into the money market's lender side. `idx` maps the vault's accounts onto the
/// money-market window `[market, lender_position, provider_borrow, borrow_vault,
/// provider]`. The vault state account is the lender (provider), so we sign for it
/// (money-market window position 4). `mm_process` needs the block `timestamp` for
/// interest accrual, so it's threaded through the closure.
fn mm_liquidity(accounts: &mut [AccountInfo], idx: [usize; 5], ix: &MoneyMarketInstruction, timestamp: u64) -> Result<(), ProgramError> {
    let data = borsh::to_vec(ix).map_err(|_| ProgramError::BorshError)?;
    cpi::invoke_signed_indexed(accounts, &idx, &[4], &data, |a, d| mm_process(a, d, timestamp))
}

/// Current lender-share balance recorded in a money-market lender-position account.
fn read_lender_shares(acc: &AccountInfo) -> u64 {
    acc.read_data::<LenderPosition>().map(|p| p.shares).unwrap_or(0)
}

// ---- processing ----

pub fn process(accounts: &mut [AccountInfo], data: &[u8], timestamp: u64) -> Result<(), ProgramError> {
    let ix = VaultInstruction::try_from_slice(data)
        .map_err(|_| ProgramError::InvalidInstruction)?;

    match ix {
        VaultInstruction::InitVault { performance_fee_bps } => {
            if accounts.len() < 5 { return Err(ProgramError::NotEnoughAccounts); }
            accounts[4].require_signer()?; // manager
            if performance_fee_bps > BPS as u64 { return Err(ProgramError::InvalidInstruction); }

            let mut vault: VaultState = accounts[0].read_data().unwrap_or_default();
            if vault.is_initialized { return Err(ProgramError::AlreadyInitialized); }
            vault.asset_mint  = accounts[1].key;
            vault.share_mint  = accounts[2].key;
            vault.asset_vault = accounts[3].key;
            vault.manager     = accounts[4].key;
            vault.performance_fee_bps = performance_fee_bps;
            vault.is_initialized = true;
            accounts[0].write_data(&vault)?;
        }

        VaultInstruction::Deposit { amount, min_shares } => {
            if accounts.len() < 6 { return Err(ProgramError::NotEnoughAccounts); }
            accounts[5].require_signer()?; // user
            if amount == 0 { return Err(ProgramError::InvalidInstruction); }
            let mut vault: VaultState = accounts[0].read_data()?;
            if !vault.is_initialized { return Err(ProgramError::NotInitialized); }

            // Price new shares off the *live* NAV (idle + current lender-share value)
            // rather than the cached `total_assets`, which lags accrued lender yield.
            // Using the stale cache would mint too many shares and dilute existing
            // holders of that unreported yield. Requires the optional market account;
            // falls back to the cache when the vault holds no lender shares.
            let nav_basis = if vault.lender_shares > 0 && accounts.len() >= 7 {
                let market: MarketState = accounts[6].read_data()?;
                balance(&accounts[2])?
                    .checked_add(lender_share_value(&market, vault.lender_shares))
                    .ok_or(ProgramError::Overflow)?
            } else {
                vault.total_assets
            };

            // shares to credit the user, and the total supply delta (bootstrap locks MIN_SHARES).
            let (user_shares, supply_delta) = if vault.total_shares == 0 {
                let u = amount.checked_sub(MIN_SHARES).ok_or(ProgramError::InsufficientFunds)?;
                (u, amount)
            } else {
                let s = ((amount as u128) * vault.total_shares as u128
                    / nav_basis.max(1) as u128) as u64;
                (s, s)
            };
            if user_shares < min_shares { return Err(ProgramError::SlippageExceeded); }

            // CPI: pull assets in, then mint shares to the depositor.
            token_transfer(accounts, 1, 2, 5, amount)?;           // user_asset -> asset_vault
            mint_signed(accounts, 4, 3, 0, user_shares)?;         // share_mint -> user_shares (vault signs)

            vault.total_assets = vault.total_assets.checked_add(amount).ok_or(ProgramError::Overflow)?;
            vault.total_shares = vault.total_shares.checked_add(supply_delta).ok_or(ProgramError::Overflow)?;
            accounts[0].write_data(&vault)?;
        }

        VaultInstruction::Withdraw { shares, min_assets } => {
            if accounts.len() < 6 { return Err(ProgramError::NotEnoughAccounts); }
            accounts[5].require_signer()?; // user
            let mut vault: VaultState = accounts[0].read_data()?;
            if vault.total_shares == 0 { return Err(ProgramError::NotInitialized); }

            let assets_out = ((shares as u128) * vault.total_assets as u128
                / vault.total_shares as u128) as u64;
            if assets_out < min_assets { return Err(ProgramError::SlippageExceeded); }

            // If idle balance can't cover the redemption, auto-undeploy the shortfall
            // from the money market — when the optional accounts [6]=mm_market,
            // [7]=mm_lender_position, [8]=mm_borrow_vault are supplied. Without them an
            // under-funded redemption simply fails on the asset transfer below.
            let idle = balance(&accounts[2])?;
            if idle < assets_out && accounts.len() >= 9 {
                let shortfall = assets_out - idle;
                // vault: [2]=asset_vault (provider_borrow), [6]=mm_market,
                // [7]=mm_lender_position, [8]=mm_borrow_vault, [0]=provider.
                mm_liquidity(accounts, [6, 7, 2, 8, 0], &MoneyMarketInstruction::RemoveLiquidity { amount: shortfall }, timestamp)?;
                vault.lender_shares = read_lender_shares(&accounts[7]);
            }

            // CPI: burn the user's shares, then send assets out (vault signs).
            burn(accounts, 3, 4, 5, shares)?;                     // user_shares burned by user
            token_transfer_signed(accounts, 2, 1, 0, assets_out)?; // asset_vault -> user_asset

            vault.total_assets = vault.total_assets.checked_sub(assets_out).ok_or(ProgramError::InsufficientFunds)?;
            vault.total_shares = vault.total_shares.checked_sub(shares).ok_or(ProgramError::InsufficientFunds)?;
            accounts[0].write_data(&vault)?;
        }

        VaultInstruction::Report => {
            if accounts.len() < 5 { return Err(ProgramError::NotEnoughAccounts); }
            accounts[1].require_signer()?; // manager
            let mut vault: VaultState = accounts[0].read_data()?;
            if !vault.is_initialized { return Err(ProgramError::NotInitialized); }
            if accounts[1].key != vault.manager { return Err(ProgramError::Unauthorized); }

            // NAV is the idle vault balance plus the current redeemable value of the
            // vault's lender shares (which grows with borrower interest). Profit is the
            // gain since the last report. The market account is required only when the
            // vault actually holds lender shares.
            let deployed_value = if vault.lender_shares > 0 {
                if accounts.len() < 6 { return Err(ProgramError::NotEnoughAccounts); }
                let market: MarketState = accounts[5].read_data()?;
                lender_share_value(&market, vault.lender_shares)
            } else {
                0
            };
            let nav = balance(&accounts[2])?
                .checked_add(deployed_value).ok_or(ProgramError::Overflow)?;
            let profit = nav.saturating_sub(vault.total_assets);

            if profit > 0 && vault.performance_fee_bps > 0 && vault.total_shares > 0 {
                let fee = (profit as u128) * vault.performance_fee_bps as u128 / BPS;
                if fee > 0 && (nav as u128) > fee {
                    // Dilution mint: keeps existing depositors whole minus the fee.
                    let fee_shares = (fee * vault.total_shares as u128 / (nav as u128 - fee)) as u64;
                    if fee_shares > 0 {
                        mint_signed(accounts, 3, 4, 0, fee_shares)?; // share_mint -> manager_shares
                        vault.total_shares = vault.total_shares.checked_add(fee_shares).ok_or(ProgramError::Overflow)?;
                    }
                }
            }

            vault.total_assets = nav;
            accounts[0].write_data(&vault)?;
        }

        VaultInstruction::Deploy { amount } => {
            if accounts.len() < 6 { return Err(ProgramError::NotEnoughAccounts); }
            accounts[5].require_signer()?; // manager authorizes
            if amount == 0 { return Err(ProgramError::InvalidInstruction); }
            let mut vault: VaultState = accounts[0].read_data()?;
            if !vault.is_initialized { return Err(ProgramError::NotInitialized); }
            if accounts[5].key != vault.manager { return Err(ProgramError::Unauthorized); }

            // CPI: lend `amount` of idle assets into the money market's supply side,
            // minting lender shares to the vault. Funds move idle → deployed; NAV is
            // unchanged at deploy time, so total_assets stays put.
            // vault: [1]=asset_vault, [2]=mm_market, [3]=mm_lender_position, [4]=mm_borrow_vault.
            mm_liquidity(accounts, [2, 3, 1, 4, 0], &MoneyMarketInstruction::AddLiquidity { amount }, timestamp)?;

            vault.lender_shares = read_lender_shares(&accounts[3]);
            accounts[0].write_data(&vault)?;
        }

        VaultInstruction::Undeploy { amount } => {
            if accounts.len() < 6 { return Err(ProgramError::NotEnoughAccounts); }
            accounts[5].require_signer()?; // manager authorizes
            if amount == 0 { return Err(ProgramError::InvalidInstruction); }
            let mut vault: VaultState = accounts[0].read_data()?;
            if !vault.is_initialized { return Err(ProgramError::NotInitialized); }
            if accounts[5].key != vault.manager { return Err(ProgramError::Unauthorized); }

            // CPI: redeem lender shares worth `amount` back into the idle asset vault.
            // The money market burns the right shares and caps by available liquidity.
            mm_liquidity(accounts, [2, 3, 1, 4, 0], &MoneyMarketInstruction::RemoveLiquidity { amount }, timestamp)?;

            vault.lender_shares = read_lender_shares(&accounts[3]);
            accounts[0].write_data(&vault)?;
        }
    }

    Ok(())
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use himsha_runtime::account::AccountState;
    use himsha_token_program::MintState;

    fn prog() -> Pubkey { himsha_runtime::program_ids::vault_program() }
    fn asset_mint() -> Pubkey { Pubkey::from_seed(b"asset-mint") }
    fn share_mint_k() -> Pubkey { Pubkey::from_seed(b"share-mint") }
    fn vault_k() -> Pubkey { Pubkey::from_seed(b"vault") }
    fn user_k() -> Pubkey { Pubkey::from_seed(b"user") }
    fn manager_k() -> Pubkey { Pubkey::from_seed(b"manager") }

    fn token_acct(key: &str, mint: Pubkey, owner: Pubkey, amount: u64) -> AccountInfo {
        let mut a = AccountInfo::new(Pubkey::from_seed(key.as_bytes()),
            himsha_runtime::program_ids::token_program(), 0, 256);
        a.write_data(&TokenAccountState {
            mint, owner, amount, delegate: None,
            state: AccountState::Initialized, delegated_amount: 0, close_authority: None,
        }).unwrap();
        a
    }

    fn share_mint_acct(supply: u64) -> AccountInfo {
        let mut a = AccountInfo::new(share_mint_k(), himsha_runtime::program_ids::token_program(), 0, 256);
        a.write_data(&MintState {
            mint_authority: Some(vault_k()), supply, decimals: 0,
            is_initialized: true, freeze_authority: None,
        }).unwrap();
        a
    }

    fn vault_acct(total_assets: u64, total_shares: u64, fee_bps: u64) -> AccountInfo {
        let mut a = AccountInfo::new(vault_k(), prog(), 0, 512);
        a.write_data(&VaultState {
            asset_mint: asset_mint(), share_mint: share_mint_k(),
            asset_vault: Pubkey::from_seed(b"asset-vault"), manager: manager_k(),
            total_assets, lender_shares: 0, total_shares, performance_fee_bps: fee_bps, is_initialized: true,
        }).unwrap();
        a
    }

    fn bal(a: &AccountInfo) -> u64 { a.read_data::<TokenAccountState>().unwrap().amount }
    fn shares(a: &AccountInfo) -> u64 { a.read_data::<MintState>().unwrap().supply }

    /// 6-account window for Deposit/Withdraw, with the vault at the given state.
    fn dw_accounts(total_assets: u64, total_shares: u64, user_asset: u64, user_shares: u64,
        vault_assets: u64, share_supply: u64) -> Vec<AccountInfo> {
        vec![
            vault_acct(total_assets, total_shares, 0),
            token_acct("user-asset",  asset_mint(),  user_k(),  user_asset),
            token_acct("asset-vault", asset_mint(),  vault_k(), vault_assets),
            token_acct("user-shares", share_mint_k(), user_k(), user_shares),
            { let mut m = share_mint_acct(share_supply); m.key = share_mint_k(); m },
            AccountInfo::new(user_k(), prog(), 0, 0).as_signer(),
        ]
    }

    #[test]
    fn test_init_vault() {
        let mut accounts = vec![
            AccountInfo::new(vault_k(), prog(), 0, 512),
            AccountInfo::new(asset_mint(), prog(), 0, 0),
            AccountInfo::new(share_mint_k(), prog(), 0, 0),
            AccountInfo::new(Pubkey::from_seed(b"asset-vault"), prog(), 0, 0),
            AccountInfo::new(manager_k(), prog(), 0, 0).as_signer(),
        ];
        let ix = borsh::to_vec(&VaultInstruction::InitVault { performance_fee_bps: 1000 }).unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        let v: VaultState = accounts[0].read_data().unwrap();
        assert!(v.is_initialized);
        assert_eq!(v.manager, manager_k());
        assert_eq!(v.performance_fee_bps, 1000);
    }

    #[test]
    fn test_deposit_bootstrap_locks_min_shares() {
        let mut accounts = dw_accounts(0, 0, 1_000_000, 0, 0, 0);
        let ix = borsh::to_vec(&VaultInstruction::Deposit { amount: 1_000_000, min_shares: 1 }).unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        assert_eq!(bal(&accounts[1]), 0);             // user spent assets
        assert_eq!(bal(&accounts[2]), 1_000_000);     // vault funded
        assert_eq!(bal(&accounts[3]), 999_000);       // user got shares (− MIN_SHARES)
        let v: VaultState = accounts[0].read_data().unwrap();
        assert_eq!(v.total_assets, 1_000_000);
        assert_eq!(v.total_shares, 1_000_000);        // includes locked MIN_SHARES
    }

    #[test]
    fn test_deposit_proportional() {
        // Vault already at 1:1 (assets==shares). A 500 deposit mints 500 shares.
        let mut accounts = dw_accounts(1_000, 1_000, 500, 0, 1_000, 1_000);
        let ix = borsh::to_vec(&VaultInstruction::Deposit { amount: 500, min_shares: 1 }).unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        assert_eq!(bal(&accounts[3]), 500);
        let v: VaultState = accounts[0].read_data().unwrap();
        assert_eq!(v.total_assets, 1_500);
        assert_eq!(v.total_shares, 1_500);
    }

    #[test]
    fn test_deposit_after_yield_mints_fewer_shares() {
        // total_assets 2_000 but total_shares 1_000 (share price 2.0 after yield).
        // Depositing 1_000 assets should mint only 500 shares.
        let mut accounts = dw_accounts(2_000, 1_000, 1_000, 0, 2_000, 1_000);
        let ix = borsh::to_vec(&VaultInstruction::Deposit { amount: 1_000, min_shares: 1 }).unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        assert_eq!(bal(&accounts[3]), 500); // 1000 * 1000 / 2000
    }

    #[test]
    fn test_deposit_slippage() {
        let mut accounts = dw_accounts(2_000, 1_000, 1_000, 0, 2_000, 1_000);
        let ix = borsh::to_vec(&VaultInstruction::Deposit { amount: 1_000, min_shares: 600 }).unwrap();
        assert_eq!(process(&mut accounts, &ix, 0), Err(ProgramError::SlippageExceeded));
    }

    #[test]
    fn test_withdraw_returns_assets() {
        // User holds 500 shares of a 1_500-asset / 1_500-share vault → redeems 500 assets.
        let mut accounts = dw_accounts(1_500, 1_500, 0, 500, 1_500, 1_500);
        let ix = borsh::to_vec(&VaultInstruction::Withdraw { shares: 500, min_assets: 1 }).unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        assert_eq!(bal(&accounts[1]), 500);   // user received assets
        assert_eq!(bal(&accounts[2]), 1_000); // vault drained by 500
        assert_eq!(bal(&accounts[3]), 0);     // shares burned
        let v: VaultState = accounts[0].read_data().unwrap();
        assert_eq!(v.total_assets, 1_000);
        assert_eq!(v.total_shares, 1_000);
    }

    #[test]
    fn test_withdraw_after_yield_returns_more() {
        // Share price 2.0 (3_000 assets / 1_500 shares). 500 shares → 1_000 assets.
        let mut accounts = dw_accounts(3_000, 1_500, 0, 500, 3_000, 1_500);
        let ix = borsh::to_vec(&VaultInstruction::Withdraw { shares: 500, min_assets: 1 }).unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        assert_eq!(bal(&accounts[1]), 1_000); // 500 * 3000 / 1500
    }

    #[test]
    fn test_report_syncs_nav_and_takes_fee() {
        // Vault thinks it has 1_000 assets / 1_000 shares; actual balance grew to 1_200
        // (200 yield). 10% perf fee on 200 = 20 → fee shares minted to manager.
        let mut accounts = vec![
            vault_acct(1_000, 1_000, 1000),                         // [0] vault, 10% fee
            AccountInfo::new(manager_k(), prog(), 0, 0).as_signer(), // [1] manager
            token_acct("asset-vault", asset_mint(), vault_k(), 1_200), // [2] vault holds 1_200 now
            { let mut m = share_mint_acct(1_000); m.key = share_mint_k(); m }, // [3] share mint
            token_acct("mgr-shares", share_mint_k(), manager_k(), 0),  // [4] manager shares
        ];
        let ix = borsh::to_vec(&VaultInstruction::Report).unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        let v: VaultState = accounts[0].read_data().unwrap();
        assert_eq!(v.total_assets, 1_200);          // NAV synced up
        // fee = 20; fee_shares = 20 * 1000 / (1200 - 20) = 16
        let fee_shares = bal(&accounts[4]);
        assert_eq!(fee_shares, 16);
        assert_eq!(v.total_shares, 1_016);
    }

    #[test]
    fn test_report_wrong_manager_fails() {
        let mut accounts = vec![
            vault_acct(1_000, 1_000, 1000),
            AccountInfo::new(Pubkey::from_seed(b"intruder"), prog(), 0, 0).as_signer(),
            token_acct("asset-vault", asset_mint(), vault_k(), 1_200),
            { let mut m = share_mint_acct(1_000); m.key = share_mint_k(); m },
            token_acct("mgr-shares", share_mint_k(), manager_k(), 0),
        ];
        let ix = borsh::to_vec(&VaultInstruction::Report).unwrap();
        assert_eq!(process(&mut accounts, &ix, 0), Err(ProgramError::Unauthorized));
    }

    #[test]
    fn test_report_no_profit_no_fee() {
        let mut accounts = vec![
            vault_acct(1_000, 1_000, 1000),
            AccountInfo::new(manager_k(), prog(), 0, 0).as_signer(),
            token_acct("asset-vault", asset_mint(), vault_k(), 1_000), // unchanged
            { let mut m = share_mint_acct(1_000); m.key = share_mint_k(); m },
            token_acct("mgr-shares", share_mint_k(), manager_k(), 0),
        ];
        let ix = borsh::to_vec(&VaultInstruction::Report).unwrap();
        process(&mut accounts, &ix, 0).unwrap();
        assert_eq!(bal(&accounts[4]), 0); // no fee shares
        let _ = shares; // silence unused in some configs
    }

    #[test]
    fn test_deposit_requires_signer() {
        let mut accounts = dw_accounts(1_000, 1_000, 500, 0, 1_000, 1_000);
        accounts[5].is_signer = false; // user did not sign
        let ix = borsh::to_vec(&VaultInstruction::Deposit { amount: 500, min_shares: 1 }).unwrap();
        assert_eq!(process(&mut accounts, &ix, 0), Err(ProgramError::MissingSigner));
    }

    // ---- strategy deployment: lending into the money market (real CPI) ----

    fn mm_prog() -> Pubkey { himsha_runtime::program_ids::money_market_program() }
    fn mm_market_k() -> Pubkey { Pubkey::from_seed(b"mm-market") }
    fn vault_lender_pos_k() -> Pubkey { Pubkey::from_seed(b"vault-lender-pos") }

    /// A never-stale money market whose *borrow* mint is the vault's asset mint —
    /// so the vault can lend its asset on the supply side. Pre-seed the pool with
    /// `total_borrows`/`total_cash`/`total_lender_shares` to simulate accrued yield.
    fn mm_market_acct(total_borrows: u64, total_cash: u64, total_lender_shares: u64) -> AccountInfo {
        use himsha_money_market_program::{MarketState, INDEX_SCALE, PRICE_SCALE};
        let mut a = AccountInfo::new(mm_market_k(), mm_prog(), 0, 512);
        a.write_data(&MarketState {
            collateral_mint: Pubkey::from_seed(b"mm-coll-mint"),
            borrow_mint: asset_mint(),               // vault lends its asset
            collateral_vault: Pubkey::from_seed(b"mm-coll-vault"),
            borrow_vault: Pubkey::from_seed(b"mm-borrow-vault"),
            collateral_factor_bps: 7500,
            liquidation_threshold_bps: 8000,
            liquidation_bonus_bps: 500,
            price: PRICE_SCALE as u64,
            oracle_feed: Pubkey::from_seed(b"mm-feed"),
            price_updated_at: 0,
            max_price_staleness: u64::MAX,
            total_collateral: 0, total_borrows, total_cash,
            total_lender_shares,
            base_rate_bps: 0, slope_bps: 0,          // no live accrual in these tests
            borrow_index: INDEX_SCALE,
            last_accrual_ts: 0,
            is_initialized: true,
        }).unwrap();
        a
    }

    /// Vault state with explicit `lender_shares` (fee 0).
    fn vault_acct_shares(total_assets: u64, total_shares: u64, lender_shares: u64) -> AccountInfo {
        let mut a = AccountInfo::new(vault_k(), prog(), 0, 512);
        a.write_data(&VaultState {
            asset_mint: asset_mint(), share_mint: share_mint_k(),
            asset_vault: Pubkey::from_seed(b"asset-vault"), manager: manager_k(),
            total_assets, lender_shares, total_shares, performance_fee_bps: 0, is_initialized: true,
        }).unwrap();
        a
    }

    fn empty_lender_pos() -> AccountInfo {
        AccountInfo::new(vault_lender_pos_k(), mm_prog(), 0, 128)
    }

    /// The vault's lender position holding `shares`, owned by the vault authority.
    fn vault_lender_pos(shares: u64) -> AccountInfo {
        let mut a = AccountInfo::new(vault_lender_pos_k(), mm_prog(), 0, 128);
        a.write_data(&LenderPosition {
            owner: vault_k(), market: mm_market_k(), shares, is_initialized: true,
        }).unwrap();
        a
    }

    fn pos_shares(a: &AccountInfo) -> u64 { a.read_data::<LenderPosition>().unwrap().shares }

    /// 6-account Deploy/Undeploy window:
    /// [0]=vault, [1]=asset_vault, [2]=mm_market, [3]=mm_lender_position,
    /// [4]=mm_borrow_vault, [5]=manager.
    #[allow(clippy::too_many_arguments)]
    fn deploy_accounts(idle: u64, total_assets: u64, total_shares: u64, lender_shares: u64,
        mm_market: AccountInfo, mm_lender_pos: AccountInfo, mm_borrow_vault_bal: u64) -> Vec<AccountInfo> {
        vec![
            vault_acct_shares(total_assets, total_shares, lender_shares),
            token_acct("asset-vault", asset_mint(), vault_k(), idle),
            mm_market,
            mm_lender_pos,
            token_acct("mm-borrow-vault", asset_mint(), mm_market_k(), mm_borrow_vault_bal),
            AccountInfo::new(manager_k(), prog(), 0, 0).as_signer(),
        ]
    }

    #[test]
    fn test_deploy_lends_into_market() {
        // Vault holds 1_000 idle; lend 400 into a fresh market → 400 shares 1:1.
        let mut accounts = deploy_accounts(1_000, 1_000, 1_000, 0, mm_market_acct(0, 0, 0), empty_lender_pos(), 0);
        process(&mut accounts, &borsh::to_vec(&VaultInstruction::Deploy { amount: 400 }).unwrap(), 0).unwrap();

        assert_eq!(bal(&accounts[1]), 600);   // idle drained by 400
        assert_eq!(bal(&accounts[4]), 400);   // market's borrow vault funded
        let v: VaultState = accounts[0].read_data().unwrap();
        assert_eq!(v.lender_shares, 400);
        assert_eq!(v.total_assets, 1_000);    // NAV unchanged at deploy time
        assert_eq!(pos_shares(&accounts[3]), 400);
        let m: MarketState = accounts[2].read_data().unwrap();
        assert_eq!(m.total_cash, 400);
        assert_eq!(m.total_lender_shares, 400);
    }

    #[test]
    fn test_deploy_then_undeploy_round_trip() {
        let mut accounts = deploy_accounts(1_000, 1_000, 1_000, 0, mm_market_acct(0, 0, 0), empty_lender_pos(), 0);
        process(&mut accounts, &borsh::to_vec(&VaultInstruction::Deploy { amount: 400 }).unwrap(), 0).unwrap();
        process(&mut accounts, &borsh::to_vec(&VaultInstruction::Undeploy { amount: 400 }).unwrap(), 0).unwrap();

        assert_eq!(bal(&accounts[1]), 1_000); // all assets back to idle
        assert_eq!(bal(&accounts[4]), 0);     // market vault drained
        let v: VaultState = accounts[0].read_data().unwrap();
        assert_eq!(v.lender_shares, 0);
        assert_eq!(pos_shares(&accounts[3]), 0);
    }

    #[test]
    fn test_report_reprices_lender_shares_as_yield() {
        // The vault holds 1_000 lender shares (cached NAV 1_000, all deployed). The
        // pool grew to 1_063 underlying for 1_000 shares (borrowers paid interest).
        // Report must re-price NAV up to 1_063 — real yield, no token moved in.
        let mut accounts = vec![
            vault_acct_shares(1_000, 1_000, 1_000),
            AccountInfo::new(manager_k(), prog(), 0, 0).as_signer(),
            token_acct("asset-vault", asset_mint(), vault_k(), 0), // fully deployed
            { let mut m = share_mint_acct(1_000); m.key = share_mint_k(); m },
            token_acct("mgr-shares", share_mint_k(), manager_k(), 0),
            mm_market_acct(0, 1_063, 1_000),                       // [5] pool 1_063 / 1_000 shares
        ];
        process(&mut accounts, &borsh::to_vec(&VaultInstruction::Report).unwrap(), 0).unwrap();
        let v: VaultState = accounts[0].read_data().unwrap();
        assert_eq!(v.total_assets, 1_063); // lender shares re-priced → yield recognized
    }

    #[test]
    fn test_deploy_wrong_manager_fails() {
        let mut accounts = deploy_accounts(1_000, 1_000, 1_000, 0, mm_market_acct(0, 0, 0), empty_lender_pos(), 0);
        accounts[5] = AccountInfo::new(Pubkey::from_seed(b"intruder"), prog(), 0, 0).as_signer();
        let ix = borsh::to_vec(&VaultInstruction::Deploy { amount: 400 }).unwrap();
        assert_eq!(process(&mut accounts, &ix, 0), Err(ProgramError::Unauthorized));
        assert_eq!(bal(&accounts[1]), 1_000); // nothing moved
    }

    #[test]
    fn test_deploy_requires_signer() {
        let mut accounts = deploy_accounts(1_000, 1_000, 1_000, 0, mm_market_acct(0, 0, 0), empty_lender_pos(), 0);
        accounts[5].is_signer = false;
        let ix = borsh::to_vec(&VaultInstruction::Deploy { amount: 400 }).unwrap();
        assert_eq!(process(&mut accounts, &ix, 0), Err(ProgramError::MissingSigner));
    }

    #[test]
    fn test_undeploy_more_than_held_fails() {
        // Vault holds 400 lender shares; pulling 500 underlying would burn 500 shares
        // > 400 held → the money market rejects it and nothing moves.
        let mut accounts = deploy_accounts(600, 400, 1_000, 400, mm_market_acct(0, 400, 400), vault_lender_pos(400), 400);
        let ix = borsh::to_vec(&VaultInstruction::Undeploy { amount: 500 }).unwrap();
        assert_eq!(process(&mut accounts, &ix, 0), Err(ProgramError::InsufficientFunds));
        assert_eq!(bal(&accounts[4]), 400); // market vault untouched
    }

    #[test]
    fn test_withdraw_auto_undeploys_shortfall() {
        // Vault: NAV 1_000 / 1_000 shares, 600 lent out, only 400 idle. A user redeems
        // 500 shares → 500 assets, but idle is 400. The vault auto-undeploys the 100
        // shortfall from the market, then pays the user.
        let mut accounts = vec![
            vault_acct_shares(1_000, 1_000, 600),                          // [0]
            token_acct("user-asset", asset_mint(), user_k(), 0),            // [1]
            token_acct("asset-vault", asset_mint(), vault_k(), 400),        // [2] idle 400
            token_acct("user-shares", share_mint_k(), user_k(), 500),       // [3]
            { let mut m = share_mint_acct(1_000); m.key = share_mint_k(); m }, // [4]
            AccountInfo::new(user_k(), prog(), 0, 0).as_signer(),           // [5]
            mm_market_acct(0, 600, 600),                                    // [6] pool 600 / 600 shares
            vault_lender_pos(600),                                          // [7] vault holds 600 shares
            token_acct("mm-borrow-vault", asset_mint(), mm_market_k(), 600),// [8]
        ];
        let ix = borsh::to_vec(&VaultInstruction::Withdraw { shares: 500, min_assets: 1 }).unwrap();
        process(&mut accounts, &ix, 0).unwrap();

        assert_eq!(bal(&accounts[1]), 500);   // user received 500 assets
        assert_eq!(bal(&accounts[2]), 0);     // idle 400 + 100 undeployed - 500 out
        assert_eq!(bal(&accounts[8]), 500);   // market vault gave up the 100 shortfall
        let v: VaultState = accounts[0].read_data().unwrap();
        assert_eq!(v.lender_shares, 500);     // 600 - 100 burned
        assert_eq!(v.total_shares, 500);
        assert_eq!(v.total_assets, 500);
    }

    #[test]
    fn test_deposit_reprices_off_live_nav() {
        // Cached total_assets is a stale 1_000, but the vault's 1_000 lender shares
        // are now worth 1_200 (unreported yield). Depositing 1_200 with the market
        // supplied must mint 1_000 shares (fair), NOT 1_200 (which the stale cache
        // would mint, diluting existing holders).
        let mut accounts = vec![
            vault_acct_shares(1_000, 1_000, 1_000),                   // [0] cached NAV 1_000 (stale)
            token_acct("user-asset", asset_mint(), user_k(), 1_200),  // [1]
            token_acct("asset-vault", asset_mint(), vault_k(), 0),    // [2] idle 0
            token_acct("user-shares", share_mint_k(), user_k(), 0),   // [3]
            { let mut m = share_mint_acct(1_000); m.key = share_mint_k(); m }, // [4]
            AccountInfo::new(user_k(), prog(), 0, 0).as_signer(),     // [5]
            mm_market_acct(0, 1_200, 1_000),                          // [6] 1_000 shares worth 1_200
        ];
        process(&mut accounts, &borsh::to_vec(&VaultInstruction::Deposit { amount: 1_200, min_shares: 1 }).unwrap(), 0).unwrap();
        assert_eq!(bal(&accounts[3]), 1_000); // fair price off live NAV, not stale 1_200
    }

    #[test]
    fn test_deposit_without_market_uses_cached_nav() {
        // Same vault, but no market passed (6 accounts) → falls back to the cached
        // 1_000 basis, minting 1_200 shares. Confirms the fix is gated on the market.
        let mut accounts = dw_accounts(1_000, 1_000, 1_200, 0, 0, 1_000);
        // Give the vault lender shares but withhold the market account.
        let mut v: VaultState = accounts[0].read_data().unwrap();
        v.lender_shares = 1_000;
        accounts[0].write_data(&v).unwrap();
        process(&mut accounts, &borsh::to_vec(&VaultInstruction::Deposit { amount: 1_200, min_shares: 1 }).unwrap(), 0).unwrap();
        assert_eq!(bal(&accounts[3]), 1_200); // cached basis → 1_200 shares
    }
}

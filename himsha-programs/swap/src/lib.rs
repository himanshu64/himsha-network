//! HIMSHA Swap Program — Constant-Product AMM (x * y = k)
//!
//! Supports:
//!   - Pool initialization (pick two token mints)
//!   - Liquidity deposit (proportional, or bootstrap first deposit)
//!   - Liquidity withdrawal (proportional)
//!   - Token swap (with fee)
//!
//! Pool account stores `PoolState` (borsh).
//! LP token mint is created during `Initialize`.

use borsh::{BorshDeserialize, BorshSerialize};
use himsha_runtime::{
    account::{AccountInfo, AccountMeta},
    cpi,
    error::ProgramError,
    instruction::Instruction,
    pubkey::Pubkey,
};
use himsha_token_program::{process as token_process, TokenAccountState, TokenInstruction};

/// LP tokens permanently locked on the first (bootstrap) deposit. This burns a
/// tiny, unredeemable amount of liquidity so the pool can never be fully drained
/// to zero `lp_supply`, which defuses the classic first-depositor share-inflation
/// attack (and avoids divide-by-tiny-supply rounding exploits). Mirrors Uniswap V2.
pub const MINIMUM_LIQUIDITY: u64 = 1000;

// ---- pool state ----

#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
pub struct PoolState {
    pub token_a_mint: Pubkey,
    pub token_b_mint: Pubkey,
    pub token_a_reserve: Pubkey, // token account holding side A
    pub token_b_reserve: Pubkey, // token account holding side B
    pub lp_mint: Pubkey,
    pub fee_numerator: u64,   // e.g. 3
    pub fee_denominator: u64, // e.g. 1000 → 0.3 % fee
    pub reserve_a: u64,       // cached reserve amounts (updated on each tx)
    pub reserve_b: u64,
    pub lp_supply: u64,
    pub is_initialized: bool,
}

// ---- instructions ----

#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum SwapInstruction {
    /// Initialize a new pool.
    /// accounts[0] = pool (writable), [1] = token_a_mint, [2] = token_b_mint,
    /// [3] = reserve_a token account, [4] = reserve_b token account,
    /// [5] = lp_mint (writable), [6] = payer (signer).
    Initialize {
        fee_numerator: u64,
        fee_denominator: u64,
    },

    /// Swap `amount_in` of token A for at least `min_out` of token B (or vice-versa).
    /// accounts[0] = pool, [1] = source token account (user, writable),
    /// [2] = destination token account (user, writable),
    /// [3] = reserve_in (pool side, writable), [4] = reserve_out (pool side, writable),
    /// [5] = user (signer).
    Swap { amount_in: u64, min_amount_out: u64 },

    /// Deposit liquidity.  Mints LP tokens to accounts[5].
    /// accounts[0] = pool, [1] = user_token_a, [2] = user_token_b,
    /// [3] = reserve_a, [4] = reserve_b, [5] = user_lp, [6] = user (signer),
    /// [7] = lp_mint (writable).
    Deposit { max_a: u64, max_b: u64, min_lp: u64 },

    /// Withdraw liquidity by burning LP tokens.
    /// accounts[0] = pool, [1] = user_token_a, [2] = user_token_b,
    /// [3] = reserve_a, [4] = reserve_b, [5] = user_lp, [6] = user (signer),
    /// [7] = lp_mint (writable).
    Withdraw {
        lp_amount: u64,
        min_a: u64,
        min_b: u64,
    },
}

// ---- instruction builders ----

pub fn initialize(
    pool: Pubkey,
    token_a_mint: Pubkey,
    token_b_mint: Pubkey,
    reserve_a: Pubkey,
    reserve_b: Pubkey,
    lp_mint: Pubkey,
    payer: Pubkey,
    fee_num: u64,
    fee_den: u64,
) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::swap_program(),
        vec![
            AccountMeta::writable(pool, false),
            AccountMeta::readonly(token_a_mint, false),
            AccountMeta::readonly(token_b_mint, false),
            AccountMeta::writable(reserve_a, false),
            AccountMeta::writable(reserve_b, false),
            AccountMeta::writable(lp_mint, false),
            AccountMeta::writable(payer, true),
        ],
        &SwapInstruction::Initialize {
            fee_numerator: fee_num,
            fee_denominator: fee_den,
        },
    )
}

pub fn swap(
    pool: Pubkey,
    source: Pubkey,
    destination: Pubkey,
    reserve_in: Pubkey,
    reserve_out: Pubkey,
    user: Pubkey,
    amount_in: u64,
    min_out: u64,
) -> Instruction {
    Instruction::with_args(
        himsha_runtime::program_ids::swap_program(),
        vec![
            AccountMeta::readonly(pool, false),
            AccountMeta::writable(source, false),
            AccountMeta::writable(destination, false),
            AccountMeta::writable(reserve_in, false),
            AccountMeta::writable(reserve_out, false),
            AccountMeta::readonly(user, true),
        ],
        &SwapInstruction::Swap {
            amount_in,
            min_amount_out: min_out,
        },
    )
}

// ---- processing (runs inside zkVM) ----

fn token_pid() -> Pubkey {
    himsha_runtime::program_ids::token_program()
}

pub fn process(accounts: &mut [AccountInfo], data: &[u8]) -> Result<(), ProgramError> {
    let ix = SwapInstruction::try_from_slice(data).map_err(|_| ProgramError::InvalidInstruction)?;

    match ix {
        SwapInstruction::Initialize {
            fee_numerator,
            fee_denominator,
        } => {
            if fee_denominator == 0 {
                return Err(ProgramError::InvalidInstruction);
            }
            if accounts.len() < 7 {
                return Err(ProgramError::NotEnoughAccounts);
            }

            let mut pool: PoolState = accounts[0].read_data().unwrap_or_default();
            if pool.is_initialized {
                return Err(ProgramError::AlreadyInitialized);
            }

            pool.token_a_mint = accounts[1].key;
            pool.token_b_mint = accounts[2].key;
            pool.token_a_reserve = accounts[3].key;
            pool.token_b_reserve = accounts[4].key;
            pool.lp_mint = accounts[5].key;
            pool.fee_numerator = fee_numerator;
            pool.fee_denominator = fee_denominator;
            pool.is_initialized = true;
            accounts[0].write_data(&pool)?;
        }

        SwapInstruction::Swap {
            amount_in,
            min_amount_out,
        } => {
            // accounts[0]=pool, [1]=user source, [2]=user dest,
            // [3]=reserve_in (pool side), [4]=reserve_out (pool side), [5]=user.
            if accounts.len() < 6 {
                return Err(ProgramError::NotEnoughAccounts);
            }
            accounts[5].require_signer()?; // user must authorize the swap

            let mut pool: PoolState = accounts[0].read_data()?;
            if !pool.is_initialized {
                return Err(ProgramError::NotInitialized);
            }

            // Reserves are read *live* from the pool's reserve token accounts —
            // they are the source of truth, so the swap works in either direction
            // depending on which reserve account is passed as in/out.
            let reserve_in = accounts[3].read_data::<TokenAccountState>()?.amount;
            let reserve_out = accounts[4].read_data::<TokenAccountState>()?.amount;
            if reserve_in == 0 || reserve_out == 0 {
                return Err(ProgramError::PoolEmpty);
            }

            // constant-product formula: (reserve_in + amount_in_after_fee) * (reserve_out - out) = k
            let fee_amount = amount_in
                .checked_mul(pool.fee_numerator)
                .ok_or(ProgramError::Overflow)?
                .checked_div(pool.fee_denominator)
                .ok_or(ProgramError::Overflow)?;
            let amount_in_after_fee = amount_in
                .checked_sub(fee_amount)
                .ok_or(ProgramError::Overflow)?;

            let amount_out = (reserve_out as u128)
                .checked_mul(amount_in_after_fee as u128)
                .ok_or(ProgramError::Overflow)?
                .checked_div(
                    (reserve_in as u128)
                        .checked_add(amount_in_after_fee as u128)
                        .ok_or(ProgramError::Overflow)?,
                )
                .ok_or(ProgramError::Overflow)? as u64;

            if amount_out < min_amount_out {
                return Err(ProgramError::SlippageExceeded);
            }

            // CPI → token program: pull `amount_in` from the user into the pool's
            // reserve_in account (the full input, including the fee, stays in the pool).
            let transfer_in = borsh::to_vec(&TokenInstruction::Transfer { amount: amount_in })
                .map_err(|_| ProgramError::BorshError)?;
            cpi::invoke_indexed(
                accounts,
                &[1, 3, 5],
                &transfer_in,
                &token_pid(),
                token_process,
            )?;

            // CPI → token program: send `amount_out` from reserve_out to the user.
            // The pool (accounts[0]) is the reserve authority — it didn't sign the
            // tx, so we sign for it via invoke_signed (window index 2 = the owner).
            let transfer_out = borsh::to_vec(&TokenInstruction::Transfer { amount: amount_out })
                .map_err(|_| ProgramError::BorshError)?;
            cpi::invoke_signed_indexed(
                accounts,
                &[4, 2, 0],
                &[2],
                &transfer_out,
                &token_pid(),
                token_process,
            )?;

            // Sync the cached reserves from the post-transfer token balances so
            // `reserve_a`/`reserve_b` stay consistent with on-chain truth.
            sync_reserves(accounts, &mut pool)?;
            accounts[0].write_data(&pool)?;
        }

        SwapInstruction::Deposit {
            max_a,
            max_b,
            min_lp,
        } => {
            // accounts: [0]=pool, [1]=user_a, [2]=user_b, [3]=reserve_a,
            // [4]=reserve_b, [5]=user_lp, [6]=user, [7]=lp_mint.
            if accounts.len() < 8 {
                return Err(ProgramError::NotEnoughAccounts);
            }
            accounts[6].require_signer()?; // user must authorize the deposit
            let mut pool: PoolState = accounts[0].read_data()?;

            // `user_lp`     — LP tokens credited to the depositor (checked vs slippage).
            // `supply_delta`— total LP minted into existence (≥ user_lp on bootstrap,
            //                 the difference being the permanently-locked minimum).
            let (deposit_a, deposit_b, user_lp, supply_delta) = if pool.lp_supply == 0 {
                // Bootstrap: mint sqrt(a*b) LP, but lock MINIMUM_LIQUIDITY forever.
                let lp = integer_sqrt(max_a.checked_mul(max_b).ok_or(ProgramError::Overflow)?);
                let user_lp = lp
                    .checked_sub(MINIMUM_LIQUIDITY)
                    .ok_or(ProgramError::InsufficientFunds)?; // initial liquidity too small
                                                              // Only `user_lp` is minted as real tokens; `lp_supply` tracks the full
                                                              // `lp` so the virtual MINIMUM_LIQUIDITY can never be burned/drained
                                                              // (invariant: pool.lp_supply == lp_mint.supply + MINIMUM_LIQUIDITY).
                (max_a, max_b, user_lp, lp)
            } else {
                // Proportional deposit — uses live reserve balances as the truth.
                let reserve_a = reserve_balance(&accounts[3])?;
                let reserve_b = reserve_balance(&accounts[4])?;
                let lp_a = max_a
                    .checked_mul(pool.lp_supply)
                    .ok_or(ProgramError::Overflow)?
                    .checked_div(reserve_a.max(1))
                    .ok_or(ProgramError::Overflow)?;
                let lp_b = max_b
                    .checked_mul(pool.lp_supply)
                    .ok_or(ProgramError::Overflow)?
                    .checked_div(reserve_b.max(1))
                    .ok_or(ProgramError::Overflow)?;
                let lp_out = lp_a.min(lp_b);
                let actual_a = lp_out
                    .checked_mul(reserve_a)
                    .ok_or(ProgramError::Overflow)?
                    .checked_div(pool.lp_supply)
                    .ok_or(ProgramError::Overflow)?;
                let actual_b = lp_out
                    .checked_mul(reserve_b)
                    .ok_or(ProgramError::Overflow)?
                    .checked_div(pool.lp_supply)
                    .ok_or(ProgramError::Overflow)?;
                (actual_a, actual_b, lp_out, lp_out)
            };

            if user_lp < min_lp {
                return Err(ProgramError::SlippageExceeded);
            }

            // CPI → token program: pull both sides from the user into the reserves.
            let tx_a = borsh::to_vec(&TokenInstruction::Transfer { amount: deposit_a })
                .map_err(|_| ProgramError::BorshError)?;
            cpi::invoke_indexed(accounts, &[1, 3, 6], &tx_a, &token_pid(), token_process)?;
            let tx_b = borsh::to_vec(&TokenInstruction::Transfer { amount: deposit_b })
                .map_err(|_| ProgramError::BorshError)?;
            cpi::invoke_indexed(accounts, &[2, 4, 6], &tx_b, &token_pid(), token_process)?;

            // CPI → token program: mint LP tokens to the depositor.
            // MintTo expects [mint, destination, authority] → [lp_mint, user_lp, user].
            let mint_ix = borsh::to_vec(&TokenInstruction::MintTo { amount: user_lp })
                .map_err(|_| ProgramError::BorshError)?;
            cpi::invoke_indexed(accounts, &[7, 5, 6], &mint_ix, &token_pid(), token_process)?;

            pool.lp_supply = pool
                .lp_supply
                .checked_add(supply_delta)
                .ok_or(ProgramError::Overflow)?;
            sync_reserves_ab(accounts, &mut pool)?;
            accounts[0].write_data(&pool)?;
        }

        SwapInstruction::Withdraw {
            lp_amount,
            min_a,
            min_b,
        } => {
            // accounts: [0]=pool, [1]=user_a, [2]=user_b, [3]=reserve_a,
            // [4]=reserve_b, [5]=user_lp, [6]=user, [7]=lp_mint.
            if accounts.len() < 8 {
                return Err(ProgramError::NotEnoughAccounts);
            }
            accounts[6].require_signer()?; // user must authorize the withdrawal
            let mut pool: PoolState = accounts[0].read_data()?;
            if pool.lp_supply == 0 {
                return Err(ProgramError::PoolEmpty);
            }

            // Payout is proportional to the live reserves. `lp_supply` includes the
            // locked MINIMUM_LIQUIDITY, so a full burn can never drain the pool 1:1.
            let reserve_a = reserve_balance(&accounts[3])?;
            let reserve_b = reserve_balance(&accounts[4])?;
            let out_a = (reserve_a as u128)
                .checked_mul(lp_amount as u128)
                .ok_or(ProgramError::Overflow)?
                .checked_div(pool.lp_supply as u128)
                .ok_or(ProgramError::Overflow)? as u64;
            let out_b = (reserve_b as u128)
                .checked_mul(lp_amount as u128)
                .ok_or(ProgramError::Overflow)?
                .checked_div(pool.lp_supply as u128)
                .ok_or(ProgramError::Overflow)? as u64;

            if out_a < min_a || out_b < min_b {
                return Err(ProgramError::SlippageExceeded);
            }

            // CPI → token program: burn the user's LP first.
            // Burn expects [token_account, mint, owner] → [user_lp, lp_mint, user].
            let burn_ix = borsh::to_vec(&TokenInstruction::Burn { amount: lp_amount })
                .map_err(|_| ProgramError::BorshError)?;
            cpi::invoke_indexed(accounts, &[5, 7, 6], &burn_ix, &token_pid(), token_process)?;

            // CPI → token program: pay both sides out of the reserves to the user.
            let tx_a = borsh::to_vec(&TokenInstruction::Transfer { amount: out_a })
                .map_err(|_| ProgramError::BorshError)?;
            cpi::invoke_indexed(accounts, &[3, 1, 6], &tx_a, &token_pid(), token_process)?;
            let tx_b = borsh::to_vec(&TokenInstruction::Transfer { amount: out_b })
                .map_err(|_| ProgramError::BorshError)?;
            cpi::invoke_indexed(accounts, &[4, 2, 6], &tx_b, &token_pid(), token_process)?;

            pool.lp_supply = pool
                .lp_supply
                .checked_sub(lp_amount)
                .ok_or(ProgramError::InsufficientFunds)?;
            sync_reserves_ab(accounts, &mut pool)?;
            accounts[0].write_data(&pool)?;
        }
    }

    Ok(())
}

/// Refresh `pool.reserve_a` / `pool.reserve_b` from the live balances of the two
/// reserve token accounts (`accounts[3]` = reserve_in, `accounts[4]` = reserve_out),
/// mapping each side back to A/B via the pool's recorded reserve account keys.
fn sync_reserves(accounts: &[AccountInfo], pool: &mut PoolState) -> Result<(), ProgramError> {
    let in_bal = accounts[3].read_data::<TokenAccountState>()?.amount;
    let out_bal = accounts[4].read_data::<TokenAccountState>()?.amount;
    if accounts[3].key == pool.token_a_reserve {
        pool.reserve_a = in_bal;
        pool.reserve_b = out_bal;
    } else {
        pool.reserve_a = out_bal;
        pool.reserve_b = in_bal;
    }
    Ok(())
}

/// Read a reserve token account's current balance.
fn reserve_balance(acc: &AccountInfo) -> Result<u64, ProgramError> {
    Ok(acc.read_data::<TokenAccountState>()?.amount)
}

/// Sync cached reserves for the Deposit/Withdraw layout, where `accounts[3]` is
/// always reserve A and `accounts[4]` is always reserve B (no in/out swizzle).
fn sync_reserves_ab(accounts: &[AccountInfo], pool: &mut PoolState) -> Result<(), ProgramError> {
    pool.reserve_a = reserve_balance(&accounts[3])?;
    pool.reserve_b = reserve_balance(&accounts[4])?;
    Ok(())
}

fn integer_sqrt(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = x.div_ceil(2);
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use himsha_runtime::{account::AccountInfo, pubkey::Pubkey};

    fn swap_prog() -> Pubkey {
        himsha_runtime::program_ids::swap_program()
    }

    fn pool_account() -> AccountInfo {
        AccountInfo::new(Pubkey::from_seed(b"pool"), swap_prog(), 0, 512)
    }

    fn initialized_pool(reserve_a: u64, reserve_b: u64, lp_supply: u64) -> AccountInfo {
        let mut acc = pool_account();
        let state = PoolState {
            token_a_mint: Pubkey::from_seed(b"mint-a"),
            token_b_mint: Pubkey::from_seed(b"mint-b"),
            token_a_reserve: Pubkey::from_seed(b"res-a"),
            token_b_reserve: Pubkey::from_seed(b"res-b"),
            lp_mint: Pubkey::from_seed(b"lp"),
            fee_numerator: 3,
            fee_denominator: 1000,
            reserve_a,
            reserve_b,
            lp_supply,
            is_initialized: true,
        };
        acc.write_data(&state).unwrap();
        acc
    }

    fn dummy(key: &str) -> AccountInfo {
        AccountInfo::new(Pubkey::from_seed(key.as_bytes()), swap_prog(), 0, 0)
    }

    use himsha_runtime::account::AccountState;

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

    /// Build the 6-account window for a `Swap` (A → B):
    /// [0]=pool, [1]=user A source, [2]=user B dest, [3]=reserve A, [4]=reserve B, [5]=user.
    /// Reserve token-account keys match `initialized_pool` ("res-a"/"res-b").
    fn swap_accounts(reserve_a: u64, reserve_b: u64, user_a: u64) -> Vec<AccountInfo> {
        let mint_a = Pubkey::from_seed(b"mint-a");
        let mint_b = Pubkey::from_seed(b"mint-b");
        let user = Pubkey::from_seed(b"user");
        let pool_k = Pubkey::from_seed(b"pool");
        vec![
            initialized_pool(reserve_a, reserve_b, 1000),
            token_acct("user-a", mint_a, user, user_a),
            token_acct("user-b", mint_b, user, 0),
            token_acct("res-a", mint_a, pool_k, reserve_a),
            token_acct("res-b", mint_b, pool_k, reserve_b),
            AccountInfo::new(user, swap_prog(), 0, 0).as_signer(),
        ]
    }

    fn bal(acc: &AccountInfo) -> u64 {
        acc.read_data::<TokenAccountState>().unwrap().amount
    }

    fn mint_state_acct(key: &str, supply: u64) -> AccountInfo {
        let mut a = AccountInfo::new(
            Pubkey::from_seed(key.as_bytes()),
            himsha_runtime::program_ids::token_program(),
            0,
            256,
        );
        a.write_data(&himsha_token_program::MintState {
            mint_authority: Some(Pubkey::from_seed(b"pool")),
            supply,
            decimals: 0,
            is_initialized: true,
            freeze_authority: None,
        })
        .unwrap();
        a
    }

    /// 8-account window for Deposit/Withdraw:
    /// [0]=pool, [1]=user_a, [2]=user_b, [3]=reserve_a, [4]=reserve_b,
    /// [5]=user_lp, [6]=user, [7]=lp_mint. The lp_mint's real supply is
    /// `lp_supply - MINIMUM_LIQUIDITY` (the locked minimum is never minted).
    fn lp_accounts(
        reserve_a: u64,
        reserve_b: u64,
        lp_supply: u64,
        user_a: u64,
        user_b: u64,
        user_lp_bal: u64,
    ) -> Vec<AccountInfo> {
        let mint_a = Pubkey::from_seed(b"mint-a");
        let mint_b = Pubkey::from_seed(b"mint-b");
        let lp_key = Pubkey::from_seed(b"lp");
        let user = Pubkey::from_seed(b"user");
        let pool_k = Pubkey::from_seed(b"pool");
        let lp_token_supply = lp_supply.saturating_sub(MINIMUM_LIQUIDITY).max(user_lp_bal);
        vec![
            initialized_pool(reserve_a, reserve_b, lp_supply),
            token_acct("user-a", mint_a, user, user_a),
            token_acct("user-b", mint_b, user, user_b),
            token_acct("res-a", mint_a, pool_k, reserve_a),
            token_acct("res-b", mint_b, pool_k, reserve_b),
            token_acct("user-lp", lp_key, user, user_lp_bal),
            AccountInfo::new(user, swap_prog(), 0, 0).as_signer(),
            mint_state_acct("lp", lp_token_supply),
        ]
    }

    // ---- Initialize ----

    #[test]
    fn test_initialize_pool() {
        let mut accounts = vec![
            pool_account(),
            dummy("mint-a"),
            dummy("mint-b"),
            dummy("res-a"),
            dummy("res-b"),
            dummy("lp"),
            dummy("payer"),
        ];
        let ix = borsh::to_vec(&SwapInstruction::Initialize {
            fee_numerator: 3,
            fee_denominator: 1000,
        })
        .unwrap();
        process(&mut accounts, &ix).unwrap();
        let state: PoolState = accounts[0].read_data().unwrap();
        assert!(state.is_initialized);
        assert_eq!(state.fee_numerator, 3);
        assert_eq!(state.fee_denominator, 1000);
    }

    #[test]
    fn test_initialize_zero_fee_denominator() {
        let mut accounts = vec![
            pool_account(),
            dummy("a"),
            dummy("b"),
            dummy("ra"),
            dummy("rb"),
            dummy("lp"),
            dummy("p"),
        ];
        let ix = borsh::to_vec(&SwapInstruction::Initialize {
            fee_numerator: 0,
            fee_denominator: 0, // invalid
        })
        .unwrap();
        assert_eq!(
            process(&mut accounts, &ix),
            Err(ProgramError::InvalidInstruction)
        );
    }

    #[test]
    fn test_initialize_already_initialized() {
        let pool = initialized_pool(0, 0, 0);
        let mut accounts = vec![
            pool,
            dummy("a"),
            dummy("b"),
            dummy("ra"),
            dummy("rb"),
            dummy("lp"),
            dummy("p"),
        ];
        let ix = borsh::to_vec(&SwapInstruction::Initialize {
            fee_numerator: 3,
            fee_denominator: 1000,
        })
        .unwrap();
        assert_eq!(
            process(&mut accounts, &ix),
            Err(ProgramError::AlreadyInitialized)
        );
    }

    // ---- Swap (constant product) ----

    #[test]
    fn test_swap_basic() {
        // Pool: 1000 tokenA, 1000 tokenB, 0.3% fee; user holds 1000 A.
        let mut accounts = swap_accounts(1000, 1000, 1000);
        // Swap 100 tokenA, expect ~90 tokenB (x*y=k).
        let ix = borsh::to_vec(&SwapInstruction::Swap {
            amount_in: 100,
            min_amount_out: 80,
        })
        .unwrap();
        process(&mut accounts, &ix).unwrap();

        // Real token movement happened via CPI.
        assert_eq!(bal(&accounts[1]), 900); // user spent 100 A
        assert_eq!(bal(&accounts[3]), 1100); // reserve A grew by 100
        assert_eq!(bal(&accounts[2]), 90); // user received 90 B
        assert_eq!(bal(&accounts[4]), 910); // reserve B shrank by 90

        // Cached reserves synced to the live token balances.
        let state: PoolState = accounts[0].read_data().unwrap();
        assert_eq!(state.reserve_a, 1100);
        assert_eq!(state.reserve_b, 910);
    }

    #[test]
    fn test_swap_slippage_exceeded() {
        let mut accounts = swap_accounts(1000, 1000, 1000);
        // Ask for too much output — slippage; nothing should move.
        let ix = borsh::to_vec(&SwapInstruction::Swap {
            amount_in: 10,
            min_amount_out: 999,
        })
        .unwrap();
        assert_eq!(
            process(&mut accounts, &ix),
            Err(ProgramError::SlippageExceeded)
        );
        assert_eq!(bal(&accounts[1]), 1000); // user untouched
        assert_eq!(bal(&accounts[3]), 1000); // reserve untouched
    }

    #[test]
    fn test_swap_empty_pool() {
        let mut accounts = swap_accounts(0, 0, 1000); // empty reserves
        let ix = borsh::to_vec(&SwapInstruction::Swap {
            amount_in: 100,
            min_amount_out: 1,
        })
        .unwrap();
        assert_eq!(process(&mut accounts, &ix), Err(ProgramError::PoolEmpty));
    }

    #[test]
    fn test_swap_constant_product_invariant() {
        // Verify k = x*y is non-decreasing after a fee-bearing swap.
        let (ra, rb) = (10_000u64, 10_000u64);
        let mut accounts = swap_accounts(ra, rb, 5_000);
        let ix = borsh::to_vec(&SwapInstruction::Swap {
            amount_in: 1_000,
            min_amount_out: 1,
        })
        .unwrap();
        process(&mut accounts, &ix).unwrap();
        let state: PoolState = accounts[0].read_data().unwrap();
        let new_k = state.reserve_a as u128 * state.reserve_b as u128;
        let old_k = ra as u128 * rb as u128;
        assert!(new_k >= old_k, "AMM invariant violated: k decreased");
    }

    // ---- Deposit ----

    #[test]
    fn test_deposit_bootstrap() {
        // First deposit into an empty pool: user funds 1e6 of each side.
        let mut accounts = lp_accounts(0, 0, 0, 1_000_000, 1_000_000, 0);
        let ix = borsh::to_vec(&SwapInstruction::Deposit {
            max_a: 1_000_000,
            max_b: 1_000_000,
            min_lp: 1,
        })
        .unwrap();
        process(&mut accounts, &ix).unwrap();

        // Real token movement: both sides pulled into the reserves via CPI.
        assert_eq!(bal(&accounts[1]), 0); // user A spent
        assert_eq!(bal(&accounts[2]), 0); // user B spent
        assert_eq!(bal(&accounts[3]), 1_000_000); // reserve A funded
        assert_eq!(bal(&accounts[4]), 1_000_000); // reserve B funded
                                                  // LP minted to the user = sqrt(a*b) - MINIMUM_LIQUIDITY.
        assert_eq!(bal(&accounts[5]), 999_000);

        let state: PoolState = accounts[0].read_data().unwrap();
        assert_eq!(state.reserve_a, 1_000_000);
        assert_eq!(state.reserve_b, 1_000_000);
        assert_eq!(state.lp_supply, 1_000_000); // includes the locked minimum
    }

    #[test]
    fn test_deposit_bootstrap_below_minimum_fails() {
        // sqrt(a*b) <= MINIMUM_LIQUIDITY → no LP left for the depositor.
        let mut accounts = lp_accounts(0, 0, 0, 30, 30, 0);
        let ix = borsh::to_vec(&SwapInstruction::Deposit {
            max_a: 30,
            max_b: 30,
            min_lp: 1, // sqrt(900) = 30 < 1000
        })
        .unwrap();
        assert_eq!(
            process(&mut accounts, &ix),
            Err(ProgramError::InsufficientFunds)
        );
    }

    #[test]
    fn test_swap_fee_accrues_to_pool() {
        // With a non-zero fee, the *full* input (fee included) stays in the pool,
        // so reserve_in grows by amount_in while the user is charged the fee on
        // output — i.e. the fee accrues to LPs as extra reserves (k grows).
        let mut accounts = swap_accounts(1_000_000, 1_000_000, 100_000);
        let ix = borsh::to_vec(&SwapInstruction::Swap {
            amount_in: 10_000,
            min_amount_out: 1,
        })
        .unwrap();
        process(&mut accounts, &ix).unwrap();

        // fee = 10_000 * 3 / 1000 = 30; out computed on 9_970 after-fee.
        let out = 1_000_000u128 * 9_970 / (1_000_000 + 9_970);
        assert_eq!(bal(&accounts[3]), 1_010_000); // reserve_in += FULL input
        assert_eq!(bal(&accounts[4]), 1_000_000 - out as u64); // reserve_out -= output
                                                               // The retained 30-unit fee means k strictly increased.
        let state: PoolState = accounts[0].read_data().unwrap();
        assert!(
            state.reserve_a as u128 * state.reserve_b as u128 > 1_000_000u128 * 1_000_000,
            "fee did not accrue to the pool",
        );
    }

    // ---- Withdraw ----

    #[test]
    fn test_withdraw_basic() {
        // Pool 2000 A / 3000 B, lp_supply 1000; user holds 100 LP (10%).
        let mut accounts = lp_accounts(2000, 3000, 1000, 0, 0, 100);
        let ix = borsh::to_vec(&SwapInstruction::Withdraw {
            lp_amount: 100,
            min_a: 1,
            min_b: 1,
        })
        .unwrap();
        process(&mut accounts, &ix).unwrap();

        // out_a = 2000*100/1000 = 200, out_b = 3000*100/1000 = 300.
        assert_eq!(bal(&accounts[1]), 200); // user received A
        assert_eq!(bal(&accounts[2]), 300); // user received B
        assert_eq!(bal(&accounts[3]), 1800); // reserve A paid out
        assert_eq!(bal(&accounts[4]), 2700); // reserve B paid out
        assert_eq!(bal(&accounts[5]), 0); // user's LP burned

        let state: PoolState = accounts[0].read_data().unwrap();
        assert_eq!(state.reserve_a, 1800);
        assert_eq!(state.reserve_b, 2700);
        assert_eq!(state.lp_supply, 900);
    }

    #[test]
    fn test_withdraw_empty_pool_fails() {
        let mut accounts = lp_accounts(0, 0, 0, 0, 0, 0);
        let ix = borsh::to_vec(&SwapInstruction::Withdraw {
            lp_amount: 100,
            min_a: 1,
            min_b: 1,
        })
        .unwrap();
        assert_eq!(process(&mut accounts, &ix), Err(ProgramError::PoolEmpty));
    }

    #[test]
    fn test_withdraw_slippage() {
        // Withdraw 100 LP → ~100 of each, but demand 500 → slippage (before any CPI).
        let mut accounts = lp_accounts(1000, 1000, 1000, 0, 0, 100);
        let ix = borsh::to_vec(&SwapInstruction::Withdraw {
            lp_amount: 100,
            min_a: 500,
            min_b: 500,
        })
        .unwrap();
        assert_eq!(
            process(&mut accounts, &ix),
            Err(ProgramError::SlippageExceeded)
        );
    }
}

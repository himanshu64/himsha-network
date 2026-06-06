# Yield Vaults — Automated Yield Strategies

An automated investment layer that allocates deposits into predefined,
yield-generating strategies so users earn passive income without hand-managing DeFi.

## Key features
- Automated capital allocation
- Lending-yield optimization
- Structured-product strategies
- Auto-rebalancing
- Risk-based vault selection
- Passive income generation

## Business value
Users earn yield without manually operating complex lending/structured-finance
strategies; the platform abstracts strategy selection and rebalancing.

## Status — built
- **Program:** [`himsha-programs/vault`](../../himsha-programs/vault) — an ERC-4626-style
  share vault. `InitVault` / `Deposit` (mint shares) / `Withdraw` (burn shares, redeem
  assets) with a `MINIMUM_LIQUIDITY` lock, and **`Report`** — the keeper hook that syncs
  NAV to the vault's real balance and mints **performance-fee** shares to the manager on
  profit. All token moves are real CPI; the vault signs for its own authority. (11 tests.)
- **Keeper:** [`himsha-sdk/examples/yield-keeper.ts`](../../himsha-sdk/examples/yield-keeper.ts) —
  a polling service that calls `Report` each tick (and is where rebalancing logic lives).
- **SDK:** `VaultProgram` (init/deposit/withdraw/report) in [`@himsha-network/sdk`](../../himsha-sdk).

## How HIMSHA powers this
Yield Vaults composes existing programs; the share accounting lives in the vault program:

- **Lending yield:** deposit into [`himsha-programs/money-market`](../../himsha-programs/money-market)
  as the borrow-side liquidity (`AddLiquidity`); interest accrues to suppliers via the
  borrow index.
- **Swap/LP yield:** provide liquidity to [`himsha-programs/swap`](../../himsha-programs/swap)
  (`Deposit`) and earn the 0.3% trading fee.
- **A `vault` program (to build):** mints vault-share tokens, tracks each depositor's
  share, and a **keeper/agent** moves capital between strategies (auto-rebalance,
  auto-compound) by calling the underlying programs.

## Strategy tiers (illustrative)
| Strategy | Source | Risk |
|---|---|---|
| Lending | money-market supply APY | Low |
| LP fees | swap pool fees | Low–Medium |
| Structured / covered-call | external venues (off-chain integration) | Medium–High |

## To productionize
- Write the `himsha-programs/vault` program (shares, deposit/withdraw, NAV).
- Build the rebalancing keeper (the [AI Yield agent](./ai-copilot.md) can recommend
  migrations; a keeper executes them).
- Risk tiering + disclosures; securities analysis (yield products draw regulatory
  scrutiny — see the root README compliance notes).

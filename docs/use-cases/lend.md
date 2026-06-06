# Lend — Bitcoin-Backed Credit with ~300ms Liquidation Fidelity

A lending protocol where users post Bitcoin (or Bitcoin assets / inscriptions) as
collateral and borrow liquidity, protected by a high-frequency liquidation engine.

## Key features
- BTC-backed loans
- Real-time collateral monitoring
- High-frequency liquidation engine (~300ms response target)
- Risk-management automation
- Margin / leverage support

## Business value
Bitcoin holders unlock liquidity **without selling their BTC**; lenders are protected
by rapid liquidation when collateral value falls.

## How HIMSHA powers this
Two complementary programs:

- **Token-collateral money market — [`himsha-programs/money-market`](../../himsha-programs/money-market):**
  `Supply` collateral → `Borrow` against it → `Repay`; enforced **collateral factor /
  health factor**, **utilization-based interest accrual** (a Compound-style borrow
  index), and **`Liquidate`** that seizes collateral plus a liquidation bonus once a
  position crosses the liquidation threshold. `AddLiquidity` funds the borrow side.
- **Ordinals-collateral lending — [`himsha-programs/lending`](../../himsha-programs/lending):**
  bid/accept/repay/default over inscriptions, with interest, partial repayment, bid
  cancellation, and a **settlement queue** the node drains to move the actual Bitcoin
  UTXOs (`send_payment` / `transfer_utxo`).

## The ~300ms liquidation engine
The on-chain primitive exists (`is_liquidatable`, `Liquidate`). The "300ms fidelity"
is delivered by an **off-chain keeper** you run alongside the node:

1. The [Bitcoin indexer auto-sync](../../himsha-node/src/bitcoin_indexer.rs) streams
   price/mempool/height events.
2. A keeper recomputes each position's health on every tick (using
   `money_market::is_healthy` / `is_liquidatable`).
3. When a position is unhealthy, the keeper submits a `Liquidate` instruction.

Tightening the loop to ~300ms is a matter of poll interval + a low-latency price feed.
(Today's default sync poll is configurable via `BITCOIN_SYNC_INTERVAL_SECS`.)

## How a client uses it
```ts
import { MoneyMarketProgram } from '@himsha-network/sdk';

MoneyMarketProgram.supply(market, position, userCollateral, collateralVault, user, 1_000_000n);
MoneyMarketProgram.borrow(market, position, userBorrow, borrowVault, user, 700_000n);
// keeper: MoneyMarketProgram.liquidate(market, position, ...);
```

## To productionize
- Build the liquidation keeper service + a real price oracle (the market stores a
  `price`; today it's set/updated manually).
- Margin/leverage UX and risk limits.
- Consumer-credit disclosures + AML (see compliance notes in the root README).

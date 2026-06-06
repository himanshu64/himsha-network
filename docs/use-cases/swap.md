# Swap — Native BTC Trading with Atomic Settlement

A decentralized Bitcoin trading engine: users swap Bitcoin-native assets directly,
with no wrapped tokens, no intermediary, and no counterparty risk.

## Key features
- Native Bitcoin / Bitcoin-asset trading
- **Atomic settlement** — the whole trade succeeds or nothing happens
- No counterparty risk
- Near-instant finality
- Decentralized execution

## Business value
Secure, trustless Bitcoin trading — like a DEX, but optimized for Bitcoin-native
assets (tokens, Runes).

## How HIMSHA powers this
- **Program:** [`himsha-programs/swap`](../../himsha-programs/swap) — a constant-product
  AMM (`x·y=k`). The `Swap` instruction reads live reserves and performs **real token
  movement via CPI** into the token program (pull `amount_in`, send `amount_out`),
  with slippage protection (`min_amount_out`) and 0.3% fee accrual to LPs.
- **Atomicity:** a single `Swap` instruction either completes both CPI transfers or
  returns `ProgramError` and persists nothing — that's the atomic unit today.
  *Cross-instruction* all-or-nothing settlement (full multi-leg rollback) is a node
  enhancement still to build.
- **Liquidity:** `Deposit`/`Withdraw` move both sides via CPI and mint/burn LP tokens,
  with a `MINIMUM_LIQUIDITY` lock guarding the first-depositor attack.

## How a client uses it
TypeScript SDK ([`@himsha-network/sdk`](../../himsha-sdk)):

```ts
import { SwapProgram } from '@himsha-network/sdk';

const ix = SwapProgram.swap(
  pool, userSource, userDest, reserveIn, reserveOut, user,
  100_000n,   // amountIn
  90_000n,    // minAmountOut (slippage floor)
);
// add ix to a transaction, sign, and send via himsha_sendTransaction
```

## To productionize
- Real Bitcoin-asset settlement on L1 (the node already has `transfer_utxo` /
  `send_payment` Bitcoin functions; wire swap outputs to UTXO moves).
- Multi-instruction atomic rollback in the node's `send_transaction`.
- Price-impact / routing UI, and AML/sanctions screening on counterparties.

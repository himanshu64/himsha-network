# Prime — Real-Time Portfolio Management Dashboard

A professional dashboard for tracking digital-asset portfolios across wallets and
HIMSHA positions (swap LP, loans, vault deposits).

## Key features
- Live portfolio tracking
- P&L monitoring
- Asset-allocation visualization
- Risk analytics
- Trade history & reporting
- Multi-wallet support

## Business value
Traders, funds, and institutions get a single unified view of holdings and performance
across all four products.

## How HIMSHA powers this
Prime is a **read-only application layer** over the node's JSON-RPC — no new on-chain
program. It aggregates state via:

- `himsha_getAccountInfo` — a specific account (token balance, LP, position).
- `himsha_getProgramAccounts` — every account owned by a program (e.g. all of a user's
  positions in the money market, all token accounts) via the node's owner-scan.
- `himsha_getBlock` / `himsha_getSlot` — timeline / history.
- `himsha_getUtxo` + the Bitcoin indexer — on-chain BTC holdings and confirmations.
- The [SDKs](../modules/sdks.md) decode account data (`PoolState`, `Position`,
  `MarketState`, token/rune balances) into portfolio rows.

**Multi-wallet** = the dashboard queries several pubkeys and merges results.
**Reporting/RBAC/compliance** live in the Prime app (not the chain).

## Sketch
```ts
import { HimshaConnection, PROGRAM_IDS } from '@himsha-network/sdk';
const conn = new HimshaConnection('http://localhost:9100');

// All of a user's money-market positions / token accounts to value the book:
const positions = await conn.getProgramAccounts(PROGRAM_IDS.moneyMarket);
const balances  = await conn.getProgramAccounts(PROGRAM_IDS.token);
// decode + price + aggregate into allocation / P&L / risk views
```

## To productionize
- A price/marketdata service for valuation and P&L.
- An indexing/caching layer (the raw `getProgramAccounts` scan won't scale to large
  state — add a dedicated indexer DB).
- Auth + RBAC, audit logging, exportable reports (ties into the [AI tax assistant](./ai-copilot.md)).

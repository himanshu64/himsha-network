# Lightning Network Integration

HIMSHA is **not** a Lightning node. It *integrates* an existing one (LND here;
CLN or LDK would mirror the same shape) over its REST API and uses Lightning as a
**fast, low-fee off-chain settlement rail** for sat-denominated payouts — loan
repayments, yield distributions, fee rebates — instead of forcing every payment
onto an on-chain UTXO move.

The chain state, ZK execution, and account model stay exactly as they are. Lightning
is only a payout *transport*: when a settlement's recipient is a BOLT-11 invoice,
the node pays it over Lightning; otherwise it falls back to the on-chain Bitcoin path.

```
program logic (ZK)  ──▶  settlement queue  ──┬─▶  BOLT-11 invoice?  ──▶  LND pay_invoice   (off-chain, instant)
                                             └─▶  Bitcoin address?  ──▶  on-chain UTXO move (BitcoinIndexer)
```

## Why

| | On-chain settlement | Lightning settlement |
|---|---|---|
| Latency | ~10 min / confirmations | sub-second |
| Fee | sat/vB, fixed floor | ~ppm, negligible for small payouts |
| Min economical amount | dust limit (~330 sat) | a few sats |
| Best for | large / final settlement | streaming, micro-payouts, repayments |

DeFi payouts (interest, yield, small loan repayments) are exactly the workload
Lightning is good at — frequent and small. Large/final settlements still go
on-chain where finality matters more than speed.

## Configuration

Two environment variables enable it — leave them unset to run with Lightning
disabled (every Lightning RPC then returns a structured `-32040` error and
on-chain settlement is used unconditionally):

```bash
export LND_REST_URL="https://127.0.0.1:8080"        # your LND REST endpoint
export LND_MACAROON_HEX="$(xxd -ps -u -c 1000 ~/.lnd/data/chain/bitcoin/regtest/admin.macaroon)"
```

TLS is accepted as-is (`danger_accept_invalid_certs`) so a local self-signed LND
works out of the box. For production, terminate TLS properly and use a scoped
(invoice/pay-only) macaroon rather than `admin.macaroon`.

## RPC methods

| Method | Params | Returns |
|--------|--------|---------|
| `himsha_createInvoice` | `amount_sat: u64, memo: String` | BOLT-11 payment request |
| `himsha_payInvoice` | `bolt11: String` | payment hash |
| `himsha_lightningBalance` | — | spendable channel balance (sats) |

When LND is not configured, each returns:

```json
{ "jsonrpc": "2.0",
  "error": { "code": -32040, "message": "lightning not configured (set LND_REST_URL, LND_MACAROON_HEX)" },
  "id": 1 }
```

### TypeScript SDK

```ts
const conn = new HimshaConnection('http://localhost:9100');

const invoice = await conn.createInvoice(1_000n, 'loan #42 repayment');
const hash    = await conn.payInvoice(invoice);
const balance = await conn.lightningBalance();   // bigint sats
```

## Lending settlement over Lightning

The node's loan-settlement path is Lightning-aware. A lending program enqueues a
repayment whose recipient is either a Bitcoin address (on-chain) or a BOLT-11
invoice (Lightning). At settlement time the node inspects the recipient:

- **starts with `lnbc` / `lntb` / `lnbcrt` / `lnsb`** → routed to `pay_invoice`
  over Lightning (instant, off-chain).
- **anything else** → on-chain UTXO move via the Bitcoin indexer.

This routing is the `is_invoice()` heuristic in
[`himsha-node/src/lightning.rs`](../himsha-node/src/lightning.rs); the dispatch
lives in `HimshaNode::settle_lending`.

## Status & limitations

- ✅ Compiles, request-shaping and routing covered by unit tests
  (`test_is_invoice`), graceful unconfigured behaviour verified against a live node.
- ⚠️ **Live invoice create/pay is unverified without a funded LND node + open
  channels.** The REST request bodies follow LND's `/v1/invoices`,
  `/v1/channels/transactions`, and `/v1/balance/channels` shapes; confirm against
  your LND version before relying on them.
- HIMSHA does not manage channels, liquidity, or routing — that's the operator's
  LND node. HIMSHA only creates/pays invoices and reads channel balance.
- No HTLC-on-chain-state coupling yet: a Lightning payout is fire-and-settle, not
  atomically tied to the ZK state transition. For high-value settlement prefer the
  on-chain path until atomic LN↔chain settlement is added.

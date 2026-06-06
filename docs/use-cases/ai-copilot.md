# AI Copilot — LLM + RAG over the Bitcoin Financial Stack

Most crypto products overwhelm users with data. An LLM + RAG layer turns HIMSHA's
on-chain and market data into **actionable, plain-language insight** — and can act on
the user's behalf by building transactions. This is the strongest differentiator and
the highest-ticket product (institutional copilots at $2k+/month).

> Application/integration layer — **planned**, not part of the Rust workspace. It
> reads from the node's JSON-RPC + the Bitcoin indexer, and writes by building
> transactions through the [SDKs](../modules/sdks.md).

## Architecture

```
Bitcoin & platform data sources
  Wallet · Transactions · UTXOs · Market · Lending · Liquidations ·
  Yield strategies · Research · Protocol docs · Regulatory docs · News/social
            ↓  data pipeline
  Vector database (Pinecone / Weaviate / Qdrant)
            ↓  RAG layer
  LLM agent layer (GPT / Claude / Gemini / Llama)
            ↓
  Products: Swap | Lend | Prime | Yield Vaults
```

Data in: node RPC (`himsha_getAccountInfo`, `himsha_getProgramAccounts`,
`himsha_getBlock`, `himsha_getUtxo`) + the [indexer auto-sync](../../himsha-node/src/bitcoin_indexer.rs)
event stream (Seen/Confirmed/Evicted, chain tip). Actions out: the agent composes
instructions with the SDKs and submits `himsha_sendTransaction`.

## Per-product AI features

### Swap — Trading Copilot
- **Advisory:** "Should I swap 2 BTC into USD now?" → reasons over price, volatility,
  the user's book (from Prime), macro/sentiment, and recommends a sizing.
- **Natural-language orders:** "Buy $10k BTC if price drops below $95,000" →
  structured limit order.
- **Strategy RAG:** retrieve historical breakout trades / playbooks and summarize patterns.

### Lend — Risk & Borrowing Advisor
- **Liquidation assistant:** "Will I be liquidated?" → uses `money_market::is_liquidatable`
  + price/volatility to give the exact liquidation price and headroom.
- **Smart borrowing:** "How much can I safely borrow?" → max vs. recommended-safe amount.
- **Credit intelligence:** RAG over borrowing/repayment history → risk profile.

### Prime — Portfolio Analyst Agent
- "Explain my portfolio like a hedge-fund manager" → allocation, concentration risk.
- **Portfolio chat:** "Why am I down this month?" → attributes P&L across positions.
- **Tax assistant:** RAG over tax rules + history → jurisdiction-specific reports
  (e.g. India capital-gains).

### Yield Vaults — Yield Agent
- **Recommendation engine:** "Where should I deploy 5 BTC?" → ranks strategies by
  APY/risk/history.
- **Plain-language vault explanations** and a **daily rebalancing agent** that flags
  "moving A→B adds ~3.2% APY".

## Bitcoin-specific RAG sources
- **On-chain:** blocks, transactions, UTXOs, mempool (from the node + indexer).
- **Market:** Binance / Coinbase / Kraken / Deribit feeds.
- **Research:** Glassnode, Bitcoin Magazine, institutional reports, ETF filings.
- **Regulatory:** SEC guidance, IRS, India crypto rules, global compliance.
- **Internal:** user portfolios, loan history, trade history, yield performance.

## Multi-agent system
A router over specialized agents rather than one chatbot:

| Agent | Answers |
|---|---|
| Market Analyst | price moves, trends, sentiment |
| Lending Risk | liquidation risk, borrow limits |
| Portfolio | allocation, performance |
| Yield | vault selection, optimization |
| Compliance | tax, regulations, reporting |

## Monetization tiers
| Tier | Price | Includes |
|---|---|---|
| Retail | $49–$99/mo | Portfolio AI, yield recs, tax assistant, trading copilot |
| Pro traders | $299–$999/mo | AI risk monitoring, market intelligence, liquidation forecasting, strategy generation |
| Institutions | $2k–$10k/mo | Portfolio copilots, research RAG, compliance assistant, custom reporting, multi-account intelligence |

## The flagship bet
**Prime + Lend AI Copilot for institutions:** an always-on agent that continuously
monitors portfolios, loans, liquidation risk, yield opportunities, and compliance, and
**proactively alerts with recommendations** instead of waiting to be asked. It solves a
real operational problem for funds and large BTC holders — the clearest path to
$2k+/month contracts.

## Build notes / guardrails
- Keep the LLM **advisory by default**; require explicit user signing for any action
  the agent proposes (the SDKs build the tx; the user/wallet signs).
- Ground every figure in retrieved data (cite the source account/tx) to avoid
  hallucinated numbers in a financial context.
- Run the compliance agent's outputs past real counsel — AI tax/regulatory output is
  assistance, not advice.

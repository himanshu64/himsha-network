# HIMSHA Network — Use Cases

A Bitcoin-native financial infrastructure platform built on the HIMSHA programs. Four
core products — **Swap, Lend, Prime, Yield Vaults** — plus an **AI copilot layer**
that turns raw Bitcoin/market data into actionable insight.

> Educational / proof-of-concept — see the [root README](../../README.md) disclaimer.
> Anything marketed below as a "product" is a use case *on top of* the HIMSHA
> programs; the build-status column says what actually exists in this repo today.

## The product suite

| Product | What it is | Powered by (HIMSHA) | Status |
|---|---|---|---|
| [**Swap**](./swap.md) | Native BTC asset swaps with atomic settlement | [`himsha-programs/swap`](../../himsha-programs/swap) (constant-product AMM, real CPI token transfers) | ✅ program built |
| [**Lend**](./lend.md) | BTC-backed credit with fast liquidation | [`himsha-programs/money-market`](../../himsha-programs/money-market) + [`himsha-programs/lending`](../../himsha-programs/lending) | ✅ programs built; liquidation *keeper* = to build |
| [**Prime**](./prime.md) | Real-time portfolio management dashboard | node RPC (`himsha_getProgramAccounts`, `himsha_getAccountInfo`) + [SDKs](../modules/sdks.md) | ⚠️ app layer, planned |
| [**Yield Vaults**](./yield-vaults.md) | Automated yield strategies | [`himsha-programs/vault`](../../himsha-programs/vault) + [keeper](../../himsha-sdk/examples/yield-keeper.ts) | ✅ vault + keeper built; strategy-CPI = to wire |
| [**AI Copilot**](./ai-copilot.md) | LLM + RAG multi-agent advisory across all four | node RPC + Bitcoin indexer feed → vector DB → LLM | ⬜ planned (app layer) |

## Platform requirements (target)

- Bitcoin-native architecture
- Institutional-grade security
- Real-time data processing
- Multi-wallet integration
- Role-based access control
- Compliance-ready reporting (see [compliance notes](../../README.md))
- Advanced analytics dashboard
- Mobile-responsive design
- API-first architecture (the node's JSON-RPC + the SDKs)
- Scalable cloud infrastructure (see [deployment guides](../deployment/README.md))

## Target users

Bitcoin holders · professional traders · crypto funds · family offices · institutions ·
high-net-worth investors.

## Goal

A complete Bitcoin financial ecosystem where users can **trade, borrow, manage
portfolios, and generate yield** from a single unified platform — with an AI layer
that proactively monitors and advises.

## How the pieces connect

```
                         ┌────────────────── AI Copilot (LLM + RAG) ───────────────────┐
                         │  Market · Lending-risk · Portfolio · Yield · Compliance agents │
                         └───────────────▲───────────────────────────▲──────────────────┘
                                         │ reads                       │ acts (builds tx)
   Users / wallets / API ── SDKs ──►  HIMSHA node (JSON-RPC :9100)  ◄── Bitcoin indexer (auto-sync)
                                         │
        ┌────────────────────────────────┼────────────────────────────────┐
        ▼                ▼                ▼                ▼                ▼
      Swap            Lend             Prime          Yield Vaults     (token/runes/nft/ata)
   (swap pgm)   (money-market +     (read-only       (composed
                  lending pgm)      analytics)        strategies)
```

See each product page for features, business value, and exactly which HIMSHA module /
RPC / SDK call powers it.

/**
 * HIMSHA Yield-Vault Keeper
 * =========================
 * A background service that keeps a vault's NAV current and (in a fuller build)
 * rebalances capital across strategies.
 *
 * Each tick it calls the vault's `Report` instruction, which:
 *   - reads the vault's real asset-vault balance,
 *   - mints performance-fee shares to the manager on any profit,
 *   - syncs `total_assets` so share price reflects accrued yield.
 *
 * Run (from himsha-sdk/):
 *   NODE_URL=http://127.0.0.1:9100 \
 *   VAULT=<pubkey> MANAGER=<pubkey> ASSET_VAULT=<pubkey> \
 *   SHARE_MINT=<pubkey> MANAGER_SHARES=<pubkey> \
 *   INTERVAL_MS=15000 \
 *   npx ts-node examples/yield-keeper.ts
 *
 * NOTE: signing is a placeholder here. This PoC node only checks the signature
 * *count*; a production keeper must produce a real BIP-340 Schnorr signature for
 * the manager over `tx.messageHash()` and attach it via `tx.addSignature(...)`.
 */

import {
  HimshaConnection,
  HimshaPublicKey,
  HimshaTransaction,
  VaultProgram,
} from '../src/index';

function env(name: string): string {
  const v = process.env[name];
  if (!v) throw new Error(`missing env ${name}`);
  return v;
}

const NODE_URL    = process.env.NODE_URL ?? 'http://127.0.0.1:9100';
const INTERVAL_MS = Number(process.env.INTERVAL_MS ?? '15000');

const vault         = HimshaPublicKey.fromBase58(env('VAULT'));
const manager       = HimshaPublicKey.fromBase58(env('MANAGER'));
const assetVault    = HimshaPublicKey.fromBase58(env('ASSET_VAULT'));
const shareMint     = HimshaPublicKey.fromBase58(env('SHARE_MINT'));
const managerShares = HimshaPublicKey.fromBase58(env('MANAGER_SHARES'));

const conn = new HimshaConnection(NODE_URL);

/** Placeholder signer — replace with real Schnorr signing over the message hash. */
function signAsManager(_messageHash: Uint8Array): Uint8Array {
  return new Uint8Array(64); // 64-byte placeholder; node checks count only (PoC)
}

async function reportOnce(): Promise<void> {
  const ix = VaultProgram.report(vault, manager, assetVault, shareMint, managerShares);
  const tx = HimshaTransaction.create([manager], [ix]);
  tx.addSignature(signAsManager(tx.messageHash()));

  const txId = await conn.sendTransaction(tx);
  console.log(`[keeper] reported NAV — tx ${txId}`);
}

// Where strategy rebalancing would live: read vault state + each strategy's APY,
// then move idle capital (e.g. money-market AddLiquidity / Withdraw) before reporting.
async function maybeRebalance(): Promise<void> {
  // const v = await conn.getAccountData<VaultStateView>(vault);
  // compare strategy APYs, submit Allocate/Deallocate, etc. (future work)
}

async function tick(): Promise<void> {
  try {
    await maybeRebalance();
    await reportOnce();
  } catch (e) {
    console.error('[keeper] tick failed:', (e as Error).message);
  }
}

async function main(): Promise<void> {
  if (!(await conn.isNodeReady())) throw new Error('node not ready');
  console.log(`[keeper] watching vault ${vault.toBase58()} every ${INTERVAL_MS}ms`);
  await tick();
  setInterval(tick, INTERVAL_MS);
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});

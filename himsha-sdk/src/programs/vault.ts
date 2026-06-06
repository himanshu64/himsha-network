import { HimshaInstruction } from '../transaction';
import { HimshaPublicKey, PROGRAM_IDS } from '../pubkey';

// Borsh enum variant indices — order must match VaultInstruction in Rust.
const IX = {
  InitVault: new Uint8Array([0]),
  Deposit:   new Uint8Array([1]),
  Withdraw:  new Uint8Array([2]),
  Report:    new Uint8Array([3]),
} as const;

function u64Le(n: bigint): Uint8Array {
  const buf = new Uint8Array(8);
  new DataView(buf.buffer).setBigUint64(0, n, true);
  return buf;
}
function concat(...arrays: Uint8Array[]): Uint8Array {
  const len = arrays.reduce((s, a) => s + a.length, 0);
  const out = new Uint8Array(len);
  let offset = 0;
  for (const a of arrays) { out.set(a, offset); offset += a.length; }
  return out;
}

/** Initialize a yield vault over `assetMint`, issuing `shareMint` shares. */
export function initVault(
  vault: HimshaPublicKey,
  assetMint: HimshaPublicKey,
  shareMint: HimshaPublicKey,
  assetVault: HimshaPublicKey,
  manager: HimshaPublicKey,
  performanceFeeBps: bigint, // e.g. 1000n = 10%
): HimshaInstruction {
  const data = concat(IX.InitVault, u64Le(performanceFeeBps));
  return new HimshaInstruction(
    PROGRAM_IDS.vault,
    [
      HimshaInstruction.writable(vault, false),
      HimshaInstruction.readonly(assetMint, false),
      HimshaInstruction.readonly(shareMint, false),
      HimshaInstruction.writable(assetVault, false),
      HimshaInstruction.readonly(manager, true),
    ],
    data,
  );
}

// Deposit / Withdraw share the 6-account layout:
// [vault(w), userAsset(w), assetVault(w), userShares(w), shareMint(w), user(signer)].
function userIx(
  tag: Uint8Array,
  vault: HimshaPublicKey, userAsset: HimshaPublicKey, assetVault: HimshaPublicKey,
  userShares: HimshaPublicKey, shareMint: HimshaPublicKey, user: HimshaPublicKey,
  a: bigint, b: bigint,
): HimshaInstruction {
  const data = concat(tag, u64Le(a), u64Le(b));
  return new HimshaInstruction(
    PROGRAM_IDS.vault,
    [
      HimshaInstruction.writable(vault, false),
      HimshaInstruction.writable(userAsset, false),
      HimshaInstruction.writable(assetVault, false),
      HimshaInstruction.writable(userShares, false),
      HimshaInstruction.writable(shareMint, false),
      HimshaInstruction.readonly(user, true),
    ],
    data,
  );
}

/** Deposit `amount` assets, minting at least `minShares` vault shares. */
export function deposit(
  vault: HimshaPublicKey, userAsset: HimshaPublicKey, assetVault: HimshaPublicKey,
  userShares: HimshaPublicKey, shareMint: HimshaPublicKey, user: HimshaPublicKey,
  amount: bigint, minShares: bigint,
): HimshaInstruction {
  return userIx(IX.Deposit, vault, userAsset, assetVault, userShares, shareMint, user, amount, minShares);
}

/** Redeem `shares` for at least `minAssets` assets. */
export function withdraw(
  vault: HimshaPublicKey, userAsset: HimshaPublicKey, assetVault: HimshaPublicKey,
  userShares: HimshaPublicKey, shareMint: HimshaPublicKey, user: HimshaPublicKey,
  shares: bigint, minAssets: bigint,
): HimshaInstruction {
  return userIx(IX.Withdraw, vault, userAsset, assetVault, userShares, shareMint, user, shares, minAssets);
}

/**
 * Sync NAV to the vault's actual balance and mint performance-fee shares to the
 * manager on profit. Called by the keeper after yield lands in the vault.
 * Accounts: [vault(w), manager(signer), assetVault, shareMint(w), managerShares(w)].
 */
export function report(
  vault: HimshaPublicKey, manager: HimshaPublicKey, assetVault: HimshaPublicKey,
  shareMint: HimshaPublicKey, managerShares: HimshaPublicKey,
): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.vault,
    [
      HimshaInstruction.writable(vault, false),
      HimshaInstruction.readonly(manager, true),
      HimshaInstruction.readonly(assetVault, false),
      HimshaInstruction.writable(shareMint, false),
      HimshaInstruction.writable(managerShares, false),
    ],
    IX.Report,
  );
}

export const VaultProgram = { initVault, deposit, withdraw, report };

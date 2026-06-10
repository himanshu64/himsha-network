import { HimshaInstruction } from '../transaction';
import { HimshaPublicKey, PROGRAM_IDS } from '../pubkey';

// Borsh enum variant indices — order must match MoneyMarketInstruction in Rust.
const IX = {
  InitMarket:   new Uint8Array([0]),
  AddLiquidity: new Uint8Array([1]),
  Supply:       new Uint8Array([2]),
  Withdraw:     new Uint8Array([3]),
  Borrow:       new Uint8Array([4]),
  Repay:        new Uint8Array([5]),
  Liquidate:    new Uint8Array([6]),
  SyncPrice:    new Uint8Array([7]),
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

/** Initialize a money market (collateral + borrowable asset pair). */
export function initMarket(
  market:                 HimshaPublicKey,
  collateralMint:         HimshaPublicKey,
  borrowMint:             HimshaPublicKey,
  collateralVault:        HimshaPublicKey,
  borrowVault:            HimshaPublicKey,
  admin:                  HimshaPublicKey,
  oracleFeed:             HimshaPublicKey, // PriceFeed account prices are synced from
  collateralFactorBps:    bigint, // e.g. 7500n = 75% LTV
  liquidationThresholdBps: bigint, // e.g. 8000n
  liquidationBonusBps:    bigint, // e.g. 500n = 5%
  closeFactorBps:         bigint, // max debt fraction per liquidation (0n = 100%)
  price:                  bigint, // initial seed price, scaled by 1e6
  baseRateBps:            bigint, // annual rate at 0% utilization
  slopeBps:               bigint, // extra annual rate at 100% utilization
  kinkUtilizationBps:     bigint, // optimal-utilization kink (0n = linear model)
  jumpSlopeBps:           bigint, // extra annual rate per util above the kink
  maxPriceStalenessSecs:  bigint, // reject prices older than this
): HimshaInstruction {
  const data = concat(
    IX.InitMarket,
    u64Le(collateralFactorBps), u64Le(liquidationThresholdBps),
    u64Le(liquidationBonusBps), u64Le(closeFactorBps), u64Le(price),
    u64Le(baseRateBps), u64Le(slopeBps),
    u64Le(kinkUtilizationBps), u64Le(jumpSlopeBps),
    u64Le(maxPriceStalenessSecs),
  );
  return new HimshaInstruction(
    PROGRAM_IDS.moneyMarket,
    [
      HimshaInstruction.writable(market, false),
      HimshaInstruction.readonly(collateralMint, false),
      HimshaInstruction.readonly(borrowMint, false),
      HimshaInstruction.writable(collateralVault, false),
      HimshaInstruction.writable(borrowVault, false),
      HimshaInstruction.readonly(admin, true),
      HimshaInstruction.readonly(oracleFeed, false),
    ],
    data,
  );
}

/** Sync the market's cached price from its oracle feed. */
export function syncPrice(market: HimshaPublicKey, oracleFeed: HimshaPublicKey): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.moneyMarket,
    [
      HimshaInstruction.writable(market, false),
      HimshaInstruction.readonly(oracleFeed, false),
    ],
    IX.SyncPrice,
  );
}

/** Provide borrow-asset liquidity into the vault (supply side). */
export function addLiquidity(
  market:         HimshaPublicKey,
  providerBorrow: HimshaPublicKey,
  borrowVault:    HimshaPublicKey,
  provider:       HimshaPublicKey,
  amount:         bigint,
): HimshaInstruction {
  const data = concat(IX.AddLiquidity, u64Le(amount));
  return new HimshaInstruction(
    PROGRAM_IDS.moneyMarket,
    [
      HimshaInstruction.writable(market, false),
      HimshaInstruction.writable(providerBorrow, false),
      HimshaInstruction.writable(borrowVault, false),
      HimshaInstruction.readonly(provider, true),
    ],
    data,
  );
}

// Supply / Withdraw / Borrow / Repay share the 5-account layout:
// [market(w), position(w), userToken(w), vault(w), user(signer)].
function vaultIx(
  tag: Uint8Array,
  market: HimshaPublicKey, position: HimshaPublicKey,
  userToken: HimshaPublicKey, vault: HimshaPublicKey, user: HimshaPublicKey,
  amount: bigint,
): HimshaInstruction {
  const data = concat(tag, u64Le(amount));
  return new HimshaInstruction(
    PROGRAM_IDS.moneyMarket,
    [
      HimshaInstruction.writable(market, false),
      HimshaInstruction.writable(position, false),
      HimshaInstruction.writable(userToken, false),
      HimshaInstruction.writable(vault, false),
      HimshaInstruction.readonly(user, true),
    ],
    data,
  );
}

/** Supply collateral (userCollateral -> collateralVault). */
export function supply(market: HimshaPublicKey, position: HimshaPublicKey, userCollateral: HimshaPublicKey, collateralVault: HimshaPublicKey, user: HimshaPublicKey, amount: bigint): HimshaInstruction {
  return vaultIx(IX.Supply, market, position, userCollateral, collateralVault, user, amount);
}
/** Withdraw collateral (must remain healthy). */
export function withdraw(market: HimshaPublicKey, position: HimshaPublicKey, userCollateral: HimshaPublicKey, collateralVault: HimshaPublicKey, user: HimshaPublicKey, amount: bigint): HimshaInstruction {
  return vaultIx(IX.Withdraw, market, position, userCollateral, collateralVault, user, amount);
}
/** Borrow the borrowable asset against supplied collateral. */
export function borrow(market: HimshaPublicKey, position: HimshaPublicKey, userBorrow: HimshaPublicKey, borrowVault: HimshaPublicKey, user: HimshaPublicKey, amount: bigint): HimshaInstruction {
  return vaultIx(IX.Borrow, market, position, userBorrow, borrowVault, user, amount);
}
/** Repay debt. */
export function repay(market: HimshaPublicKey, position: HimshaPublicKey, userBorrow: HimshaPublicKey, borrowVault: HimshaPublicKey, user: HimshaPublicKey, amount: bigint): HimshaInstruction {
  return vaultIx(IX.Repay, market, position, userBorrow, borrowVault, user, amount);
}

/** Liquidate an unhealthy position: repay debt, seize collateral + bonus. */
export function liquidate(
  market:               HimshaPublicKey,
  position:             HimshaPublicKey,
  liquidatorBorrow:     HimshaPublicKey,
  borrowVault:          HimshaPublicKey,
  liquidatorCollateral: HimshaPublicKey,
  collateralVault:      HimshaPublicKey,
  liquidator:           HimshaPublicKey,
  repayAmount:          bigint,
): HimshaInstruction {
  const data = concat(IX.Liquidate, u64Le(repayAmount));
  return new HimshaInstruction(
    PROGRAM_IDS.moneyMarket,
    [
      HimshaInstruction.writable(market, false),
      HimshaInstruction.writable(position, false),
      HimshaInstruction.writable(liquidatorBorrow, false),
      HimshaInstruction.writable(borrowVault, false),
      HimshaInstruction.writable(liquidatorCollateral, false),
      HimshaInstruction.writable(collateralVault, false),
      HimshaInstruction.readonly(liquidator, true),
    ],
    data,
  );
}

export const MoneyMarketProgram = {
  initMarket, addLiquidity, supply, withdraw, borrow, repay, liquidate, syncPrice,
};

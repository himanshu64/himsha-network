import { HimshaInstruction } from '../transaction';
import { HimshaPublicKey, PROGRAM_IDS } from '../pubkey';

const IX = {
  Initialize: new Uint8Array([0]),
  Swap:       new Uint8Array([1]),
  Deposit:    new Uint8Array([2]),
  Withdraw:   new Uint8Array([3]),
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

/** Initialize a new constant-product liquidity pool. */
export function initialize(
  pool:          HimshaPublicKey,
  tokenAMint:    HimshaPublicKey,
  tokenBMint:    HimshaPublicKey,
  reserveA:      HimshaPublicKey,
  reserveB:      HimshaPublicKey,
  lpMint:        HimshaPublicKey,
  payer:         HimshaPublicKey,
  feeNumerator:  bigint,  // e.g. 3n
  feeDenominator: bigint, // e.g. 1000n  → 0.3% fee
): HimshaInstruction {
  const data = concat(IX.Initialize, u64Le(feeNumerator), u64Le(feeDenominator));
  return new HimshaInstruction(
    PROGRAM_IDS.swap,
    [
      HimshaInstruction.writable(pool, false),
      HimshaInstruction.readonly(tokenAMint, false),
      HimshaInstruction.readonly(tokenBMint, false),
      HimshaInstruction.writable(reserveA, false),
      HimshaInstruction.writable(reserveB, false),
      HimshaInstruction.writable(lpMint, false),
      HimshaInstruction.writable(payer, true),
    ],
    data,
  );
}

/**
 * Swap `amountIn` of one token for at least `minAmountOut` of the other.
 * `source` and `destination` are the user's token accounts.
 * `reserveIn` and `reserveOut` are the pool's reserve accounts.
 */
export function swap(
  pool:         HimshaPublicKey,
  source:       HimshaPublicKey,
  destination:  HimshaPublicKey,
  reserveIn:    HimshaPublicKey,
  reserveOut:   HimshaPublicKey,
  user:         HimshaPublicKey,
  amountIn:     bigint,
  minAmountOut: bigint,
): HimshaInstruction {
  const data = concat(IX.Swap, u64Le(amountIn), u64Le(minAmountOut));
  return new HimshaInstruction(
    PROGRAM_IDS.swap,
    [
      HimshaInstruction.readonly(pool, false),
      HimshaInstruction.writable(source, false),
      HimshaInstruction.writable(destination, false),
      HimshaInstruction.writable(reserveIn, false),
      HimshaInstruction.writable(reserveOut, false),
      HimshaInstruction.readonly(user, true),
    ],
    data,
  );
}

/** Deposit liquidity. Receive LP tokens in `userLp`. */
export function deposit(
  pool:      HimshaPublicKey,
  userTokenA: HimshaPublicKey,
  userTokenB: HimshaPublicKey,
  reserveA:  HimshaPublicKey,
  reserveB:  HimshaPublicKey,
  userLp:    HimshaPublicKey,
  user:      HimshaPublicKey,
  lpMint:    HimshaPublicKey,
  maxA:      bigint,
  maxB:      bigint,
  minLp:     bigint,
): HimshaInstruction {
  const data = concat(IX.Deposit, u64Le(maxA), u64Le(maxB), u64Le(minLp));
  return new HimshaInstruction(
    PROGRAM_IDS.swap,
    [
      HimshaInstruction.writable(pool, false),
      HimshaInstruction.writable(userTokenA, false),
      HimshaInstruction.writable(userTokenB, false),
      HimshaInstruction.writable(reserveA, false),
      HimshaInstruction.writable(reserveB, false),
      HimshaInstruction.writable(userLp, false),
      HimshaInstruction.readonly(user, true),
      HimshaInstruction.writable(lpMint, false),
    ],
    data,
  );
}

/** Withdraw liquidity by burning `lpAmount` LP tokens from `lpMint`. */
export function withdraw(
  pool:      HimshaPublicKey,
  userTokenA: HimshaPublicKey,
  userTokenB: HimshaPublicKey,
  reserveA:  HimshaPublicKey,
  reserveB:  HimshaPublicKey,
  userLp:    HimshaPublicKey,
  user:      HimshaPublicKey,
  lpMint:    HimshaPublicKey,
  lpAmount:  bigint,
  minA:      bigint,
  minB:      bigint,
): HimshaInstruction {
  const data = concat(IX.Withdraw, u64Le(lpAmount), u64Le(minA), u64Le(minB));
  return new HimshaInstruction(
    PROGRAM_IDS.swap,
    [
      HimshaInstruction.writable(pool, false),
      HimshaInstruction.writable(userTokenA, false),
      HimshaInstruction.writable(userTokenB, false),
      HimshaInstruction.writable(reserveA, false),
      HimshaInstruction.writable(reserveB, false),
      HimshaInstruction.writable(userLp, false),
      HimshaInstruction.readonly(user, true),
      HimshaInstruction.writable(lpMint, false),
    ],
    data,
  );
}

export const SwapProgram = { initialize, swap, deposit, withdraw };

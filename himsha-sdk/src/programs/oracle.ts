import { HimshaInstruction } from '../transaction';
import { HimshaPublicKey, PROGRAM_IDS } from '../pubkey';

// Borsh enum variant indices — order must match OracleInstruction in Rust.
const IX = {
  InitFeed:        new Uint8Array([0]),
  UpdatePrice:     new Uint8Array([1]),
  AddPublisher:    new Uint8Array([2]),
  RemovePublisher: new Uint8Array([3]),
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

function adminIx(
  feed: HimshaPublicKey, signer: HimshaPublicKey, data: Uint8Array,
): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.oracle,
    [
      HimshaInstruction.writable(feed, false),
      HimshaInstruction.readonly(signer, true),
    ],
    data,
  );
}

/**
 * Create a price feed administered (and initially published) by `authority`.
 *
 * `maxDeviationBps` bounds how far one update may move the aggregate (0 = off);
 * `maxSubmissionAge` (seconds) drops stale publisher submissions from the
 * median (0 = never expire).
 */
export function initFeed(
  feed: HimshaPublicKey,
  authority: HimshaPublicKey,
  maxDeviationBps: bigint = 0n,
  maxSubmissionAge: bigint = 0n,
): HimshaInstruction {
  const data = concat(IX.InitFeed, u64Le(maxDeviationBps), u64Le(maxSubmissionAge));
  return adminIx(feed, authority, data);
}

/** Publish a new fixed-point price (registered publishers only). */
export function updatePrice(
  feed: HimshaPublicKey, publisher: HimshaPublicKey, price: bigint,
): HimshaInstruction {
  return adminIx(feed, publisher, concat(IX.UpdatePrice, u64Le(price)));
}

/** Register an additional publisher (authority only). */
export function addPublisher(
  feed: HimshaPublicKey, authority: HimshaPublicKey, publisher: HimshaPublicKey,
): HimshaInstruction {
  return adminIx(feed, authority, concat(IX.AddPublisher, publisher.toBytes()));
}

/** Remove a publisher and its submission (authority only). */
export function removePublisher(
  feed: HimshaPublicKey, authority: HimshaPublicKey, publisher: HimshaPublicKey,
): HimshaInstruction {
  return adminIx(feed, authority, concat(IX.RemovePublisher, publisher.toBytes()));
}

export const OracleProgram = { initFeed, updatePrice, addPublisher, removePublisher };

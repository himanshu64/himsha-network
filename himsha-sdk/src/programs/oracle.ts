import { HimshaInstruction } from '../transaction';
import { HimshaPublicKey, PROGRAM_IDS } from '../pubkey';

// Borsh enum variant indices — order must match OracleInstruction in Rust.
const IX = {
  InitFeed:    new Uint8Array([0]),
  UpdatePrice: new Uint8Array([1]),
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

/** Create a price feed owned by `authority`. */
export function initFeed(feed: HimshaPublicKey, authority: HimshaPublicKey): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.oracle,
    [
      HimshaInstruction.writable(feed, false),
      HimshaInstruction.readonly(authority, true),
    ],
    IX.InitFeed,
  );
}

/** Publish a new fixed-point price (authority only). */
export function updatePrice(
  feed: HimshaPublicKey, authority: HimshaPublicKey, price: bigint,
): HimshaInstruction {
  const data = concat(IX.UpdatePrice, u64Le(price));
  return new HimshaInstruction(
    PROGRAM_IDS.oracle,
    [
      HimshaInstruction.writable(feed, false),
      HimshaInstruction.readonly(authority, true),
    ],
    data,
  );
}

export const OracleProgram = { initFeed, updatePrice };

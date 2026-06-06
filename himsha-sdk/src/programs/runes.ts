import { HimshaInstruction } from '../transaction';
import { HimshaPublicKey, PROGRAM_IDS } from '../pubkey';

// Borsh enum variant indices — order must match RuneInstruction in Rust.
const IX = {
  Etch:        new Uint8Array([0]),
  Mint:        new Uint8Array([1]),
  Transfer:    new Uint8Array([2]),
  Burn:        new Uint8Array([3]),
  InitBalance: new Uint8Array([4]),
} as const;

function u64Le(n: bigint): Uint8Array {
  const buf = new Uint8Array(8);
  new DataView(buf.buffer).setBigUint64(0, n, true);
  return buf;
}
function u32Le(n: number): Uint8Array {
  const buf = new Uint8Array(4);
  new DataView(buf.buffer).setUint32(0, n, true);
  return buf;
}
function u8(n: number): Uint8Array {
  return new Uint8Array([n & 0xff]);
}
function encodeString(s: string): Uint8Array {
  const bytes = new TextEncoder().encode(s);
  return concat(u32Le(bytes.length), bytes);
}
function concat(...arrays: Uint8Array[]): Uint8Array {
  const len = arrays.reduce((s, a) => s + a.length, 0);
  const out = new Uint8Array(len);
  let offset = 0;
  for (const a of arrays) { out.set(a, offset); offset += a.length; }
  return out;
}

/** Open-mint terms (borsh: amount, cap, mints, start, end). */
export interface MintTerms {
  amount: bigint;
  cap:    bigint;   // 0 = unlimited
  mints:  bigint;   // usually 0n at etch time
  start:  bigint;   // unix ts, 0 = no lower bound
  end:    bigint;   // unix ts, 0 = no upper bound
}

/** Borsh Option<MintTerms>: 0x00 for None, 0x01 + fields for Some. */
function encodeTerms(terms?: MintTerms): Uint8Array {
  if (!terms) return u8(0);
  return concat(
    u8(1),
    u64Le(terms.amount), u64Le(terms.cap), u64Le(terms.mints),
    u64Le(terms.start),  u64Le(terms.end),
  );
}

/** Etch (create) a new rune. `symbol` is a unicode codepoint. */
export function etch(
  rune:          HimshaPublicKey,
  etcherBalance: HimshaPublicKey,
  etcher:        HimshaPublicKey,
  name:          string,
  symbol:        number,
  divisibility:  number,
  premine:       bigint,
  terms?:        MintTerms,
): HimshaInstruction {
  const data = concat(
    IX.Etch,
    encodeString(name),
    u32Le(symbol),
    u8(divisibility),
    u64Le(premine),
    encodeTerms(terms),
  );
  return new HimshaInstruction(
    PROGRAM_IDS.runes,
    [
      HimshaInstruction.writable(rune, false),
      HimshaInstruction.writable(etcherBalance, false),
      HimshaInstruction.readonly(etcher, true),
    ],
    data,
  );
}

/** Open-mint: mint `terms.amount` to the destination balance. */
export function mint(
  rune:        HimshaPublicKey,
  destination: HimshaPublicKey,
  minter:      HimshaPublicKey,
): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.runes,
    [
      HimshaInstruction.writable(rune, false),
      HimshaInstruction.writable(destination, false),
      HimshaInstruction.readonly(minter, true),
    ],
    IX.Mint,
  );
}

/** Transfer `amount` between two balance accounts of the same rune. */
export function transfer(
  source:      HimshaPublicKey,
  destination: HimshaPublicKey,
  owner:       HimshaPublicKey,
  amount:      bigint,
): HimshaInstruction {
  const data = concat(IX.Transfer, u64Le(amount));
  return new HimshaInstruction(
    PROGRAM_IDS.runes,
    [
      HimshaInstruction.writable(source, false),
      HimshaInstruction.writable(destination, false),
      HimshaInstruction.readonly(owner, true),
    ],
    data,
  );
}

/** Burn `amount`, reducing circulating supply. */
export function burn(
  rune:   HimshaPublicKey,
  source: HimshaPublicKey,
  owner:  HimshaPublicKey,
  amount: bigint,
): HimshaInstruction {
  const data = concat(IX.Burn, u64Le(amount));
  return new HimshaInstruction(
    PROGRAM_IDS.runes,
    [
      HimshaInstruction.writable(rune, false),
      HimshaInstruction.writable(source, false),
      HimshaInstruction.readonly(owner, true),
    ],
    data,
  );
}

/** Initialize an empty balance account. */
export function initBalance(balance: HimshaPublicKey, owner: HimshaPublicKey): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.runes,
    [
      HimshaInstruction.writable(balance, false),
      HimshaInstruction.readonly(owner, false),
    ],
    IX.InitBalance,
  );
}

export const RunesProgram = { etch, mint, transfer, burn, initBalance };

import { HimshaInstruction } from '../transaction';
import { HimshaPublicKey, PROGRAM_IDS } from '../pubkey';

/** System program instruction tag bytes (1-byte discriminant). */
const IX = {
  CreateAccount:            new Uint8Array([0]),
  CreateAccountWithAnchor:  new Uint8Array([1]),
  Transfer:                 new Uint8Array([2]),
  Assign:                   new Uint8Array([3]),
  Allocate:                 new Uint8Array([4]),
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

function concat(...arrays: Uint8Array[]): Uint8Array {
  const len = arrays.reduce((s, a) => s + a.length, 0);
  const out = new Uint8Array(len);
  let offset = 0;
  for (const a of arrays) { out.set(a, offset); offset += a.length; }
  return out;
}

/** Create a new account funded by `payer`. */
export function createAccount(
  payer:      HimshaPublicKey,
  newAccount: HimshaPublicKey,
  lamports:   bigint,
  space:      bigint,
  owner:      HimshaPublicKey,
): HimshaInstruction {
  const data = concat(IX.CreateAccount, u64Le(lamports), u64Le(space), owner.toBytes());
  return new HimshaInstruction(
    PROGRAM_IDS.system,
    [
      HimshaInstruction.writable(payer, true),
      HimshaInstruction.writable(newAccount, true),
    ],
    data,
  );
}

/** Create an account anchored to a Bitcoin UTXO. */
export function createAccountWithAnchor(
  payer:      HimshaPublicKey,
  newAccount: HimshaPublicKey,
  txid:       Uint8Array,  // 32 bytes
  vout:       number,
  space:      bigint,
  owner:      HimshaPublicKey,
): HimshaInstruction {
  const data = concat(IX.CreateAccountWithAnchor, txid, u32Le(vout), u64Le(space), owner.toBytes());
  return new HimshaInstruction(
    PROGRAM_IDS.system,
    [
      HimshaInstruction.writable(payer, true),
      HimshaInstruction.writable(newAccount, true),
    ],
    data,
  );
}

/** Transfer lamports between accounts. */
export function transfer(
  from:     HimshaPublicKey,
  to:       HimshaPublicKey,
  lamports: bigint,
): HimshaInstruction {
  const data = concat(IX.Transfer, u64Le(lamports));
  return new HimshaInstruction(
    PROGRAM_IDS.system,
    [
      HimshaInstruction.writable(from, true),
      HimshaInstruction.writable(to, false),
    ],
    data,
  );
}

/** Change the owning program of an account. */
export function assign(account: HimshaPublicKey, owner: HimshaPublicKey): HimshaInstruction {
  const data = concat(IX.Assign, owner.toBytes());
  return new HimshaInstruction(
    PROGRAM_IDS.system,
    [HimshaInstruction.writable(account, true)],
    data,
  );
}

/** Grow an account's data buffer. */
export function allocate(account: HimshaPublicKey, space: bigint): HimshaInstruction {
  const data = concat(IX.Allocate, u64Le(space));
  return new HimshaInstruction(
    PROGRAM_IDS.system,
    [HimshaInstruction.writable(account, true)],
    data,
  );
}

export const SystemProgram = { createAccount, createAccountWithAnchor, transfer, assign, allocate };

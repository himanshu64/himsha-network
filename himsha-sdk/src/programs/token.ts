import { HimshaInstruction } from '../transaction';
import { HimshaPublicKey, PROGRAM_IDS } from '../pubkey';

const IX = {
  InitializeMint:    new Uint8Array([0]),
  InitializeAccount: new Uint8Array([1]),
  MintTo:            new Uint8Array([2]),
  Transfer:          new Uint8Array([3]),
  Burn:              new Uint8Array([4]),
  Approve:           new Uint8Array([5]),
  Revoke:            new Uint8Array([6]),
  FreezeAccount:     new Uint8Array([7]),
  ThawAccount:       new Uint8Array([8]),
  CloseAccount:      new Uint8Array([9]),
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

function optionalKey(key?: HimshaPublicKey): Uint8Array {
  if (key) return concat(new Uint8Array([1]), key.toBytes());
  return new Uint8Array([0]);
}

/** Initialize a new token mint. */
export function initializeMint(
  mint:             HimshaPublicKey,
  mintAuthority:    HimshaPublicKey,
  decimals:         number,
  freezeAuthority?: HimshaPublicKey,
): HimshaInstruction {
  const data = concat(
    IX.InitializeMint,
    new Uint8Array([decimals]),
    mintAuthority.toBytes(),
    optionalKey(freezeAuthority),
  );
  return new HimshaInstruction(
    PROGRAM_IDS.token,
    [HimshaInstruction.writable(mint, false)],
    data,
  );
}

/** Initialize a token account for `owner` to hold tokens of `mint`. */
export function initializeAccount(
  tokenAccount: HimshaPublicKey,
  mint:         HimshaPublicKey,
  owner:        HimshaPublicKey,
): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.token,
    [
      HimshaInstruction.writable(tokenAccount, false),
      HimshaInstruction.readonly(mint, false),
      HimshaInstruction.readonly(owner, false),
    ],
    IX.InitializeAccount,
  );
}

/** Mint `amount` tokens to `destination`. Requires `mintAuthority` to sign. */
export function mintTo(
  mint:          HimshaPublicKey,
  destination:   HimshaPublicKey,
  mintAuthority: HimshaPublicKey,
  amount:        bigint,
): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.token,
    [
      HimshaInstruction.writable(mint, false),
      HimshaInstruction.writable(destination, false),
      HimshaInstruction.readonly(mintAuthority, true),
    ],
    concat(IX.MintTo, u64Le(amount)),
  );
}

/** Transfer `amount` tokens from `source` to `destination`. */
export function transfer(
  source:      HimshaPublicKey,
  destination: HimshaPublicKey,
  owner:       HimshaPublicKey,
  amount:      bigint,
): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.token,
    [
      HimshaInstruction.writable(source, false),
      HimshaInstruction.writable(destination, false),
      HimshaInstruction.readonly(owner, true),
    ],
    concat(IX.Transfer, u64Le(amount)),
  );
}

/** Burn `amount` tokens from `tokenAccount`. */
export function burn(
  tokenAccount: HimshaPublicKey,
  mint:         HimshaPublicKey,
  owner:        HimshaPublicKey,
  amount:       bigint,
): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.token,
    [
      HimshaInstruction.writable(tokenAccount, false),
      HimshaInstruction.writable(mint, false),
      HimshaInstruction.readonly(owner, true),
    ],
    concat(IX.Burn, u64Le(amount)),
  );
}

/** Approve `delegate` to spend up to `amount` tokens from `source`. */
export function approve(
  source:   HimshaPublicKey,
  delegate: HimshaPublicKey,
  owner:    HimshaPublicKey,
  amount:   bigint,
): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.token,
    [
      HimshaInstruction.writable(source, false),
      HimshaInstruction.readonly(delegate, false),
      HimshaInstruction.readonly(owner, true),
    ],
    concat(IX.Approve, u64Le(amount)),
  );
}

/** Revoke delegate from `source`. */
export function revoke(source: HimshaPublicKey, owner: HimshaPublicKey): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.token,
    [
      HimshaInstruction.writable(source, false),
      HimshaInstruction.readonly(owner, true),
    ],
    IX.Revoke,
  );
}

/** Freeze a token account. Requires freeze authority. */
export function freezeAccount(tokenAccount: HimshaPublicKey, freezeAuthority: HimshaPublicKey): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.token,
    [
      HimshaInstruction.writable(tokenAccount, false),
      HimshaInstruction.readonly(freezeAuthority, true),
    ],
    IX.FreezeAccount,
  );
}

/** Thaw a frozen token account. */
export function thawAccount(tokenAccount: HimshaPublicKey, freezeAuthority: HimshaPublicKey): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.token,
    [
      HimshaInstruction.writable(tokenAccount, false),
      HimshaInstruction.readonly(freezeAuthority, true),
    ],
    IX.ThawAccount,
  );
}

/** Close an empty token account and reclaim lamports. */
export function closeAccount(
  tokenAccount: HimshaPublicKey,
  destination:  HimshaPublicKey,
  owner:        HimshaPublicKey,
): HimshaInstruction {
  return new HimshaInstruction(
    PROGRAM_IDS.token,
    [
      HimshaInstruction.writable(tokenAccount, false),
      HimshaInstruction.writable(destination, false),
      HimshaInstruction.readonly(owner, true),
    ],
    IX.CloseAccount,
  );
}

export const TokenProgram = {
  initializeMint, initializeAccount, mintTo, transfer, burn,
  approve, revoke, freezeAccount, thawAccount, closeAccount,
};

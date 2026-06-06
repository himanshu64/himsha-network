import { createHash } from 'crypto';

const BASE58_ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';

function base58Encode(bytes: Uint8Array): string {
  let num = BigInt('0x' + Buffer.from(bytes).toString('hex') || '0');
  let result = '';
  const base = BigInt(58);
  while (num > 0n) {
    result = BASE58_ALPHABET[Number(num % base)] + result;
    num = num / base;
  }
  for (const byte of bytes) {
    if (byte === 0) result = '1' + result;
    else break;
  }
  return result;
}

function base58Decode(str: string): Uint8Array {
  let num = 0n;
  const base = BigInt(58);
  for (const char of str) {
    const idx = BASE58_ALPHABET.indexOf(char);
    if (idx < 0) throw new Error(`Invalid base58 character: ${char}`);
    num = num * base + BigInt(idx);
  }
  const bytes: number[] = [];
  while (num > 0n) {
    bytes.unshift(Number(num & 0xffn));
    num >>= 8n;
  }
  for (const char of str) {
    if (char === '1') bytes.unshift(0);
    else break;
  }
  return new Uint8Array(bytes);
}

/** 32-byte public key. */
export class HimshaPublicKey {
  private readonly _bytes: Uint8Array;

  constructor(bytes: Uint8Array | string) {
    if (typeof bytes === 'string') {
      this._bytes = base58Decode(bytes);
    } else {
      this._bytes = bytes;
    }
    if (this._bytes.length !== 32) {
      throw new Error(`PublicKey must be 32 bytes, got ${this._bytes.length}`);
    }
  }

  static fromBase58(s: string): HimshaPublicKey {
    return new HimshaPublicKey(s);
  }

  static fromSeed(seed: string): HimshaPublicKey {
    const hash = createHash('sha256').update(seed, 'utf8').digest();
    return new HimshaPublicKey(new Uint8Array(hash));
  }

  static findProgramAddress(seeds: Uint8Array[], programId: HimshaPublicKey): [HimshaPublicKey, number] {
    for (let nonce = 255; nonce >= 0; nonce--) {
      const hasher = createHash('sha256');
      for (const seed of seeds) hasher.update(seed);
      hasher.update(new Uint8Array([nonce]));
      hasher.update(programId.toBytes());
      hasher.update(Buffer.from('himsha::pda', 'utf8'));
      return [new HimshaPublicKey(new Uint8Array(hasher.digest())), nonce];
    }
    throw new Error('Could not find program address');
  }

  toBase58(): string { return base58Encode(this._bytes); }
  toBytes(): Uint8Array { return this._bytes; }
  toString(): string { return this.toBase58(); }

  equals(other: HimshaPublicKey): boolean {
    return this._bytes.every((b, i) => b === other._bytes[i]);
  }
}

/** Well-known built-in program IDs. */
export const PROGRAM_IDS = {
  system:      HimshaPublicKey.fromSeed('himsha::system_program'),
  token:       HimshaPublicKey.fromSeed('himsha::token_program'),
  ata:         HimshaPublicKey.fromSeed('himsha::ata_program'),
  swap:        HimshaPublicKey.fromSeed('himsha::swap_program'),
  lending:     HimshaPublicKey.fromSeed('himsha::lending_program'),
  nftMetadata: HimshaPublicKey.fromSeed('himsha::nft_metadata_program'),
  runes:       HimshaPublicKey.fromSeed('himsha::runes_program'),
  moneyMarket: HimshaPublicKey.fromSeed('himsha::money_market_program'),
  vault:       HimshaPublicKey.fromSeed('himsha::vault_program'),
  oracle:      HimshaPublicKey.fromSeed('himsha::oracle_program'),
} as const;

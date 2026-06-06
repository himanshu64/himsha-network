import { createHash } from 'crypto';
import { HimshaPublicKey } from './pubkey';

export interface AccountMeta {
  pubkey: HimshaPublicKey;
  isSigner: boolean;
  isWritable: boolean;
}

export class HimshaInstruction {
  constructor(
    public readonly programId: HimshaPublicKey,
    public readonly accounts: AccountMeta[],
    public readonly data: Uint8Array,
  ) {}

  static writable(pubkey: HimshaPublicKey, isSigner: boolean): AccountMeta {
    return { pubkey, isSigner, isWritable: true };
  }

  static readonly(pubkey: HimshaPublicKey, isSigner: boolean): AccountMeta {
    return { pubkey, isSigner, isWritable: false };
  }
}

export class HimshaMessage {
  constructor(
    public readonly signers: HimshaPublicKey[],
    public readonly instructions: HimshaInstruction[],
    public readonly timestamp: bigint = BigInt(Math.floor(Date.now() / 1000)),
  ) {}

  /** SHA-256 of the serialized message — this is what each signer signs. */
  hash(): Uint8Array {
    const hasher = createHash('sha256');
    for (const signer of this.signers) {
      hasher.update(signer.toBytes());
    }
    for (const instr of this.instructions) {
      hasher.update(instr.programId.toBytes());
      for (const acc of instr.accounts) {
        hasher.update(acc.pubkey.toBytes());
        hasher.update(new Uint8Array([acc.isSigner ? 1 : 0, acc.isWritable ? 1 : 0]));
      }
      hasher.update(instr.data);
    }
    const tsBytes = new Uint8Array(8);
    new DataView(tsBytes.buffer).setBigUint64(0, this.timestamp, true);
    hasher.update(tsBytes);
    return new Uint8Array(hasher.digest());
  }

  toJSON() {
    return {
      signers:      this.signers.map(s => s.toBase58()),
      instructions: this.instructions.map(instr => ({
        programId: instr.programId.toBase58(),
        accounts:  instr.accounts.map(a => ({
          pubkey:     a.pubkey.toBase58(),
          isSigner:   a.isSigner,
          isWritable: a.isWritable,
        })),
        data: Buffer.from(instr.data).toString('hex'),
      })),
      timestamp: this.timestamp.toString(),
    };
  }
}

export class HimshaTransaction {
  version = 0;
  signatures: Uint8Array[] = [];

  constructor(private message: HimshaMessage) {}

  /** Add an instruction to this transaction (fluent builder). */
  add(instruction: HimshaInstruction): this {
    // create a new message with the added instruction
    (this.message as any).instructions.push(instruction);
    return this;
  }

  /** Sign with a raw 64-byte Schnorr signature. */
  addSignature(sig: Uint8Array): this {
    if (sig.length !== 64) throw new Error('Signature must be 64 bytes');
    this.signatures.push(sig);
    return this;
  }

  messageHash(): Uint8Array {
    return this.message.hash();
  }

  toJSON() {
    return {
      version:    this.version,
      signatures: this.signatures.map(s => Buffer.from(s).toString('hex')),
      message:    this.message.toJSON(),
    };
  }

  static create(
    signers: HimshaPublicKey[],
    instructions: HimshaInstruction[],
    timestamp?: bigint,
  ): HimshaTransaction {
    return new HimshaTransaction(new HimshaMessage(signers, instructions, timestamp));
  }
}

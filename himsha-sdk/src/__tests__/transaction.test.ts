import { HimshaPublicKey } from '../pubkey';
import { HimshaInstruction, HimshaMessage, HimshaTransaction } from '../transaction';

function makeKey(seed: string): HimshaPublicKey {
  return HimshaPublicKey.fromSeed(seed);
}

describe('HimshaInstruction', () => {
  it('creates writable account meta', () => {
    const pk = makeKey('acc');
    const meta = HimshaInstruction.writable(pk, true);
    expect(meta.pubkey.equals(pk)).toBe(true);
    expect(meta.isSigner).toBe(true);
    expect(meta.isWritable).toBe(true);
  });

  it('creates readonly account meta', () => {
    const pk = makeKey('readonly');
    const meta = HimshaInstruction.readonly(pk, false);
    expect(meta.isWritable).toBe(false);
    expect(meta.isSigner).toBe(false);
  });

  it('constructs with correct fields', () => {
    const programId = makeKey('program');
    const acc = makeKey('account');
    const data = new Uint8Array([1, 2, 3]);
    const instr = new HimshaInstruction(programId, [HimshaInstruction.writable(acc, true)], data);
    expect(instr.programId.equals(programId)).toBe(true);
    expect(instr.accounts).toHaveLength(1);
    expect(instr.data).toEqual(data);
  });
});

describe('HimshaMessage', () => {
  it('hashes deterministically', () => {
    const signer = makeKey('signer');
    const program = makeKey('program');
    const instr = new HimshaInstruction(program, [], new Uint8Array([42]));
    const ts = 1_700_000_000n;

    const msg1 = new HimshaMessage([signer], [instr], ts);
    const msg2 = new HimshaMessage([signer], [instr], ts);
    expect(msg1.hash()).toEqual(msg2.hash());
  });

  it('hash changes with different timestamp', () => {
    const signer = makeKey('signer');
    const program = makeKey('program');
    const instr = new HimshaInstruction(program, [], new Uint8Array([1]));

    const h1 = new HimshaMessage([signer], [instr], 1000n).hash();
    const h2 = new HimshaMessage([signer], [instr], 2000n).hash();
    expect(h1).not.toEqual(h2);
  });

  it('hash changes with different instruction data', () => {
    const signer = makeKey('signer');
    const program = makeKey('program');

    const h1 = new HimshaMessage([signer], [new HimshaInstruction(program, [], new Uint8Array([0]))], 0n).hash();
    const h2 = new HimshaMessage([signer], [new HimshaInstruction(program, [], new Uint8Array([1]))], 0n).hash();
    expect(h1).not.toEqual(h2);
  });

  it('serializes to JSON with correct structure', () => {
    const signer = makeKey('signer');
    const program = makeKey('prog');
    const msg = new HimshaMessage([signer], [new HimshaInstruction(program, [], new Uint8Array([7]))], 999n);
    const json = msg.toJSON();

    expect(json.signers).toHaveLength(1);
    expect(json.signers[0]).toBe(signer.toBase58());
    expect(json.instructions).toHaveLength(1);
    expect(json.instructions[0].programId).toBe(program.toBase58());
    expect(json.timestamp).toBe('999');
  });
});

describe('HimshaTransaction', () => {
  it('creates with correct version', () => {
    const signer = makeKey('signer');
    const tx = HimshaTransaction.create([signer], []);
    expect(tx.version).toBe(0);
  });

  it('adds a valid 64-byte signature', () => {
    const signer = makeKey('signer');
    const tx = HimshaTransaction.create([signer], []);
    const sig = new Uint8Array(64).fill(0xab);
    tx.addSignature(sig);
    expect(tx.signatures).toHaveLength(1);
    expect(tx.signatures[0]).toEqual(sig);
  });

  it('rejects signature with wrong length', () => {
    const signer = makeKey('signer');
    const tx = HimshaTransaction.create([signer], []);
    expect(() => tx.addSignature(new Uint8Array(63))).toThrow('64 bytes');
    expect(() => tx.addSignature(new Uint8Array(65))).toThrow('64 bytes');
  });

  it('message hash is 32 bytes', () => {
    const signer = makeKey('signer');
    const tx = HimshaTransaction.create([signer], []);
    expect(tx.messageHash()).toHaveLength(32);
  });

  it('serializes to JSON', () => {
    const signer = makeKey('signer');
    const sig = new Uint8Array(64).fill(0xff);
    const tx = HimshaTransaction.create([signer], []).addSignature(sig);
    const json = tx.toJSON();

    expect(json.version).toBe(0);
    expect(json.signatures).toHaveLength(1);
    expect(json.signatures[0]).toBe('ff'.repeat(64));
    expect(json.message).toBeDefined();
  });
});

import { HimshaPublicKey, PROGRAM_IDS } from '../pubkey';

describe('HimshaPublicKey', () => {
  it('creates from 32-byte Uint8Array', () => {
    const bytes = new Uint8Array(32).fill(1);
    const pk = new HimshaPublicKey(bytes);
    expect(pk.toBytes()).toEqual(bytes);
  });

  it('throws on wrong byte length', () => {
    expect(() => new HimshaPublicKey(new Uint8Array(31))).toThrow('32 bytes');
    expect(() => new HimshaPublicKey(new Uint8Array(33))).toThrow('32 bytes');
  });

  it('round-trips through base58', () => {
    const original = new HimshaPublicKey(new Uint8Array(32).fill(42));
    const b58 = original.toBase58();
    expect(b58).toBeTruthy();
    const restored = HimshaPublicKey.fromBase58(b58);
    expect(restored.equals(original)).toBe(true);
  });

  it('creates deterministic key from seed', () => {
    const a = HimshaPublicKey.fromSeed('test-seed');
    const b = HimshaPublicKey.fromSeed('test-seed');
    const c = HimshaPublicKey.fromSeed('different-seed');
    expect(a.equals(b)).toBe(true);
    expect(a.equals(c)).toBe(false);
  });

  it('finds program address deterministically', () => {
    const programId = HimshaPublicKey.fromSeed('my-program');
    const [pda1, bump1] = HimshaPublicKey.findProgramAddress([new TextEncoder().encode('vault')], programId);
    const [pda2, bump2] = HimshaPublicKey.findProgramAddress([new TextEncoder().encode('vault')], programId);
    expect(pda1.equals(pda2)).toBe(true);
    expect(bump1).toBe(bump2);
  });

  it('PROGRAM_IDS are deterministic', () => {
    expect(PROGRAM_IDS.system.toBase58()).toBe(HimshaPublicKey.fromSeed('himsha::system_program').toBase58());
    expect(PROGRAM_IDS.token.toBase58()).toBe(HimshaPublicKey.fromSeed('himsha::token_program').toBase58());
    expect(PROGRAM_IDS.ata.toBase58()).toBe(HimshaPublicKey.fromSeed('himsha::ata_program').toBase58());
    expect(PROGRAM_IDS.swap.toBase58()).toBe(HimshaPublicKey.fromSeed('himsha::swap_program').toBase58());
    expect(PROGRAM_IDS.lending.toBase58()).toBe(HimshaPublicKey.fromSeed('himsha::lending_program').toBase58());
  });

  it('toString returns base58', () => {
    const pk = HimshaPublicKey.fromSeed('hello');
    expect(pk.toString()).toBe(pk.toBase58());
  });
});

import {
  verifyStateProof,
  verifyAccountInState,
  leafHash,
  StateProof,
} from '../stateProof';

// Cross-language test vector generated from the Rust `himsha_runtime::merkle`
// implementation (4 leaves: key[0]=i, key[1]=0xab, bytes=[i,i+1,i+2,i+3]).
// If the TS verifier ever diverges from the Rust tree, these break.
const ROOT = '54eee82002490e070e17b13ed29afff514ac9249c3a76550759097d58c9b0dab';

const PROOFS: StateProof[] = [
  {
    stateRoot: ROOT,
    leaf: '8527bb0fce4bb40f3d73380831b581d7109dbca530aff310a4aa32357534697e',
    index: 0,
    siblings: [
      'a04698423f2735af8a0577bd581670a08c2c047044b4ae77478e51464054f181',
      '89b4335a72d20c925aaccac5e3e1bbe013ecea898b418a51f51c55b961308068',
    ],
  },
  {
    stateRoot: ROOT,
    leaf: 'a04698423f2735af8a0577bd581670a08c2c047044b4ae77478e51464054f181',
    index: 1,
    siblings: [
      '8527bb0fce4bb40f3d73380831b581d7109dbca530aff310a4aa32357534697e',
      '89b4335a72d20c925aaccac5e3e1bbe013ecea898b418a51f51c55b961308068',
    ],
  },
  {
    stateRoot: ROOT,
    leaf: 'f3ae1a5531bd2bae2efb209184cf11f14e963233167f9d181292ba1e7857cfda',
    index: 2,
    siblings: [
      'dbb00c8d0561563563c096b54c39852bb84cd21957bec6e5812c6d5b398b6736',
      '5acfe6cfb257faedb60069ffd4b9da2b4251cf6054d19df7dd40483f090e2167',
    ],
  },
  {
    stateRoot: ROOT,
    leaf: 'dbb00c8d0561563563c096b54c39852bb84cd21957bec6e5812c6d5b398b6736',
    index: 3,
    siblings: [
      'f3ae1a5531bd2bae2efb209184cf11f14e963233167f9d181292ba1e7857cfda',
      '5acfe6cfb257faedb60069ffd4b9da2b4251cf6054d19df7dd40483f090e2167',
    ],
  },
];

describe('state-root inclusion proofs (cross-language with Rust merkle)', () => {
  it('verifies every Rust-generated proof against the Rust root', () => {
    for (const p of PROOFS) {
      expect(verifyStateProof(p, ROOT)).toBe(true);
    }
  });

  it('rejects a proof against the wrong root', () => {
    const wrong = 'ff' + ROOT.slice(2);
    expect(verifyStateProof(PROOFS[2], wrong)).toBe(false);
  });

  it('rejects a tampered leaf', () => {
    const tampered = { ...PROOFS[1], leaf: '00' + PROOFS[1].leaf.slice(2) };
    expect(verifyStateProof(tampered, ROOT)).toBe(false);
  });

  it('matches the Rust leaf hash exactly', () => {
    // Rust: leaf_hash(key{[0]=7}, [1,2,3]) — domain-separated SHA-256.
    const key = new Uint8Array(32);
    key[0] = 7;
    const leaf = leafHash(key, new Uint8Array([1, 2, 3]));
    const hex = Array.from(leaf, (x) => x.toString(16).padStart(2, '0')).join('');
    expect(hex).toBe('3be7157c455ae9986535cece016a8df2e1f24c5018a4a49cb4d4d4a31ed28f0f');
  });

  it('verifyAccountInState ties the proof leaf to the caller bytes', () => {
    // Reconstruct leaf 2's (key, bytes): i=2 → key[0]=2,key[1]=0xab, bytes=[2,3,4,5].
    const key = new Uint8Array(32);
    key[0] = 2;
    key[1] = 0xab;
    const bytes = new Uint8Array([2, 3, 4, 5]);
    expect(verifyAccountInState(key, bytes, PROOFS[2], ROOT)).toBe(true);
    // Wrong bytes → rejected even though the proof path is valid.
    const wrongBytes = new Uint8Array([9, 9, 9, 9]);
    expect(verifyAccountInState(key, wrongBytes, PROOFS[2], ROOT)).toBe(false);
  });
});

import { createHash } from 'crypto';

/**
 * State-root inclusion proof, as returned by `himsha_getStateProof`.
 *
 * Mirrors the Rust `himsha_runtime::merkle` tree exactly so a client can verify,
 * with no trust in the node, that an account is committed in the state root —
 * and, when the root matches `anchoredStateRoot`, in the Bitcoin-anchored state.
 */
export interface StateProof {
  stateRoot: string;            // hex
  leaf: string;                 // hex (SHA-256(0x00 ‖ key ‖ account_bytes))
  index: number;
  siblings: string[];           // hex
  anchoredSlot?: number | null;
  anchoredStateRoot?: string | null;
  anchoredBtcTxid?: string | null;
}

function sha256(...parts: Uint8Array[]): Uint8Array {
  const h = createHash('sha256');
  for (const p of parts) h.update(p);
  return new Uint8Array(h.digest());
}

function fromHex(hex: string): Uint8Array {
  if (hex.length % 2 !== 0) throw new Error(`odd-length hex: ${hex}`);
  const out = new Uint8Array(hex.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  return out;
}

function toHex(b: Uint8Array): string {
  return Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
}

const LEAF_TAG = new Uint8Array([0x00]);
const NODE_TAG = new Uint8Array([0x01]);

/** Leaf hash for an account: SHA-256(0x00 ‖ key ‖ accountBytes). */
export function leafHash(key: Uint8Array, accountBytes: Uint8Array): Uint8Array {
  return sha256(LEAF_TAG, key, accountBytes);
}

/** Internal-node hash: SHA-256(0x01 ‖ left ‖ right). */
function nodeHash(left: Uint8Array, right: Uint8Array): Uint8Array {
  return sha256(NODE_TAG, left, right);
}

/**
 * Recompute the root a proof attests to and compare against `root` (hex).
 * Matches `MerkleProof::verify` in Rust: at each level the sibling sits on the
 * right when the running index is even, on the left when odd.
 */
export function verifyStateProof(proof: StateProof, root: string): boolean {
  let h = fromHex(proof.leaf);
  let idx = proof.index;
  for (const sibHex of proof.siblings) {
    const sib = fromHex(sibHex);
    h = (idx & 1) === 0 ? nodeHash(h, sib) : nodeHash(sib, h);
    idx = Math.floor(idx / 2);
  }
  return toHex(h) === root.toLowerCase();
}

/**
 * Verify that `accountBytes` (the borsh-encoded StoredAccount the client holds
 * for `key`) is the exact value committed under `proof`. This is the strongest
 * check: it ties the proof's leaf to the caller's own account bytes, then walks
 * the tree to the root — so a node cannot serve a proof for a different value.
 */
export function verifyAccountInState(
  key: Uint8Array,
  accountBytes: Uint8Array,
  proof: StateProof,
  root: string,
): boolean {
  if (toHex(leafHash(key, accountBytes)) !== proof.leaf.toLowerCase()) return false;
  return verifyStateProof(proof, root);
}

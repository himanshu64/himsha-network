"""
State-root inclusion proofs for HIMSHA Network.

Mirrors the Rust ``himsha_runtime::merkle`` tree and the TypeScript
``stateProof.ts`` exactly, so a client can verify — with no trust in the node —
that an account is committed in a block's state root, and (when the root matches
``anchored_state_root``) in the Bitcoin-anchored state.

Hashing is domain-separated:
    - leaf:     SHA-256(0x00 || key || account_bytes)
    - internal: SHA-256(0x01 || left || right)

Verification walks ``siblings`` leaf->root: at each level the sibling sits on the
right when the running index is even, on the left when odd.
"""

from __future__ import annotations

import hashlib

LEAF_TAG = b"\x00"
NODE_TAG = b"\x01"


def _sha256(*parts: bytes) -> bytes:
    h = hashlib.sha256()
    for p in parts:
        h.update(p)
    return h.digest()


def leaf_hash(key: bytes, account_bytes: bytes) -> bytes:
    """Leaf hash for an account: SHA-256(0x00 || key || account_bytes)."""
    return _sha256(LEAF_TAG, key, account_bytes)


def node_hash(left: bytes, right: bytes) -> bytes:
    """Internal-node hash: SHA-256(0x01 || left || right)."""
    return _sha256(NODE_TAG, left, right)


def verify_state_proof(proof: dict, root_hex: str) -> bool:
    """
    Recompute the root a proof attests to and compare against ``root_hex``.

    Matches ``MerkleProof::verify`` in Rust: at each level the sibling sits on
    the right when the running index is even, on the left when odd.

    ``proof`` is the JSON object returned by ``himsha_getStateProof`` (snake_case
    keys: ``leaf``, ``index``, ``siblings``, ...).
    """
    h = bytes.fromhex(proof["leaf"])
    idx = int(proof["index"])
    for sib_hex in proof["siblings"]:
        sib = bytes.fromhex(sib_hex)
        h = node_hash(h, sib) if (idx & 1) == 0 else node_hash(sib, h)
        idx >>= 1
    return h.hex() == root_hex.lower()


def verify_account_in_state(
    key: bytes,
    account_bytes: bytes,
    proof: dict,
    root_hex: str,
) -> bool:
    """
    Verify that ``account_bytes`` (the encoded account the client holds for
    ``key``) is the exact value committed under ``proof``.

    This is the strongest check: it ties the proof's leaf to the caller's own
    account bytes, then walks the tree to the root — so a node cannot serve a
    proof for a different value.
    """
    if leaf_hash(key, account_bytes).hex() != proof["leaf"].lower():
        return False
    return verify_state_proof(proof, root_hex)

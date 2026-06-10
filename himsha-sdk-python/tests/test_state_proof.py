"""
Cross-language Merkle state-proof vectors.

These are the EXACT vectors produced by the Rust ``himsha_runtime::merkle``
implementation (4 leaves: key[0]=i, key[1]=0xab, account_bytes=[i, i+1, i+2, i+3]),
verified here to prove the Python verifier matches Rust/TS bit-for-bit.
"""

from himsha_sdk.state_proof import (
    leaf_hash,
    verify_state_proof,
    verify_account_in_state,
)

# Root of the 4-leaf tree.
ROOT = "54eee82002490e070e17b13ed29afff514ac9249c3a76550759097d58c9b0dab"

# Inclusion proof for leaf index 2.
PROOF_2 = {
    "leaf": "f3ae1a5531bd2bae2efb209184cf11f14e963233167f9d181292ba1e7857cfda",
    "index": 2,
    "siblings": [
        "dbb00c8d0561563563c096b54c39852bb84cd21957bec6e5812c6d5b398b6736",
        "5acfe6cfb257faedb60069ffd4b9da2b4251cf6054d19df7dd40483f090e2167",
    ],
}


def _key(i: int) -> bytes:
    """Reconstruct leaf i's key: 32 bytes, key[0]=i, key[1]=0xab."""
    k = bytearray(32)
    k[0] = i
    k[1] = 0xAB
    return bytes(k)


def test_leaf_hash_vector():
    # leaf_hash(key{[0]=7}, [1,2,3]) — standalone vector from Rust.
    k = bytearray(32)
    k[0] = 7
    got = leaf_hash(bytes(k), bytes([1, 2, 3])).hex()
    assert got == "3be7157c455ae9986535cece016a8df2e1f24c5018a4a49cb4d4d4a31ed28f0f"


def test_verify_state_proof_against_root():
    assert verify_state_proof(PROOF_2, ROOT) is True


def test_verify_state_proof_wrong_root_fails():
    wrong = "00" + ROOT[2:]
    assert verify_state_proof(PROOF_2, wrong) is False


def test_verify_state_proof_uppercase_root():
    # Root comparison is case-insensitive.
    assert verify_state_proof(PROOF_2, ROOT.upper()) is True


def test_proof_2_leaf_matches_reconstructed_account():
    # Leaf 2: key[0]=2, key[1]=0xab, account_bytes=[2,3,4,5].
    key = _key(2)
    account_bytes = bytes([2, 3, 4, 5])
    assert leaf_hash(key, account_bytes).hex() == PROOF_2["leaf"]


def test_verify_account_in_state_real_bytes():
    key = _key(2)
    account_bytes = bytes([2, 3, 4, 5])
    assert verify_account_in_state(key, account_bytes, PROOF_2, ROOT) is True


def test_verify_account_in_state_wrong_bytes_fails():
    key = _key(2)
    wrong_bytes = bytes([9, 9, 9, 9])
    assert verify_account_in_state(key, wrong_bytes, PROOF_2, ROOT) is False

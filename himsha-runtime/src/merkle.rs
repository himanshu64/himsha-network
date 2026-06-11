//! Binary Merkle tree over account state — the basis for the block `state_root`
//! and the inclusion proofs served by `himsha_getStateProof`.
//!
//! Hashing is domain-separated so a leaf can never be reinterpreted as an
//! internal node (second-preimage resistance):
//!   - leaf:     `SHA-256(0x00 ‖ key[32] ‖ account_bytes)`
//!   - internal: `SHA-256(0x01 ‖ left[32] ‖ right[32])`
//!
//! Leaves are taken in **ascending key order** (the order redb iterates the
//! account table), so the root is a deterministic commitment to the full state.
//! When a level has an odd number of nodes the last is duplicated (Bitcoin
//! style); that is safe here because account keys are unique, so two identical
//! leaves cannot occur. The empty tree has the all-zero root.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Hash of an empty state (no accounts).
pub const EMPTY_ROOT: [u8; 32] = [0u8; 32];

/// Leaf hash for an account: `SHA-256(0x00 ‖ key ‖ account_bytes)`.
pub fn leaf_hash(key: &[u8; 32], account_bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([0x00]);
    h.update(key);
    h.update(account_bytes);
    h.finalize().into()
}

/// Internal-node hash: `SHA-256(0x01 ‖ left ‖ right)`.
pub fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([0x01]);
    h.update(left);
    h.update(right);
    h.finalize().into()
}

/// Fold one level into the next, duplicating the last node when odd.
fn parent_level(level: &[[u8; 32]]) -> Vec<[u8; 32]> {
    let mut next = Vec::with_capacity(level.len().div_ceil(2));
    let mut i = 0;
    while i < level.len() {
        let l = level[i];
        let r = if i + 1 < level.len() { level[i + 1] } else { l };
        next.push(node_hash(&l, &r));
        i += 2;
    }
    next
}

/// Merkle root over `leaves` (in the caller's order). Empty → [`EMPTY_ROOT`].
pub fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return EMPTY_ROOT;
    }
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        level = parent_level(&level);
    }
    level[0]
}

/// An inclusion proof: `leaf` at `index` among the ordered leaves, with the
/// `siblings` needed to recompute the root. Verifiable without the tree.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
pub struct MerkleProof {
    pub leaf: [u8; 32],
    pub index: u64,
    pub siblings: Vec<[u8; 32]>,
}

impl MerkleProof {
    /// Recompute the root this proof attests to and compare against `root`.
    pub fn verify(&self, root: &[u8; 32]) -> bool {
        let mut h = self.leaf;
        let mut idx = self.index;
        for sib in &self.siblings {
            h = if idx & 1 == 0 {
                node_hash(&h, sib)
            } else {
                node_hash(sib, &h)
            };
            idx >>= 1;
        }
        &h == root
    }
}

/// Build an inclusion proof for `index` among `leaves`. Returns `None` if the
/// index is out of range or the tree is empty.
pub fn build_proof(leaves: &[[u8; 32]], index: usize) -> Option<MerkleProof> {
    if index >= leaves.len() {
        return None;
    }
    let mut siblings = Vec::new();
    let mut idx = index;
    let mut level = leaves.to_vec();
    while level.len() > 1 {
        // Sibling is the paired node; at an odd-length tail the node is paired
        // with itself (matching the duplication in `parent_level`).
        let sib = if idx ^ 1 < level.len() {
            level[idx ^ 1]
        } else {
            level[idx]
        };
        siblings.push(sib);
        level = parent_level(&level);
        idx >>= 1;
    }
    Some(MerkleProof {
        leaf: leaves[index],
        index: index as u64,
        siblings,
    })
}

// =============================================================
#[cfg(test)]
mod tests {
    use super::*;

    fn leaves(n: usize) -> Vec<[u8; 32]> {
        (0..n)
            .map(|i| {
                let mut k = [0u8; 32];
                k[0] = i as u8;
                leaf_hash(&k, &[i as u8; 4])
            })
            .collect()
    }

    #[test]
    fn empty_tree_has_zero_root() {
        assert_eq!(merkle_root(&[]), EMPTY_ROOT);
        assert_eq!(build_proof(&[], 0), None);
    }

    #[test]
    fn single_leaf_root_is_the_leaf() {
        let l = leaves(1);
        assert_eq!(merkle_root(&l), l[0]);
        let p = build_proof(&l, 0).unwrap();
        assert!(p.siblings.is_empty());
        assert!(p.verify(&l[0]));
    }

    #[test]
    fn proofs_verify_for_every_index_power_of_two() {
        let l = leaves(8);
        let root = merkle_root(&l);
        for i in 0..8 {
            let p = build_proof(&l, i).unwrap();
            assert!(p.verify(&root), "proof {i} should verify");
        }
    }

    #[test]
    fn proofs_verify_for_odd_leaf_counts() {
        // Odd counts exercise the duplicate-last-node path at multiple levels.
        for n in [3usize, 5, 7, 9, 13] {
            let l = leaves(n);
            let root = merkle_root(&l);
            for i in 0..n {
                let p = build_proof(&l, i).unwrap();
                assert!(p.verify(&root), "n={n} index={i} should verify");
            }
        }
    }

    #[test]
    fn proof_against_wrong_root_fails() {
        let l = leaves(6);
        let root = merkle_root(&l);
        let mut wrong = root;
        wrong[0] ^= 0xff;
        assert!(!build_proof(&l, 2).unwrap().verify(&wrong));
    }

    #[test]
    fn tampered_proof_fails() {
        let l = leaves(6);
        let root = merkle_root(&l);
        let mut p = build_proof(&l, 2).unwrap();
        p.leaf[0] ^= 0x01; // claim a different leaf at the same position
        assert!(!p.verify(&root));
    }

    #[test]
    fn out_of_range_index_has_no_proof() {
        assert_eq!(build_proof(&leaves(4), 4), None);
    }

    #[test]
    fn root_changes_when_any_leaf_changes() {
        let mut l = leaves(5);
        let r0 = merkle_root(&l);
        l[3][0] ^= 0x01;
        assert_ne!(merkle_root(&l), r0);
    }
}

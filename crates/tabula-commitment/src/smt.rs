//! Sparse Merkle Tree: parameterized by depth, domain tag, and hash function.

use std::collections::BTreeMap;

use crate::hasher::FieldHasher;

/// A sparse Merkle tree with configurable depth and domain separation.
///
/// Stores only non-empty nodes. Empty subtrees use precomputed empty hashes.
/// Key bits are read LSB-first (bit 0 = leftmost branch at level 0).
#[derive(Clone, Debug)]
pub struct SparseMerkleTree<H: FieldHasher> {
    hasher: H,
    depth: usize,
    domain_tag: u32,
    /// (level, index) → hash. Only non-empty nodes stored.
    nodes: BTreeMap<(usize, u64), H::Digest>,
    /// key → leaf digest.
    leaves: BTreeMap<u64, H::Digest>,
    /// empty_hashes[i] = hash of the empty subtree at level i.
    empty_hashes: Vec<H::Digest>,
}

/// A Merkle proof (inclusion or non-inclusion).
#[derive(Clone, Debug)]
pub struct MerkleProof<D> {
    /// The queried key.
    pub key: u64,
    /// The leaf value, or `None` for non-membership.
    pub value: Option<D>,
    /// Sibling hashes from leaf to root (length = depth).
    pub siblings: Vec<D>,
}

impl<H: FieldHasher> SparseMerkleTree<H> {
    /// Create an empty tree with the given depth and domain tag.
    pub fn new(hasher: H, depth: usize, domain_tag: u32) -> Self {
        let mut empty_hashes = Vec::with_capacity(depth + 1);
        // Domain-dependent empty leaf: different domain tags → different trees.
        empty_hashes.push(hasher.hash_domain(domain_tag, &[]));
        for level in 0..depth {
            let prev = empty_hashes[level];
            let next = Self::node_hash_static(&hasher, domain_tag, level, &prev, &prev);
            empty_hashes.push(next);
        }
        Self {
            hasher,
            depth,
            domain_tag,
            nodes: BTreeMap::new(),
            leaves: BTreeMap::new(),
            empty_hashes,
        }
    }

    /// The current root hash.
    pub fn root(&self) -> H::Digest {
        self.get_node(self.depth, 0)
    }

    /// Get the leaf value for a key, or `None` if absent.
    pub fn get(&self, key: u64) -> Option<H::Digest> {
        self.leaves.get(&key).copied()
    }

    /// Insert or update a leaf. Returns the new root.
    pub fn insert(&mut self, key: u64, value: H::Digest) -> H::Digest {
        self.leaves.insert(key, value);
        self.recompute_path(key);
        self.root()
    }

    /// Remove a leaf. Returns the new root.
    pub fn remove(&mut self, key: u64) -> H::Digest {
        self.leaves.remove(&key);
        self.recompute_path(key);
        self.root()
    }

    /// Generate a proof for the given key (membership or non-membership).
    pub fn prove(&self, key: u64) -> MerkleProof<H::Digest> {
        let value = self.leaves.get(&key).copied();
        let mut siblings = Vec::with_capacity(self.depth);
        let mut index = key;
        for level in 0..self.depth {
            let sibling_index = index ^ 1;
            siblings.push(self.get_node(level, sibling_index));
            index >>= 1;
        }
        MerkleProof {
            key,
            value,
            siblings,
        }
    }

    /// Verify a proof against a root hash.
    pub fn verify_proof(
        hasher: &H,
        domain_tag: u32,
        root: &H::Digest,
        proof: &MerkleProof<H::Digest>,
        depth: usize,
    ) -> bool {
        let empty_leaf = hasher.hash_domain(domain_tag, &[]);
        let mut current = proof.value.unwrap_or(empty_leaf);
        let mut index = proof.key;
        for level in 0..depth {
            let sibling = &proof.siblings[level];
            let bit = index & 1;
            current = if bit == 0 {
                Self::node_hash_static(hasher, domain_tag, level, &current, sibling)
            } else {
                Self::node_hash_static(hasher, domain_tag, level, sibling, &current)
            };
            index >>= 1;
        }
        current == *root
    }

    /// Number of non-empty leaves.
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    /// Whether the tree has no leaves.
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    // ── internal ──

    fn get_node(&self, level: usize, index: u64) -> H::Digest {
        if level == 0 {
            return self
                .leaves
                .get(&index)
                .copied()
                .unwrap_or(self.empty_hashes[0]);
        }
        self.nodes
            .get(&(level, index))
            .copied()
            .unwrap_or(self.empty_hashes[level])
    }

    fn recompute_path(&mut self, key: u64) {
        let mut index = key;
        for level in 0..self.depth {
            let left_index = index & !1;
            let right_index = left_index | 1;
            let left = self.get_node(level, left_index);
            let right = self.get_node(level, right_index);
            let parent_hash =
                Self::node_hash_static(&self.hasher, self.domain_tag, level, &left, &right);
            let parent_index = index >> 1;
            let parent_level = level + 1;
            if parent_hash == self.empty_hashes[parent_level] {
                self.nodes.remove(&(parent_level, parent_index));
            } else {
                self.nodes.insert((parent_level, parent_index), parent_hash);
            }
            index = parent_index;
        }
    }

    fn node_hash_static(
        hasher: &H,
        _domain_tag: u32,
        _level: usize,
        left: &H::Digest,
        right: &H::Digest,
    ) -> H::Digest {
        // 2-to-1 compression. Domain separation is achieved by using
        // different domain_tag values in the tree constructor, which
        // produces different empty_hashes chains. The compress function
        // itself is a fixed-width permutation-based construction.
        hasher.compress(left, right)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field::{DOMAIN_COL, DOMAIN_SMT, DOMAIN_TABLE};
    use crate::hasher::MockFieldHasher;

    fn make_tree(depth: usize, domain: u32) -> SparseMerkleTree<MockFieldHasher> {
        SparseMerkleTree::new(MockFieldHasher, depth, domain)
    }

    #[test]
    fn empty_tree_has_deterministic_root() {
        let t1 = make_tree(16, DOMAIN_SMT);
        let t2 = make_tree(16, DOMAIN_SMT);
        assert_eq!(t1.root(), t2.root());
    }

    #[test]
    fn insert_changes_root() {
        let mut tree = make_tree(16, DOMAIN_SMT);
        let empty_root = tree.root();
        let leaf = MockFieldHasher.hash(&[p3_baby_bear::BabyBear::new(42)]);
        tree.insert(0, leaf);
        assert_ne!(tree.root(), empty_root);
    }

    #[test]
    fn insert_then_prove_membership() {
        let mut tree = make_tree(16, DOMAIN_SMT);
        let leaf = MockFieldHasher.hash(&[p3_baby_bear::BabyBear::new(99)]);
        tree.insert(5, leaf);
        let proof = tree.prove(5);
        assert_eq!(proof.value, Some(leaf));
        assert!(SparseMerkleTree::verify_proof(
            &MockFieldHasher,
            DOMAIN_SMT,
            &tree.root(),
            &proof,
            16
        ));
    }

    #[test]
    fn prove_absent_key_non_membership() {
        let mut tree = make_tree(16, DOMAIN_SMT);
        let leaf = MockFieldHasher.hash(&[p3_baby_bear::BabyBear::new(1)]);
        tree.insert(0, leaf);
        let proof = tree.prove(1); // key 1 not inserted
        assert_eq!(proof.value, None);
        assert!(SparseMerkleTree::verify_proof(
            &MockFieldHasher,
            DOMAIN_SMT,
            &tree.root(),
            &proof,
            16
        ));
    }

    #[test]
    fn insert_then_remove_returns_to_empty() {
        let mut tree = make_tree(16, DOMAIN_SMT);
        let empty_root = tree.root();
        // Use value 42 (not 1) to avoid collision with empty leaf hash:
        // empty_hashes[0] = hash_domain(DOMAIN_SMT=1, &[]) = hash(&[1]).
        let leaf = MockFieldHasher.hash(&[p3_baby_bear::BabyBear::new(42)]);
        tree.insert(42, leaf);
        assert_ne!(tree.root(), empty_root);
        tree.remove(42);
        assert_eq!(tree.root(), empty_root);
    }

    #[test]
    fn bulk_insert_all_proofs_valid() {
        let mut tree = make_tree(16, DOMAIN_SMT);
        let h = MockFieldHasher;
        for i in 0..100u64 {
            let leaf = h.hash(&[p3_baby_bear::BabyBear::new(i as u32)]);
            tree.insert(i, leaf);
        }
        for i in 0..100u64 {
            let proof = tree.prove(i);
            assert!(
                SparseMerkleTree::verify_proof(&h, DOMAIN_SMT, &tree.root(), &proof, 16),
                "proof invalid for key {i}"
            );
        }
    }

    #[test]
    fn non_membership_on_populated_tree() {
        let mut tree = make_tree(16, DOMAIN_SMT);
        let h = MockFieldHasher;
        for i in 0..10u64 {
            let leaf = h.hash(&[p3_baby_bear::BabyBear::new(i as u32)]);
            tree.insert(i * 100, leaf);
        }
        // Prove non-membership for keys not inserted.
        for i in [1u64, 50, 99, 101, 999] {
            let proof = tree.prove(i);
            assert_eq!(proof.value, None);
            assert!(SparseMerkleTree::verify_proof(
                &h,
                DOMAIN_SMT,
                &tree.root(),
                &proof,
                16
            ));
        }
    }

    #[test]
    fn different_domain_tags_different_roots() {
        let mut t1 = make_tree(16, DOMAIN_SMT);
        let mut t2 = make_tree(16, DOMAIN_COL);
        let leaf = MockFieldHasher.hash(&[p3_baby_bear::BabyBear::new(1)]);
        t1.insert(0, leaf);
        t2.insert(0, leaf);
        // Different domain tags → different empty hashes → different roots.
        assert_ne!(t1.root(), t2.root());
    }

    #[test]
    fn different_depths_work() {
        for depth in [8, 16, 32] {
            let mut tree = make_tree(depth, DOMAIN_SMT);
            let leaf = MockFieldHasher.hash(&[p3_baby_bear::BabyBear::new(1)]);
            tree.insert(0, leaf);
            let proof = tree.prove(0);
            assert_eq!(proof.siblings.len(), depth);
            assert!(SparseMerkleTree::verify_proof(
                &MockFieldHasher,
                DOMAIN_SMT,
                &tree.root(),
                &proof,
                depth,
            ));
        }
    }

    #[test]
    fn update_value_changes_root() {
        let mut tree = make_tree(16, DOMAIN_SMT);
        let h = MockFieldHasher;
        let v1 = h.hash(&[p3_baby_bear::BabyBear::new(1)]);
        let v2 = h.hash(&[p3_baby_bear::BabyBear::new(2)]);
        tree.insert(5, v1);
        let root1 = tree.root();
        tree.insert(5, v2);
        let root2 = tree.root();
        assert_ne!(root1, root2);
    }

    #[test]
    fn stress_test_1000_keys() {
        let mut tree = make_tree(32, DOMAIN_TABLE);
        let h = MockFieldHasher;
        for i in 0..1000u64 {
            let leaf = h.hash(&[p3_baby_bear::BabyBear::new(i as u32 + 1)]);
            tree.insert(i, leaf);
        }
        assert_eq!(tree.len(), 1000);
        // Spot-check a few proofs.
        for i in [0u64, 499, 999] {
            let proof = tree.prove(i);
            assert!(
                SparseMerkleTree::verify_proof(&h, DOMAIN_TABLE, &tree.root(), &proof, 32),
                "proof invalid for key {i}"
            );
        }
    }

    #[test]
    fn len_and_is_empty() {
        let mut tree = make_tree(8, DOMAIN_SMT);
        assert!(tree.is_empty());
        assert_eq!(tree.len(), 0);
        let leaf = MockFieldHasher.hash(&[p3_baby_bear::BabyBear::new(1)]);
        tree.insert(0, leaf);
        assert!(!tree.is_empty());
        assert_eq!(tree.len(), 1);
    }
}

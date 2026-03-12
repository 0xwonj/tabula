# Structural Queries on Committed State in ZK Systems

Research into how ZK proof systems and verifiable databases handle structural
queries (min, max, range, aggregate, successor/predecessor) on committed state.

## 1. Authenticated Data Structure Queries in ZK

### 1.1 Proving "Minimum Value in a Set"

No ZK system directly proves "give me the minimum" as a single primitive. Instead,
systems compose two sub-proofs:

**Pattern A — Sorted commitment + boundary extraction:**
The prover commits to data in sorted order (via a sorted hash chain or sorted
Merkle tree). The minimum is the first element. The circuit asserts:
- The chain/tree is correctly sorted (constrain `a[i] < a[i+1]` for all pairs).
- The claimed minimum equals the first element in the sorted structure.
- The sorted structure commits to the same multiset as the original data
  (via permutation argument or LogUp).

Cost: O(N) constraints for sorting proof, O(1) for extraction.

**Pattern B — Witness + exhaustive comparison:**
The prover supplies the claimed minimum as a witness, then proves for every
element `e` in the committed set: `min <= e`. This requires:
- N range-check constraints (each proving `e - min >= 0`).
- A membership proof that `min` is itself in the set.

Cost: O(N) range checks.

**Pattern C — Accumulator-based:**
RSA accumulators (Boneh et al., 2018) support constant-size membership and
non-membership proofs. To prove minimum, combine a membership proof for `min`
with a non-membership proof for the range `[0, min-1]`. However, RSA
accumulators inside SNARKs are expensive (~thousands of constraints per
modular exponentiation).

### 1.2 Proving "No Element Exists in Range [a,b]"

**Sorted structure approach:**
If data is committed in sorted order, proving an empty range `[a,b]` means
finding two consecutive elements `x, y` in the sorted set where `x < a` and
`y > b`. This is a constant-size proof: two membership proofs plus two range
checks.

**Indexed Merkle Tree approach (Aztec):**
Each leaf stores `{value, next_index, next_value}` forming a sorted linked
list over the tree leaves. Non-existence of any value `v` is proven by finding
a "low leaf" whose value < v and whose next_value > v. This requires:
- 1 membership proof of the low leaf (depth hashes, ~32 levels).
- 2 range checks (low.value < v, v < low.next_value).
Total: ~32 hashes + 2 range checks.

For range emptiness `[a,b]`: find one low leaf with value < a and
next_value > b. Same cost as point non-existence.

**Sparse Merkle Tree approach (Polygon zkEVM, circomlib):**
Non-existence in a standard SMT is proven by navigating to the expected
leaf position and finding either:
1. An empty node (zero hash), or
2. A leaf with a different key (remaining key mismatch).

The circuit uses a state machine per level with states {top, old0, bot, new1,
upd, na}. Non-existence requires showing the key path leads to null or a
mismatched leaf. Cost: O(depth) hash constraints. Standard SMT depth is 256
for 256-bit keys, making this expensive compared to indexed trees (depth 32).

SMTs cannot efficiently prove range emptiness — they can only prove point
non-existence. Proving `[a,b]` is empty requires individual non-existence
proofs for every integer in the range, which is impractical.

### 1.3 Proving Successor/Predecessor in an Ordered Set

**Indexed Merkle Tree (best known approach):**
Each leaf has `{value, next_index, next_value}`. The successor of value `v`
is `next_value` of the leaf containing `v`. Proof:
- Membership proof of the leaf with value=v (O(depth) hashes).
- The successor is read directly from next_value.
- Total: 1 membership proof.

For predecessor: traverse the linked list to find the leaf whose
next_value = v. That leaf's value is the predecessor. This requires the
prover to search off-chain, then prove membership of the found leaf.
Cost: same as successor.

**Sorted hash chain (Tabula's current SSMC):**
The StateShardChip enforces strict key ordering between consecutive rows
(`constrain_key_ordering`). To prove successor of key k:
- Find the row with key=k.
- The next row's key is the successor.
- Both rows are linked by the sorted chain constraint.
- Cost: O(1) additional constraints beyond the chain itself, but the full
  chain of N elements must be committed (O(N) total).

**Standard SMT/MPT:**
These structures do not naturally support successor queries. Finding a
successor requires walking the tree structure, which has no efficient
circuit representation. Worst case: O(depth) sibling proofs per step,
with an unbounded number of steps.

### 1.4 Proving Aggregates (SUM, COUNT) Over Committed Data

**Running accumulator in sorted chain:**
Add an accumulator column to the trace:
- `sum_acc[0] = value[0]`
- `sum_acc[i] = sum_acc[i-1] + value[i]`
- Constraint: `sum_acc[i] - sum_acc[i-1] - value[i] = 0`
- Final sum = `sum_acc[N-1]`
- Count is trivially the number of real rows.

Cost: N constraints for the accumulator chain + 1 public input for the result.

**vSQL approach (Zhang et al., USENIX Security 2017):**
Decomposes SQL aggregates into verifiable polynomial delegation. SUM becomes
a dot product verified via polynomial evaluation. COUNT is the degree of the
polynomial. MIN/MAX are reduced to sorting + boundary extraction (Pattern A
above). The key innovation: instead of building a full arithmetic circuit for
each query, they use a "verifiable polynomial delegation" protocol that
achieves sublinear verification for certain aggregate types.

**Conditional aggregates (SUM WHERE predicate):**
Add a selector column `sel[i]` (0 or 1) and compute:
- `filtered_sum_acc[i] = filtered_sum_acc[i-1] + sel[i] * value[i]`
- Constrain `sel[i]` to match the predicate (e.g., range check for
  `value[i] >= threshold`).

## 2. Sparse Merkle Trees: Structural Query Capabilities

### 2.1 Can SMTs Prove Min/Max?

**No, not efficiently.** Standard SMTs organize keys by their bit-string path,
not by value ordering. There is no relationship between tree position and key
magnitude. Finding the minimum requires scanning all leaves — there is no
subtree that is guaranteed to contain the smallest key.

The only approach: commit the full dataset in a sorted structure alongside the
SMT, then cross-reference. This doubles the commitment cost.

### 2.2 SMT Successor/Predecessor

**Not supported.** Adjacent keys in an SMT share a common bit prefix, but
"adjacent in bit-prefix" has no relationship to "adjacent in value ordering."
The predecessor of key `k` in value order could be anywhere in the tree.

### 2.3 Non-Existence Proofs in SMTs

Two mechanisms (from Polygon zkEVM documentation and circomlib circuits):

**Mechanism 1 — Empty node:** Navigate the key's bit path. If the path reaches
a zero/empty node before a leaf, the key does not exist. Proof: O(depth)
hashes plus the empty node witness.

**Mechanism 2 — Key mismatch:** Navigate the path until reaching a leaf whose
remaining key differs from the target. The circuit verifies:
- The leaf is at the correct position in the tree (Merkle path valid).
- The leaf's remaining key differs from the target's remaining key.
- Security requires different hash functions for leaf vs. branch nodes
  (`H_leaf` vs `H_noleaf`) to prevent fake-leaf attacks.

From circomlib's `smtverifier.circom`:
```
// Non-inclusion check: oldKey != key when old leaf is present
keysOk.out === 0  // Forces old key to differ from target key
```

Cost: O(depth) hash computations. For 256-bit keys, depth = 256 hashes.

### 2.4 SMT vs. Sorted Merkle Trees for Ordered Queries

| Capability | SMT (256-deep) | Indexed Merkle Tree (32-deep) | Sorted Hash Chain |
|---|---|---|---|
| Point membership | O(256) hashes | O(32) hashes | O(N) chain |
| Point non-existence | O(256) hashes | O(32) hashes + 2 RC | Not applicable |
| Range non-existence | Impractical | O(32) hashes + 2 RC | O(1) if sorted |
| Successor/predecessor | Not supported | O(32) hashes | O(1) in chain |
| Min/Max | Not supported | O(32) hashes (find boundary leaf) | O(1) first/last |
| Aggregate (SUM) | Not supported | Not supported | O(N) accumulator |
| Insert cost | O(256) hashes | O(96) hashes + 2 RC | O(N) re-sort |
| Update cost | O(256) hashes | O(96) hashes + pointer updates | O(N) re-chain |

**Verdict:** SMTs are optimized for point membership/non-membership with large
key spaces. For any ordered or aggregate query, sorted structures (indexed
Merkle trees or sorted hash chains) are strictly superior.

## 3. Sorted Hash Chains / Sorted Memory Structural Queries

### 3.1 How Sorted Memory Commitment Enables Structural Proofs

Tabula's SSMC (Sorted State Merkle Commitment) pattern, visible in
`MemoryShardChip` and `StateShardChip`:

**Key ordering constraint** (`constrain_key_ordering`):
Between consecutive real rows with different keys, the circuit enforces
`local.key < next.key` using a strict inequality gadget with half-limb
decomposition and range checks. This guarantees the trace is sorted by key.

**Within-key tx ordering** (`constrain_tx_ordering`):
For rows with the same key, `tx_diff = next.tx_index - local.tx_index - 1`
is range-checked, enforcing ascending transaction order.

This sorted structure inherently supports:

- **Min key:** First real row's key in the trace.
- **Max key:** Last real row's key (row before is_real drops to 0).
- **Successor:** Next row's key when keys differ.
- **Non-existence:** A key `k` does not exist if there exist consecutive rows
  with keys `a < k < b` and no row has key = k. The sorted constraint
  guarantees completeness — no key can be skipped.
- **Count:** Number of `is_init` rows (each unique key starts with one).
- **Range query [a,b]:** All rows where `a <= key <= b`, identifiable by
  position in the sorted trace.

### 3.2 SSMC Pattern Details

From Tabula's `StateShardChip`:

**Two parallel hash chains** (old state, new state) compute running Poseidon
hash digests over sorted key-value pairs. The chain constraints:
```
// First entry: perm_input = [IV, table_id || col_id || key || value]
// Continuation: perm_input = [prev_acc, table_id || col_id || key || value]
// carry: non-entry rows propagate hash_acc unchanged
```

**Merge logic** handles state transitions (old_only, write_only, old+write,
delete) with source encoding bits `(s1, s0)`.

**Gap rows** are inserted for keys in the new state but not the old, allowing
the sorted chain to include all keys from both states.

### 3.3 Verification Costs by Query Type

| Query | Constraint Cost | Notes |
|---|---|---|
| Key exists? | 0 extra | Already in sorted trace |
| Key not exists? | 0 extra | Gap between consecutive keys proves absence |
| Min/Max key | 0 extra | Read first/last real row |
| Successor(k) | 0 extra | Next row's key |
| Count(predicate) | O(N) | Running counter column |
| SUM(column) | O(N) | Running accumulator column |
| Range [a,b] | O(1) extra | Identify boundary rows |

The "0 extra" cost means the property is already enforced by the base
sorted chain constraints. No additional circuit work is needed beyond
the base SSMC proof.

## 4. ZK-Friendly Ordered Data Structures

### 4.1 Indexed Merkle Trees (Aztec Protocol)

**Structure:** Standard Merkle tree (depth 32) where each leaf stores:
```
{value: Field, next_index: u32, next_value: Field}
```
The leaves form a sorted linked list threaded through the tree.

**Non-membership proof:**
Find the "low leaf" — a leaf with `low.value < target < low.next_value`.
Prove membership of the low leaf via standard Merkle path. Two range checks
confirm the target falls within the gap.

**Insertion:**
1. Find the low leaf for the new value.
2. Update low leaf: `low.next_value = new_value`.
3. Insert new leaf: `{new_value, low.next_index, low.next_value}`.
4. Update the old low leaf's next_index to point to the new leaf.
5. Recompute Merkle path for both modified leaves.

**Batch insertion (Aztec circuit):**
Values are sorted in descending order. Low leaves are validated sequentially,
with "pending insertions" tracked for values not yet committed to the tree.
For 4 values in a subtree: ~327 hashes vs. ~2032 for sparse tree (8x savings).

**Circuit constraints per operation:**
- 3 * depth hashes (~96 for depth 32)
- 2 range checks per non-membership proof
- Pointer update constraints

**Limitation:** No efficient aggregate queries. The linked list is not
contiguous in the trace — following it requires random access into the tree.

### 4.2 B+ Trees in ZK

No production ZK system uses B+ trees. The challenge:
- B+ tree nodes have variable fan-out, requiring conditional logic per child.
- Rebalancing (splits, merges) creates complex, data-dependent circuit paths.
- Each node access requires a Merkle proof, so a B+ tree with fan-out F at
  depth D requires D * F hash constraints per lookup.

Theoretical advantage: range queries are naturally contiguous in leaf nodes.
But the circuit overhead for node structure makes this impractical vs. sorted
chains.

### 4.3 Skip Lists in ZK

No production implementation exists. Skip lists would require:
- Proving the probabilistic level assignment is correct (or deterministic via
  hash of key).
- Multiple Merkle proofs per level traversed.
- Non-deterministic path selection (the prover chooses which levels to skip).

The non-deterministic nature actually works well for ZK — the prover supplies
the skip path as witness, and the circuit verifies each link. But the
variable-length proof makes circuit design complex.

### 4.4 Merkle Patricia Tries (Ethereum)

**Structure:** Four node types (NULL, branch, leaf, extension) with
path compression via hex-prefix encoding. Deterministic and cryptographically
verifiable.

**Existence proof:** Provide hashes of each node along the path from root to
leaf. Verifier reconstructs the path and confirms the root hash matches.

**Non-existence proof:** Demonstrate that navigating the key path leads to a
null node or a leaf with a different key suffix.

**Structural query limitations:**
- No range queries (keys are hashed, destroying ordering).
- No successor/predecessor.
- No min/max.
- The hex-prefix encoding makes circuit implementation expensive — 4 node
  types require multiplexer logic at each level.

**ZK circuit cost:** Scroll and Polygon use MPT proofs in their zkEVMs.
The circuit cost is high: each MPT step requires hash verification plus
node-type dispatch. Polygon zkEVM uses a dedicated Storage State Machine
with PIL (Polynomial Identity Language) constraints.

### 4.5 Verkle Trees

**Structure:** Wide Merkle tree (fan-out 256-1024) using polynomial
commitments (KZG or IPA) instead of hash functions.

**Proof size advantage:** ~320 bytes per value proof (vs. ~1KB for binary
Merkle). Multi-proofs are efficient: 100 values cost ~13.6KB.

**Structural query capabilities:**
- Point membership: Yes, via polynomial evaluation proof.
- Non-existence: Possible via proof that the polynomial evaluates to zero
  at the target position.
- Range queries: Not supported — key ordering is not preserved.
- Min/Max: Not supported.

Verkle trees optimize proof *size*, not structural query power. They are
state-of-the-art for Ethereum state proofs but do not enable ordered queries.

## 5. Application Patterns Requiring Structural Queries

### 5.1 DEX Order Books

**Best price query:** "What is the lowest ask / highest bid?"
- Equivalent to MIN(ask_prices) and MAX(bid_prices).
- **Solution pattern:** Two sorted hash chains — one for bids (descending),
  one for asks (ascending). Best bid = first element of bid chain; best
  ask = first element of ask chain.
- **Circuit cost:** The sorted chain is already committed. Extracting the
  boundary is O(1).

**Price range query:** "All orders between price A and B?"
- **Solution:** In a sorted chain, identify the boundary rows where
  `price >= A` and `price <= B`. All rows between these boundaries are
  in the range.
- **Circuit cost:** 2 range checks for boundaries.

**Order matching:** Prove that a trade at price P satisfies both the best
bid and best ask constraints.
- **Solution:** Prove best_bid >= P >= best_ask using the sorted chains.

### 5.2 Auctions (Highest Bid)

- Identical to MAX query on bid column.
- Sorted chain: highest bid is the last element.
- Indexed Merkle tree: traverse the linked list to find the leaf with
  no successor (next_value = infinity sentinel).

### 5.3 Token Registries

**Existence proof:** "Is token X registered?"
- SMT membership proof (if registry uses SMT): O(depth) hashes.
- Indexed Merkle tree: O(32) hashes.
- Sorted chain: binary search off-chain, then point to the row.

**Non-existence proof:** "Token X is not registered."
- SMT: O(depth) hashes (empty node or key mismatch).
- Indexed Merkle tree: O(32) hashes + 2 range checks.
- Sorted chain: show consecutive rows with keys `a < X < b`.

### 5.4 Rate Limiters (Count/Sum Over Time Windows)

**Count query:** "How many events in [t1, t2]?"
- Sort events by timestamp in a chain.
- Add a running counter column.
- Count in window = counter_at_t2 - counter_at_t1.
- Circuit cost: O(N) for the counter chain, O(1) for the subtraction.

**Sum query:** "Total amount transferred in last 24 hours?"
- Same pattern with a running sum accumulator instead of counter.

## 6. Specific Systems Analysis

### 6.1 Polygon zkEVM Storage State Machine

**Architecture:** Microprocessor design with firmware (zkASM in ROM) and
hardware (PIL polynomial identities).

**Data structure:** Sparse Merkle Tree with 256-bit keys.

**CRUD operations** proven via the SMT Processor circuit:
- Operations encoded as 2-bit function code: NOP(00), UPDATE(01), INSERT(10),
  DELETE(11).
- State machine at each tree level tracks transitions through states:
  {top, old0, bot, new1, upd, na}.
- Root verification: `checkOldInput.enabled <== enabled`
- State finality: exactly one terminal state at deepest level.
- Key integrity: identical keys enforced for UPDATE operations.

**Security measures:**
- Separate hash functions for leaves vs. branches (`H_leaf` vs `H_noleaf`)
  to prevent fake-leaf attacks.
- Remaining key binding: `L_x = H_leaf(RK_x || V_x)` ties keys to values.
- Maximum tree height = key bit-length (uniform across all keys).

**Structural query limitations:** Point operations only (CRUD). No range,
aggregate, or ordered queries. The SMT structure does not preserve key
ordering.

### 6.2 StarkNet State Model

StarkNet uses a **Patricia Trie** (binary, not hexary like Ethereum) for
contract storage. Each contract has its own storage trie.

**Storage proofs** are standard Merkle inclusion proofs on the Patricia trie.
The trie root is committed in the L1 state update.

**Structural queries:** Point membership only. The Patricia trie hashes keys,
destroying value ordering. No range or aggregate queries are natively
supported.

**Hash function:** Pedersen hash (being migrated to Poseidon for ZK
efficiency). Pedersen is ~500x more expensive than Poseidon in circuits.

### 6.3 Aztec's Indexed Merkle Trees

Already described in Section 4.1. Key additional details from the Aztec
protocol circuits:

**Nullifier tree:** Primary use case. Tracks which notes have been spent.
Each nullifier is inserted into the indexed tree; double-spending requires
proving non-existence of the nullifier, which the sorted linked list enables
efficiently.

**Batch insertion circuit:** Sorts pending values in descending order, then
processes sequentially. The circuit tracks "pending insertions" — values
queued but not yet in the tree — to correctly compute low leaves.

```
// From Aztec's indexed_tree.nr:
// sorted_values is a permutation of values_to_insert
// Values sorted by key in descending order
// Low leaf validation: value not greater than low leaf AND
//                      value not less than next leaf
```

**Cost comparison (from Aztec docs):**
- Single insertion: 3 * depth hashes + 2 range checks = ~98 constraints
- Batch of 4 (subtree): ~327 hashes vs. ~2032 for sparse tree

### 6.4 Verdict: Transparency Dictionaries (Tzialla et al., 2021)

Introduced **indexed Merkle trees** and **Phalanx** SNARK for verifiable
label-value stores. Key innovation: amortized constant-sized proofs for read
and write operations.

**Operations supported:**
- Read with proof of correct current value.
- Write with proof of authorized update.
- Non-existence via the indexed tree's linked list structure.

Does not support range or aggregate queries.

### 6.5 vSQL: Verifiable SQL (Zhang et al., 2017)

**Architecture:** Decomposes SQL into three components:
1. **Polynomial delegation** — verifiable evaluation of committed polynomials.
2. **Set operations** — multilinear extensions for WHERE/JOIN/GROUP BY.
3. **Aggregation** — inner products for SUM, polynomial degree for COUNT.

**How specific queries work:**

- **WHERE (range predicate):** Decompose the predicate into bit comparisons.
  For `value >= threshold`: compute `value - threshold`, decompose into bits,
  verify all bits are 0 or 1. O(log(field_size)) constraints per element.

- **SUM:** Reduce to inner product: `SUM(col) = <selector, col>` where
  selector is a 0/1 vector from the WHERE clause. Verified via polynomial
  commitment evaluation.

- **COUNT:** Degree of the selector polynomial.

- **MIN/MAX:** Sort the filtered column, extract boundary. Sorting is proven
  via permutation argument (the sorted column is a permutation of the
  original) plus pairwise comparison constraints.

- **JOIN:** Computed via set intersection using polynomial GCD. Verification
  is sublinear in the table size.

**Verification cost:** O(|query|) rather than O(|database|). The verifier
never touches the full database.

### 6.6 ZK Systems with Declarative Query Languages

**Axiom:** Provides a TypeScript SDK for querying historical Ethereum data.
Queries specify block numbers, account addresses, storage slots. Axiom
generates ZK proofs of the query results using MPT inclusion proofs inside
a SNARK. Not a declarative language — more of a typed API for storage proofs.

**ZKSQL / Hyper Oracle:** Emerging systems that compile SQL-like queries
into ZK circuits. Still research-stage.

No production system offers a full declarative ZK query language. The closest
pattern is: compile a query into a circuit at proof-generation time, where
the circuit structure depends on the query shape but not the data.

## 7. Lookup Arguments for Structural Operations

### 7.1 Plookup (Gabizon and Williamson, 2020)

Proves that every value in column A exists in table S. The core mechanism:

**Sorted permutation:** The prover creates permutations A' and S' such that
like-valued cells in A' are vertically adjacent. The constraint:
```
(A'(X) - S'(X)) * (A'(X) - A'(w^{-1} * X)) = 0
```
Each A' element either matches S' (a new lookup) or matches the previous A'
(a duplicate). This implicitly enforces sorted grouping.

**Range checks via lookup:** To prove all values in column A are in [0, M],
set S = {0, 1, ..., M}. A single Plookup argument proves the range for all
N values simultaneously.

**Structural relevance:** Plookup's sorted-grouping mechanism is a building
block for proving sorted order. If the "table" S is the set of all valid
sorted pairs `(v, v+1)`, a lookup proves the witness column is sorted.

### 7.2 LogUp (Habock, 2022)

Converts the lookup argument from a permutation product into a sum of
logarithmic derivatives:
```
Sum_i 1/(X - a_i) = Sum_j m_j/(X - s_j)
```
where `m_j` is the multiplicity of table entry `s_j`.

**Advantage:** Only one extra committed column (multiplicities). With GKR
optimization (Papini and Habock, 2023), the prover cost is reduced further.

**Relevance to structural queries:** LogUp is used in Tabula's bus system
for cross-chip communication. The multiplicity-based approach naturally
supports "count" queries — the multiplicity column encodes how many times
each value appears.

### 7.3 Caulk (Zapico, Buterin et al., 2022)

Achieves **sublinear prover time** for lookups: O(m * log(m)) where m is the
number of lookups and N is the table size (prover time is independent of N
after O(N log N) preprocessing).

**Relevance:** Enables lookups into very large pre-committed tables without
the prover processing the entire table. This could support queries like
"prove these values exist in a committed registry of 2^30 entries" with
prover cost proportional to the query size, not the registry size.

### 7.4 Lasso (Setty, Thaler, et al., 2023)

**Decomposable tables:** Tables that can be expressed as a tensor product of
smaller subtables. Example: a range check table for 32-bit values decomposes
into four 8-bit subtables.

The prover commits only to the subtable multiplicities, not the full table.
For a 2^128-entry table decomposed into 16-bit chunks, the prover works with
tables of size 2^16 each.

**Structural relevance:** Lasso's decomposition principle means range checks
(the most common structural sub-operation) are essentially free — they
decompose into small, fixed tables that need no explicit materialization.

## 8. Synthesis: Recommendations for Tabula

### 8.1 Current State

Tabula's SSMC already implements the most powerful structural query primitive:
**sorted hash chains with strict key ordering**. The `MemoryShardChip` and
`StateShardChip` enforce:
- Strict key ordering between distinct keys.
- Transaction ordering within same-key accesses.
- Running hash chain commitment over sorted entries.

This supports min, max, successor, predecessor, non-existence, and aggregate
queries with zero or minimal additional constraints.

### 8.2 What Tabula Cannot Currently Do

1. **Sublinear membership proofs:** The sorted chain requires O(N) trace for
   N entries. An indexed Merkle tree would enable O(log N) proofs per query.

2. **Dynamic structural queries:** The trace is fixed at proof time. There
   is no mechanism for a verifier to ask "what is the minimum of column C?"
   after the proof is generated. The query must be known at trace-building
   time so the relevant values can be included as public inputs.

3. **Cross-column aggregates:** SUM/COUNT within a single column's sorted
   chain is natural. Cross-column aggregates (JOIN-like) require new bus
   interactions.

### 8.3 Architectural Options for Adding Structural Queries

**Option 1 — Query-specific witness columns (recommended for near-term):**
For each structural query type, add a witness column to the existing trace:
- MIN/MAX: Public input asserting boundary value, verified against first/last
  real row.
- COUNT: Running counter column with AIR constraint.
- SUM: Running accumulator column with AIR constraint.
- RANGE [a,b]: Boolean selector column + boundary range checks.

Cost: O(1) additional columns per query type, all verified within the
existing sorted chain framework.

**Option 2 — Indexed Merkle Tree for sublinear proofs:**
Replace or supplement the sorted chain with an indexed Merkle tree for
scenarios needing O(log N) point queries. Trade-off: loses the contiguous
sorted layout that makes aggregates natural.

**Option 3 — Hybrid (long-term):**
Sorted chain for aggregate queries + indexed Merkle tree for point
membership/non-membership. The two structures commit to the same data
(proven via LogUp bus matching).

### 8.4 Key Design Insight

The sorted chain is the most ZK-friendly ordered data structure for
*batch* operations (where you process the entire dataset). The indexed
Merkle tree is superior for *point* operations (where you query individual
elements). The choice depends on the query workload:

- **OLAP-like (analytics, aggregates, scans):** Sorted chain wins.
- **OLTP-like (point lookups, existence checks):** Indexed Merkle tree wins.
- **Mixed:** Dual commitment with bus-linked consistency proof.

## References

1. Aztec Protocol. "Indexed Merkle Trees." Aztec Documentation. https://docs.aztec.network/aztec/concepts/storage/trees/indexed_merkle_tree
2. Tzialla, I., Kothapalli, A., Parno, B., Setty, S. "Transparency Dictionaries with Succinct Proofs of Correct Operation." IACR ePrint 2021/1263.
3. Gabizon, A., Williamson, Z. "plookup: A simplified polynomial protocol for lookup tables." IACR ePrint 2020/315.
4. Habock, U. "Multivariate lookups based on logarithmic derivatives." IACR ePrint 2022/1530.
5. Papini, S., Habock, U. "Improving logarithmic derivative lookups using GKR." IACR ePrint 2023/1284.
6. Zapico, A., Buterin, V., et al. "Caulk: Lookup Arguments in Sublinear Time." IACR ePrint 2022/621.
7. Boneh, D., Bunz, B., Fisch, B. "Batching Techniques for Accumulators." IACR ePrint 2018/1188.
8. Zhang, Y., Genkin, D., Katz, J., Papadopoulos, D., Papamanthou, C. "vSQL: Verifying Arbitrary SQL Queries over Dynamic Outsourced Databases." IEEE S&P 2017.
9. Polygon zkEVM. "Storage State Machine." Polygon Documentation.
10. iden3/circomlib. "SMT Verifier and Processor Circuits." GitHub.
11. Setty, S., Thaler, J., Wahby, R. "Unlocking the lookup singularity with Lasso." IACR ePrint 2023/1216.

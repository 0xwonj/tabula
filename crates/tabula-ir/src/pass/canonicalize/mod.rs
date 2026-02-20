//! IR canonicalization: automatically fix fixable NF violations.
//!
//! Pipeline: NF-1 read dedup → slot alias rewriting → NF-4 alias guard → slot renumbering.

mod nf1_dedup_read;
mod nf4_alias_guard;

use std::collections::BTreeMap;

use crate::{Instruction, Slot};

/// Canonicalize an instruction body.
///
/// Pipeline:
/// 1. NF-1 read deduplication (remove duplicate reads to same cell)
/// 2. Slot alias rewriting (fix references to removed slots)
/// 3. NF-4 alias guard insertion (Cmp(Ne)+Assert for write-involved ambiguous pairs)
/// 4. Slot renumbering (compact to contiguous 0..N-1)
pub fn canonicalize(body: Vec<Instruction>) -> Vec<Instruction> {
    let (body, alias_map) = nf1_dedup_read::dedup_reads(body);
    let body = apply_slot_aliases(body, &alias_map);
    let body = nf4_alias_guard::insert_alias_guards(body);
    renumber_slots(body)
}

// ---------------------------------------------------------------------------
// Slot alias rewriting
// ---------------------------------------------------------------------------

/// Resolve a slot through the alias map (transitive, breaks on self-loops).
fn resolve_alias(alias_map: &BTreeMap<Slot, Slot>, mut slot: Slot) -> Slot {
    while let Some(&target) = alias_map.get(&slot) {
        if target == slot {
            break;
        }
        slot = target;
    }
    slot
}

/// Rewrite all slot references in the instruction body using the alias map.
fn apply_slot_aliases(
    body: Vec<Instruction>,
    alias_map: &BTreeMap<Slot, Slot>,
) -> Vec<Instruction> {
    if alias_map.is_empty() {
        return body;
    }
    body.into_iter()
        .map(|instr| instr.map_slots(&|s| resolve_alias(alias_map, s)))
        .collect()
}

// ---------------------------------------------------------------------------
// Slot renumbering
// ---------------------------------------------------------------------------

/// Renumber slots so they are contiguous starting from 0.
///
/// Collects all defined (destination) slots, sorts them, and builds
/// an old→new mapping. Then rewrites all references.
fn renumber_slots(body: Vec<Instruction>) -> Vec<Instruction> {
    let defined: Vec<Slot> = body.iter().flat_map(|i| i.dst_slots()).collect();

    // Check if already contiguous — skip rewrite if so.
    let is_contiguous = defined.iter().enumerate().all(|(i, &s)| s as usize == i);
    if is_contiguous {
        return body;
    }

    // Build old→new mapping based on definition order.
    let mut rename_map: BTreeMap<Slot, Slot> = BTreeMap::new();
    let mut next: Slot = 0;
    for s in &defined {
        rename_map.entry(*s).or_insert_with(|| {
            let n = next;
            next += 1;
            n
        });
    }

    body.into_iter()
        .map(|instr| instr.map_slots(&|s| rename_map.get(&s).copied().unwrap_or(s)))
        .collect()
}

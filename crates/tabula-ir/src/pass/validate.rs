//! Normal-form validation (§2.3–2.6 of semantics-spec).
//!
//! Runs after canonicalization and type-checking as a defensive final check.
//! Rejects programs that violate NF-1 through NF-4.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};

use crate::Instruction;

use super::{RowRelation, row_relation};

/// Validate all four normal-form rules on the instruction body.
pub fn check_normal_form(body: &[Instruction]) -> Result<(), TabulaError> {
    let mut accesses: Vec<StateAccess<'_>> = Vec::new();
    for (i, instr) in body.iter().enumerate() {
        match instr {
            Instruction::Read {
                table, col, row, ..
            } => accesses.push(StateAccess {
                table: *table,
                col: *col,
                row,
                instr_idx: i,
                kind: AccessKind::Read,
            }),
            Instruction::Write {
                table, col, row, ..
            } => accesses.push(StateAccess {
                table: *table,
                col: *col,
                row,
                instr_idx: i,
                kind: AccessKind::Write,
            }),
            _ => {}
        }
    }

    let mut by_tc: BTreeMap<(TableId, ColId), Vec<&StateAccess<'_>>> = BTreeMap::new();
    for acc in &accesses {
        by_tc.entry((acc.table, acc.col)).or_default().push(acc);
    }

    for (&(table, col), group) in &by_tc {
        for i in 0..group.len() {
            for j in (i + 1)..group.len() {
                let a = group[i];
                let b = group[j];
                match row_relation(a.row, b.row) {
                    RowRelation::Ambiguous => {
                        return Err(TabulaError::NfAmbiguousAlias {
                            first: a.instr_idx,
                            second: b.instr_idx,
                            table,
                            col,
                        });
                    }
                    RowRelation::Equal => match (a.kind, b.kind) {
                        (AccessKind::Read, AccessKind::Read) => {
                            return Err(TabulaError::NfUniqueRead {
                                first: a.instr_idx,
                                second: b.instr_idx,
                                table,
                                col,
                            });
                        }
                        (AccessKind::Write, AccessKind::Write) => {
                            return Err(TabulaError::NfUniqueWrite {
                                first: a.instr_idx,
                                second: b.instr_idx,
                                table,
                                col,
                            });
                        }
                        (AccessKind::Write, AccessKind::Read) => {
                            return Err(TabulaError::NfReadAfterWrite {
                                write_at: a.instr_idx,
                                read_at: b.instr_idx,
                                table,
                                col,
                            });
                        }
                        (AccessKind::Read, AccessKind::Write) => {
                            // Read then Write to the same cell is allowed.
                        }
                    },
                    RowRelation::Distinct => {
                        // Different cells — no aliasing concern.
                    }
                }
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccessKind {
    Read,
    Write,
}

struct StateAccess<'a> {
    table: TableId,
    col: ColId,
    row: &'a crate::RowExpr,
    instr_idx: usize,
    kind: AccessKind,
}

use tabula_ir as ir;
use tabula_lang::hir;

pub(crate) fn lower_table_id(id: hir::TableId) -> ir::TableId {
    ir::TableId(id.0)
}

pub(crate) fn lower_field_id(id: hir::FieldId) -> ir::FieldId {
    ir::FieldId(id.0)
}

pub(crate) fn lower_context_field_id(id: hir::ContextFieldId) -> ir::ContextFieldId {
    ir::ContextFieldId(id.0)
}

pub(crate) fn lower_event_id(id: hir::EventId) -> ir::EventId {
    ir::EventId(id.0)
}

pub(crate) fn lower_hash_family(family: hir::HashFamily) -> ir::HashFamily {
    match family {
        hir::HashFamily::Poseidon => ir::HashFamily::Poseidon,
    }
}

pub(crate) fn lower_totality(totality: hir::CapabilityTotality) -> ir::CapabilityTotality {
    match totality {
        hir::CapabilityTotality::Total => ir::CapabilityTotality::Total,
        hir::CapabilityTotality::Checked => ir::CapabilityTotality::Checked,
    }
}

pub(crate) fn lower_query_policy(policy: hir::CapabilityQueryPolicy) -> ir::CapabilityQueryPolicy {
    match policy {
        hir::CapabilityQueryPolicy::QuerySafe => ir::CapabilityQueryPolicy::QuerySafe,
        hir::CapabilityQueryPolicy::TxOnly => ir::CapabilityQueryPolicy::TxOnly,
    }
}

pub(crate) fn lower_proof_visibility(
    visibility: hir::CapabilityProofVisibility,
) -> ir::CapabilityProofVisibility {
    match visibility {
        hir::CapabilityProofVisibility::Journaled => ir::CapabilityProofVisibility::Journaled,
        hir::CapabilityProofVisibility::OpaqueRuntimeOnly => {
            ir::CapabilityProofVisibility::OpaqueRuntimeOnly
        }
    }
}

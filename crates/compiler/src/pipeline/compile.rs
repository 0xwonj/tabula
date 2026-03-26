use tabula_ir as ir;
use tabula_lang::hir::{
    CapabilityProofVisibility, CapabilityQueryPolicy, CapabilityTotality, HashFamily,
};
use tabula_lang::{CapabilityPreludeEntry, FrontendPrelude, build_hir, parse_program, verify_hir};

use crate::error::{CompileStage, CompilerError, CompilerResult};
use crate::pipeline::diagnostics::{
    compile_error, diagnostic_from_compiler_error, frontend_diagnostic, spanless_diagnostic,
    tabula_diagnostic,
};
use crate::pipeline::fingerprint::derive_program_id;
use crate::pipeline::types::CompiledProgram;
use crate::registration::derive_field_schemes;
use crate::{CompilerCatalogs, SourceCapabilityDescriptor, hir_lower, mir};

/// Compile rewritten source through the HIR/MIR/canonical pipeline.
pub fn compile_program_source(source: &str) -> CompilerResult<CompiledProgram> {
    compile_program_source_with_catalogs(source, &CompilerCatalogs::default())
}

/// Compile rewritten source through the HIR/MIR/canonical pipeline using explicit catalogs.
pub fn compile_program_source_with_catalogs(
    source: &str,
    catalogs: &CompilerCatalogs,
) -> CompilerResult<CompiledProgram> {
    let prelude = build_frontend_prelude(catalogs).map_err(|err| {
        compile_error(vec![spanless_diagnostic(
            CompileStage::FrontendSemantics,
            source,
            "InvalidProgram",
            err.to_string(),
        )])
    })?;
    let ast = parse_program(source).map_err(|err| {
        compile_error(vec![frontend_diagnostic(
            source,
            CompileStage::FrontendParse,
            err,
        )])
    })?;
    let hir = build_hir(ast, &prelude).map_err(|err| {
        compile_error(vec![frontend_diagnostic(
            source,
            CompileStage::FrontendBuild,
            err,
        )])
    })?;
    let verified_hir = verify_hir(hir, &prelude).map_err(|err| {
        compile_error(vec![frontend_diagnostic(
            source,
            CompileStage::FrontendVerify,
            err,
        )])
    })?;
    let field_schemes = derive_field_schemes(&verified_hir);
    let program_id = derive_program_id(&verified_hir);
    let mir = hir_lower::lower_hir_to_mir(&verified_hir, program_id)
        .map_err(|err| diagnostic_from_compiler_error(source, CompileStage::HirLower, err))?;
    let verified_mir = mir::verify_program(mir).map_err(|err| {
        compile_error(vec![tabula_diagnostic(
            CompileStage::MirVerify,
            source,
            &err,
        )])
    })?;
    let analyzed = mir::analyze_program(verified_mir).map_err(|err| {
        compile_error(vec![tabula_diagnostic(
            CompileStage::MirAnalyze,
            source,
            &err,
        )])
    })?;
    let normalized = mir::inline_functions(&analyzed).map_err(|err| {
        compile_error(vec![tabula_diagnostic(
            CompileStage::MirNormalize,
            source,
            &err,
        )])
    })?;
    let canonicalized = mir::canonicalize_program(&normalized).map_err(|err| {
        compile_error(vec![tabula_diagnostic(
            CompileStage::MirNormalize,
            source,
            &err,
        )])
    })?;
    let analyzed = mir::analyze_program(canonicalized).map_err(|err| {
        compile_error(vec![tabula_diagnostic(
            CompileStage::MirAnalyze,
            source,
            &err,
        )])
    })?;
    let canonical = mir::lower_to_canonical(&analyzed).map_err(|err| {
        compile_error(vec![tabula_diagnostic(
            CompileStage::MirLower,
            source,
            &err,
        )])
    })?;
    let validated = ir::ValidatedProgram::try_from(canonical).map_err(|err| {
        compile_error(vec![tabula_diagnostic(
            CompileStage::CanonicalValidate,
            source,
            &err,
        )])
    })?;
    Ok(CompiledProgram {
        validated,
        field_schemes,
    })
}

fn build_frontend_prelude(catalogs: &CompilerCatalogs) -> Result<FrontendPrelude, CompilerError> {
    let capabilities = catalogs
        .capability_descriptors()
        .values()
        .map(capability_descriptor_to_prelude)
        .collect::<Vec<_>>();
    FrontendPrelude::new(catalogs.semantics().clone(), capabilities)
        .map_err(|err| CompilerError::InvalidProgram(anyhow::anyhow!(err.to_string())))
}

fn capability_descriptor_to_prelude(
    descriptor: &SourceCapabilityDescriptor,
) -> CapabilityPreludeEntry {
    CapabilityPreludeEntry {
        path: descriptor.path.clone(),
        inputs: descriptor.inputs.clone(),
        outputs: descriptor.outputs.clone(),
        totality: match descriptor.totality {
            ir::CapabilityTotality::Total => CapabilityTotality::Total,
            ir::CapabilityTotality::Checked => CapabilityTotality::Checked,
        },
        query_policy: match descriptor.query_policy {
            ir::CapabilityQueryPolicy::QuerySafe => CapabilityQueryPolicy::QuerySafe,
            ir::CapabilityQueryPolicy::TxOnly => CapabilityQueryPolicy::TxOnly,
        },
        proof_visibility: match descriptor.proof_visibility {
            ir::CapabilityProofVisibility::Journaled => CapabilityProofVisibility::Journaled,
            ir::CapabilityProofVisibility::OpaqueRuntimeOnly => {
                CapabilityProofVisibility::OpaqueRuntimeOnly
            }
        },
        hash_family: descriptor.hash_family.map(|family| match family {
            ir::HashFamily::Poseidon => HashFamily::Poseidon,
        }),
    }
}

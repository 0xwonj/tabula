//! Compiler-owned semantic catalogs for the native pipeline.

use std::collections::BTreeMap;

use tabula_core::MachineCapabilities;
use tabula_ir as ir;
use tabula_profile::{
    ProfileCatalog, SemanticRegistry, builtin_semantic_registry, is_bytes32_type,
};

use crate::error::CompilerCatalogError;

/// Compiler-owned source capability descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCapabilityDescriptor {
    /// Import path used by the source `use capability` declaration.
    pub path: String,
    /// Typed input contract.
    pub inputs: Vec<ir::TypeRef>,
    /// Typed output contract.
    pub outputs: Vec<ir::TypeRef>,
    /// Capability totality contract.
    pub totality: ir::CapabilityTotality,
    /// Query-policy contract.
    pub query_policy: ir::CapabilityQueryPolicy,
    /// Proof-visibility contract.
    pub proof_visibility: ir::CapabilityProofVisibility,
    /// Blessed builtin hash family classification, when applicable.
    pub hash_family: Option<ir::HashFamily>,
}

/// Source-registration catalog for source capability descriptors.
pub type SourceCapabilityCatalog = BTreeMap<String, SourceCapabilityDescriptor>;

/// Compiler-owned semantic catalogs used during native sealing.
#[derive(Debug, Clone)]
pub struct CompilerCatalogs {
    semantics: SemanticRegistry,
    machine_capabilities: MachineCapabilities,
    capability_descriptors: SourceCapabilityCatalog,
}

impl CompilerCatalogs {
    /// Build compiler catalogs seeded with the built-in semantic registry.
    pub fn standard() -> Result<Self, CompilerCatalogError> {
        Ok(Self {
            semantics: builtin_semantic_registry()
                .map_err(CompilerCatalogError::InvalidSemanticRegistry)?,
            machine_capabilities: MachineCapabilities::standard(),
            capability_descriptors: SourceCapabilityCatalog::new(),
        })
    }

    /// Build compiler catalogs without any seeded semantic definitions.
    pub fn empty() -> Self {
        Self {
            semantics: SemanticRegistry::new(),
            machine_capabilities: MachineCapabilities::standard(),
            capability_descriptors: SourceCapabilityCatalog::new(),
        }
    }

    /// Borrow the semantic registry used during sealing.
    pub fn semantics(&self) -> &SemanticRegistry {
        &self.semantics
    }

    /// Borrow the registered source capability descriptors.
    pub fn capability_descriptors(&self) -> &SourceCapabilityCatalog {
        &self.capability_descriptors
    }

    /// Borrow the native machine capability ceiling used during registration.
    pub fn machine_capabilities(&self) -> MachineCapabilities {
        self.machine_capabilities
    }

    /// Replace the semantic registry used during sealing.
    pub fn with_semantic_registry(
        mut self,
        semantics: SemanticRegistry,
    ) -> Result<Self, CompilerCatalogError> {
        semantics
            .validate()
            .map_err(CompilerCatalogError::InvalidSemanticRegistry)?;
        validate_capability_descriptor_catalog(&self.capability_descriptors, semantics.catalog())
            .map_err(|detail| CompilerCatalogError::InvalidCapabilityDescriptor { detail })?;
        self.semantics = semantics;
        Ok(self)
    }

    /// Replace the machine capability ceiling used during registration.
    pub fn with_machine_capabilities(mut self, machine_capabilities: MachineCapabilities) -> Self {
        self.machine_capabilities = machine_capabilities;
        self
    }

    /// Register one source capability descriptor available during source compilation.
    pub fn with_capability_descriptor(
        mut self,
        descriptor: SourceCapabilityDescriptor,
    ) -> Result<Self, CompilerCatalogError> {
        self.insert_capability_descriptor(descriptor)?;
        Ok(self)
    }

    /// Insert one source capability descriptor available during source compilation.
    pub fn insert_capability_descriptor(
        &mut self,
        descriptor: SourceCapabilityDescriptor,
    ) -> Result<(), CompilerCatalogError> {
        if self.capability_descriptors.contains_key(&descriptor.path) {
            return Err(CompilerCatalogError::DuplicateCapabilityDescriptor {
                path: descriptor.path,
            });
        }
        validate_capability_descriptor(&descriptor, self.semantics.catalog())
            .map_err(|detail| CompilerCatalogError::InvalidCapabilityDescriptor { detail })?;
        self.capability_descriptors
            .insert(descriptor.path.clone(), descriptor);
        Ok(())
    }
}

fn validate_capability_descriptor_catalog(
    descriptors: &SourceCapabilityCatalog,
    catalog: &ProfileCatalog,
) -> Result<(), String> {
    for descriptor in descriptors.values() {
        validate_capability_descriptor(descriptor, catalog)?;
    }
    Ok(())
}

fn validate_capability_descriptor(
    descriptor: &SourceCapabilityDescriptor,
    catalog: &ProfileCatalog,
) -> Result<(), String> {
    if descriptor.path.is_empty() {
        return Err("capability descriptor path must not be empty".into());
    }
    for (kind, tys) in [
        ("input", &descriptor.inputs),
        ("output", &descriptor.outputs),
    ] {
        for (idx, ty) in tys.iter().enumerate() {
            catalog.type_descriptor(*ty).map_err(|err| {
                format!(
                    "capability descriptor {} {kind} {} references unknown type id {}: {err}",
                    descriptor.path, idx, ty.0
                )
            })?;
        }
    }
    if let Some(hash_family) = descriptor.hash_family {
        if descriptor.outputs.len() != 1 || !is_bytes32_type(descriptor.outputs[0]) {
            return Err(format!(
                "blessed builtin hash capability {} {:?} must return [bytes32]",
                descriptor.path, hash_family
            ));
        }
        if descriptor.totality != ir::CapabilityTotality::Total {
            return Err(format!(
                "blessed builtin hash capability {} must be total",
                descriptor.path
            ));
        }
        if descriptor.query_policy != ir::CapabilityQueryPolicy::QuerySafe {
            return Err(format!(
                "blessed builtin hash capability {} must be query-safe",
                descriptor.path
            ));
        }
        if descriptor.proof_visibility != ir::CapabilityProofVisibility::OpaqueRuntimeOnly {
            return Err(format!(
                "blessed builtin hash capability {} must be runtime-opaque",
                descriptor.path
            ));
        }
    }
    Ok(())
}

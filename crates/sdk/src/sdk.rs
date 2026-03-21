use std::sync::Arc;

use tabula_artifact::Artifact;
#[cfg(feature = "prove")]
use tabula_compiler::SealedProgram;
use tabula_compiler::{
    CompilerCatalogs, PrecompileDescriptorCatalog, ProgramDefinition, SchemeDescriptorCatalog,
    compile_program_source, register_artifact, register_program_definition_with_catalogs,
};

#[cfg(feature = "verify")]
use std::collections::BTreeMap;

#[cfg(feature = "verify")]
use tabula_core::SchemeId;

#[cfg(feature = "verify")]
use tabula_ext::{PrecompileBundle, SchemeBundle};
#[cfg(feature = "verify")]
use tabula_ir::PrecompileId;

use crate::error::SdkError;
use crate::program::Program;
#[cfg(feature = "verify")]
use crate::verifier::Verifier;

/// Shared process-level SDK context.
#[derive(Clone)]
pub struct Sdk {
    pub(crate) inner: Arc<SdkInner>,
}

/// Fluent builder for [`Sdk`].
pub struct SdkBuilder {
    #[cfg(feature = "verify")]
    schemes: BTreeMap<SchemeId, SchemeBundle>,
    #[cfg(feature = "verify")]
    precompiles: BTreeMap<PrecompileId, PrecompileBundle>,
}

pub(crate) struct SdkInner {
    pub(crate) catalogs: CompilerCatalogs,
    #[cfg(feature = "verify")]
    pub(crate) schemes: BTreeMap<SchemeId, SchemeBundle>,
    #[cfg(feature = "verify")]
    pub(crate) precompiles: BTreeMap<PrecompileId, PrecompileBundle>,
}

impl Sdk {
    /// Build a standard SDK with built-in schemes only.
    pub fn standard() -> Self {
        Self::builder().build()
    }

    /// Start a customized SDK builder.
    pub fn builder() -> SdkBuilder {
        SdkBuilder::new()
    }

    /// Compile `.tab` source into a reusable SDK program.
    pub fn compile(&self, source: &str) -> Result<Program, SdkError> {
        let definition = compile_program_source(source)?;
        self.register(&definition)
    }

    /// Register an already-compiled program definition into a reusable SDK program.
    pub fn register(&self, definition: &ProgramDefinition) -> Result<Program, SdkError> {
        let compiled = register_program_definition_with_catalogs(definition, &self.inner.catalogs)?;
        Program::from_compiled(self.clone(), compiled)
    }

    /// Open a sealed artifact and validate it eagerly.
    pub fn open(&self, artifact: Artifact) -> Result<Program, SdkError> {
        let compiled = register_artifact(&artifact)?;
        Program::from_artifact(self.clone(), compiled, artifact)
    }

    /// Create an artifact-bound verifier that reuses this SDK context.
    #[cfg(feature = "verify")]
    pub fn verifier(&self, artifact: Artifact) -> Result<Verifier, SdkError> {
        let _compiled = register_artifact(&artifact)?;
        Ok(Verifier::new(self.clone(), artifact))
    }

    #[cfg(feature = "prove")]
    pub(crate) fn build_runtime(
        &self,
        sealed_program: &SealedProgram,
    ) -> Result<tabula_runtime::TabulaRuntime, SdkError> {
        let mut builder = tabula_runtime::TabulaRuntime::builder(sealed_program.clone());

        #[cfg(feature = "verify")]
        for bundle in self.inner.schemes.values() {
            builder = builder.with_scheme_bundle(bundle.clone())?;
        }

        #[cfg(feature = "verify")]
        for bundle in self.inner.precompiles.values() {
            builder = builder.with_precompile(bundle.clone())?;
        }

        builder.build().map_err(SdkError::from)
    }

    #[cfg(feature = "verify")]
    pub(crate) fn build_verifier(
        &self,
        artifact: &Artifact,
    ) -> Result<tabula_runtime::Verifier, SdkError> {
        let mut builder = tabula_runtime::Verifier::builder(artifact.clone());

        for bundle in self.inner.schemes.values() {
            builder = builder.with_scheme_bundle(bundle.clone())?;
        }

        for bundle in self.inner.precompiles.values() {
            builder = builder.with_precompile(bundle.clone())?;
        }

        builder.build().map_err(SdkError::from)
    }
}

impl SdkBuilder {
    fn new() -> Self {
        Self {
            #[cfg(feature = "verify")]
            schemes: BTreeMap::new(),
            #[cfg(feature = "verify")]
            precompiles: BTreeMap::new(),
        }
    }

    /// Register one custom scheme bundle.
    #[cfg(feature = "verify")]
    pub fn with_scheme(mut self, bundle: SchemeBundle) -> Result<Self, SdkError> {
        let scheme_id = bundle.scheme_id();
        if is_builtin_scheme_id(scheme_id) || self.schemes.contains_key(&scheme_id) {
            return Err(SdkError::InvalidSchemeBundle {
                detail: format!(
                    "duplicate scheme bundle registration for id {}",
                    scheme_id.0
                ),
            });
        }
        self.schemes.insert(scheme_id, bundle);
        Ok(self)
    }

    /// Register one custom precompile bundle.
    #[cfg(feature = "verify")]
    pub fn with_precompile(mut self, bundle: PrecompileBundle) -> Result<Self, SdkError> {
        let id = bundle.id();
        if self.precompiles.insert(id, bundle).is_some() {
            return Err(SdkError::InvalidPrecompileBundle {
                detail: format!(
                    "duplicate precompile bundle registration for id 0x{:04x}",
                    id.0
                ),
            });
        }
        Ok(self)
    }

    /// Finalize the SDK configuration.
    pub fn build(self) -> Sdk {
        #[cfg(feature = "verify")]
        let (scheme_catalog, precompile_catalog) = {
            let mut catalog = SchemeDescriptorCatalog::new();
            for bundle in self.schemes.values() {
                catalog.insert(bundle.scheme_id(), bundle.descriptor().clone());
            }
            let mut precompile_catalog = PrecompileDescriptorCatalog::new();
            for bundle in self.precompiles.values() {
                precompile_catalog.insert(bundle.id(), bundle.descriptor().clone());
            }
            (catalog, precompile_catalog)
        };

        #[cfg(not(feature = "verify"))]
        let (scheme_catalog, precompile_catalog) = (
            SchemeDescriptorCatalog::new(),
            PrecompileDescriptorCatalog::new(),
        );

        Sdk {
            inner: Arc::new(SdkInner {
                catalogs: CompilerCatalogs {
                    schemes: scheme_catalog,
                    precompiles: precompile_catalog,
                },
                #[cfg(feature = "verify")]
                schemes: self.schemes,
                #[cfg(feature = "verify")]
                precompiles: self.precompiles,
            }),
        }
    }
}

impl Default for SdkBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for SdkBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("SdkBuilder");
        #[cfg(feature = "verify")]
        debug
            .field("scheme_count", &self.schemes.len())
            .field("precompile_count", &self.precompiles.len());
        debug.finish_non_exhaustive()
    }
}

#[cfg(feature = "verify")]
fn is_builtin_scheme_id(id: SchemeId) -> bool {
    matches!(id, SchemeId::SSMC | SchemeId::SMT)
}

impl std::fmt::Debug for Sdk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sdk").finish_non_exhaustive()
    }
}

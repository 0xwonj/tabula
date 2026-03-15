//! Fluent builder for [`TabulaRuntime`] construction.
//!
//! Unifies executor-side and machine-side registration into a single API.
//! Precompiles are registered once and wired to both the executor's
//! [`PrecompileRegistry`] and the machine's [`MachineBuilder`].
//!
//! ```ignore
//! let runtime = TabulaRuntime::builder(compiled_program)
//!     .with_precompile(id, handler, verifier_ext)
//!     .build()?;
//! ```

use std::collections::{BTreeMap, BTreeSet};

use tabula_artifact::CompiledProgram;
use tabula_commitment::scheme_tags;
use tabula_core::{TableId, TableSchema};
use tabula_executor::precompile::{PrecompileHandler, PrecompileRegistry};
use tabula_executor::property::{PropertyOpeningRegistry, PropertyOpeningResolver};
use tabula_ir::PrecompileId;
use tabula_machine::{
    ChipExtension, ColumnScheme, ColumnSetupConfig, MachineBuilder, PropertyOpening, RootProof,
    TabulaStarkConfig,
};

use crate::error::RuntimeError;
use crate::runtime::TabulaRuntime;

/// Fluent builder for [`TabulaRuntime`].
///
/// Collects program, schemas, and extension registrations, then builds
/// a `TabulaRuntime` that owns a `TabulaMachine` (built once, reused).
///
/// # Required inputs
///
/// - `compiled_program` — compiler-produced semantic artifact
///
/// # Optional registrations
///
/// - `with_precompile()` — registers both executor handler and machine verifier
/// - `with_property_read()` — registers both executor resolver and machine opening
/// - `with_extension()` — machine-only chip extension
/// - `with_column_scheme()` — custom commitment scheme
/// - `with_root_proof()` — custom root proof scheme
/// - `with_config()` — custom STARK configuration
pub struct RuntimeBuilder {
    compiled_program: CompiledProgram,
    machine_builder: MachineBuilder,
    precompile_handlers: Vec<Box<dyn PrecompileHandler>>,
    property_resolver: Option<Box<dyn PropertyOpeningResolver>>,
}

impl RuntimeBuilder {
    /// Create a builder with a compiler-produced program artifact.
    pub(crate) fn new(compiled_program: CompiledProgram) -> Self {
        Self {
            compiled_program,
            machine_builder: MachineBuilder::new(),
            precompile_handlers: Vec::new(),
            property_resolver: None,
        }
    }

    /// Register a precompile with unified dual-registration.
    ///
    /// Registers the executor-side `handler` (for execution) and the
    /// machine-side `verifier` (for proving) under the same `id`.
    /// This ensures both sides stay in sync.
    pub fn with_precompile(
        mut self,
        id: PrecompileId,
        handler: impl PrecompileHandler + 'static,
        verifier: impl ChipExtension + 'static,
    ) -> Self {
        self.precompile_handlers.push(Box::new(handler));
        self.machine_builder = self.machine_builder.with_precompile(id, verifier);
        self
    }

    /// Register a machine-only chip extension.
    ///
    /// For extensions that don't need an executor-side handler
    /// (e.g., custom gadgets that only add AIR constraints).
    pub fn with_extension(mut self, ext: impl ChipExtension + 'static) -> Self {
        self.machine_builder = self.machine_builder.with_extension(ext);
        self
    }

    /// Register a custom column commitment scheme.
    ///
    /// Maps a `scheme_tag` to a [`ColumnScheme`] implementation.
    /// The default SSMC scheme (tag 0) is always pre-registered.
    pub fn with_column_scheme(
        mut self,
        scheme_tag: u16,
        scheme: impl ColumnScheme + 'static,
    ) -> Self {
        self.machine_builder = self.machine_builder.with_column_scheme(scheme_tag, scheme);
        self
    }

    /// Register a property read with unified dual-registration.
    ///
    /// Registers the executor-side `resolver` (for execution, zero crypto) and
    /// the machine-side `opening` (for proof generation) together. This ensures
    /// both sides stay in sync — the resolver produces concrete values, and
    /// the opening produces ZK witnesses for the same queries.
    ///
    /// Only one resolver can be registered (it handles all property queries).
    /// Multiple openings can be registered for different scheme tags.
    pub fn with_property_read(
        mut self,
        resolver: impl PropertyOpeningResolver + 'static,
        opening: impl PropertyOpening + 'static,
    ) -> Self {
        self.property_resolver = Some(Box::new(resolver));
        self.machine_builder = self.machine_builder.with_property_opening(opening);
        self
    }

    /// Register a machine-only property opening (no executor-side resolver).
    ///
    /// Use this when the executor doesn't need to resolve property queries
    /// (e.g., the opening is only used for verification in the machine).
    /// For full support, prefer [`with_property_read()`](Self::with_property_read).
    pub fn with_property_opening(mut self, opening: impl PropertyOpening + 'static) -> Self {
        self.machine_builder = self.machine_builder.with_property_opening(opening);
        self
    }

    /// Override the root proof scheme (default: two-level SMT).
    pub fn with_root_proof(mut self, root: impl RootProof + 'static) -> Self {
        self.machine_builder = self.machine_builder.with_root_proof(root);
        self
    }

    /// Override the STARK configuration.
    pub fn with_config(mut self, config: TabulaStarkConfig) -> Self {
        self.machine_builder = self.machine_builder.with_config(config);
        self
    }

    /// Build the runtime, creating the machine and precompile registry.
    ///
    /// Validates program-schema compatibility and precompile registration
    /// before building the machine. Derives column configs from schemas
    /// (all columns use SSMC by default).
    pub fn build(self) -> Result<TabulaRuntime, RuntimeError> {
        self.validate()?;

        let col_configs = derive_column_configs(&self.compiled_program.table_schemas);
        let machine = self
            .machine_builder
            .with_columns(col_configs)
            .build()
            .map_err(RuntimeError::MachineSetup)?;

        let precompiles = build_precompile_registry(self.precompile_handlers);
        let property_openings = self.property_resolver.map(PropertyOpeningRegistry::new);
        let schemas_by_id = index_schemas(&self.compiled_program.table_schemas);

        Ok(TabulaRuntime::from_parts(
            self.compiled_program,
            schemas_by_id,
            machine,
            precompiles,
            property_openings,
        ))
    }

    // ── Validation ───────────────────────────────────────────────────────

    fn validate(&self) -> Result<(), RuntimeError> {
        self.validate_schemas()?;
        self.validate_table_references()?;
        self.validate_precompile_references()?;
        Ok(())
    }

    /// Verify that no schema has an empty column set.
    fn validate_schemas(&self) -> Result<(), RuntimeError> {
        for schema in &self.compiled_program.table_schemas {
            if schema.columns.is_empty() {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "table '{}' (id={}) has no columns",
                        schema.name, schema.id.0,
                    ),
                });
            }
        }
        Ok(())
    }

    /// Verify that every table ID referenced by Read/Write/PropertyRead
    /// instructions exists in the provided schemas.
    fn validate_table_references(&self) -> Result<(), RuntimeError> {
        let schema_ids: BTreeSet<TableId> = self
            .compiled_program
            .table_schemas
            .iter()
            .map(|s| s.id)
            .collect();
        for id in self.compiled_program.program.referenced_table_ids() {
            if !schema_ids.contains(&id) {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!("program references table {} not found in schemas", id.0,),
                });
            }
        }
        Ok(())
    }

    /// Verify that every precompile ID referenced by Precompile instructions
    /// has a registered handler.
    fn validate_precompile_references(&self) -> Result<(), RuntimeError> {
        let registered: BTreeSet<PrecompileId> =
            self.precompile_handlers.iter().map(|h| h.id()).collect();
        for id in self.compiled_program.program.referenced_precompile_ids() {
            if !registered.contains(&id) {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "program references precompile 0x{:04x} but no handler is registered",
                        id.0,
                    ),
                });
            }
        }
        Ok(())
    }
}

/// Derive `ColumnSetupConfig` from schemas.
///
/// All columns default to SSMC scheme with commitment reception enabled.
/// Custom schemes override this via `with_column_scheme()` on the machine builder.
fn derive_column_configs(schemas: &[TableSchema]) -> Vec<ColumnSetupConfig> {
    schemas
        .iter()
        .flat_map(|schema| {
            schema.columns.iter().map(move |col_def| ColumnSetupConfig {
                table_id: schema.id,
                col_id: col_def.id,
                scheme_tag: scheme_tags::SSMC,
                receives_commitment: true,
            })
        })
        .collect()
}

fn build_precompile_registry(handlers: Vec<Box<dyn PrecompileHandler>>) -> PrecompileRegistry {
    let mut registry = PrecompileRegistry::new();
    for handler in handlers {
        registry.register_boxed(handler);
    }
    registry
}

fn index_schemas(schemas: &[TableSchema]) -> BTreeMap<TableId, TableSchema> {
    schemas.iter().cloned().map(|s| (s.id, s)).collect()
}

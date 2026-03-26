use std::collections::BTreeMap;

use tabula_profile::{SemanticRegistry, TypeCapabilities};

use super::consts::ensure_capability;
use crate::ast;
use crate::error::{FrontendError, FrontendErrorKind};
use crate::hir;
use crate::span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TypeCapabilityKind {
    Arithmetic,
    Equality,
    Ordering,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityPreludeEntry {
    pub path: String,
    pub inputs: Vec<hir::TypeRef>,
    pub outputs: Vec<hir::TypeRef>,
    pub totality: hir::CapabilityTotality,
    pub query_policy: hir::CapabilityQueryPolicy,
    pub proof_visibility: hir::CapabilityProofVisibility,
    pub hash_family: Option<hir::HashFamily>,
}

#[derive(Debug, Clone)]
pub struct FrontendPrelude {
    registry: SemanticRegistry,
    capabilities: BTreeMap<String, CapabilityPreludeEntry>,
}

impl FrontendPrelude {
    pub fn new(
        registry: SemanticRegistry,
        capabilities: Vec<CapabilityPreludeEntry>,
    ) -> Result<Self, FrontendError> {
        let mut capability_map = BTreeMap::new();
        for capability in capabilities {
            if capability_map
                .insert(capability.path.clone(), capability)
                .is_some()
            {
                return Err(FrontendError::new(
                    FrontendErrorKind::DuplicateSymbol,
                    Span::new(0, 0),
                    "duplicate capability prelude entry",
                ));
            }
        }
        let prelude = Self {
            registry,
            capabilities: capability_map,
        };
        prelude.validate_surface_baseline()?;
        Ok(prelude)
    }

    pub fn builtin() -> Self {
        Self::new(
            tabula_profile::builtin_semantic_registry().expect("built-in semantic registry"),
            vec![],
        )
        .expect("built-in frontend prelude")
    }

    pub(super) fn resolve_type(
        &self,
        name: &str,
        span: Span,
    ) -> Result<hir::TypeRef, FrontendError> {
        self.registry.resolve_type_name(name).map_err(|source| {
            FrontendError::new(
                FrontendErrorKind::TypeResolution,
                span,
                format!("unknown type {name}: {source}"),
            )
        })
    }

    pub(super) fn resolve_scheme(
        &self,
        name: &str,
        span: Span,
    ) -> Result<(tabula_core::SchemeId, String), FrontendError> {
        self.registry
            .resolve_scheme_name(name)
            .map(|id| (id, name.to_string()))
            .map_err(|source| {
                FrontendError::new(
                    FrontendErrorKind::TypeResolution,
                    span,
                    format!("unknown scheme {name}: {source}"),
                )
            })
    }

    fn validate_surface_baseline(&self) -> Result<(), FrontendError> {
        let span = Span::new(0, 0);
        for name in ["bool", "u64", "i64", "bytes32"] {
            self.resolve_type(name, span).map(|_| ())?;
        }
        for scheme in ["ssmc", "smt"] {
            self.resolve_scheme(scheme, span).map(|_| ())?;
        }
        Ok(())
    }

    pub(super) fn resolve_capability(
        &self,
        path: &ast::IdentPath,
        span: Span,
    ) -> Result<&CapabilityPreludeEntry, FrontendError> {
        let key = path.as_string();
        self.capabilities.get(&key).ok_or_else(|| {
            FrontendError::new(
                FrontendErrorKind::UndefinedSymbol,
                span,
                format!("unknown capability import {}", path.as_string()),
            )
        })
    }

    fn type_capabilities(
        &self,
        ty: hir::TypeRef,
        span: Span,
    ) -> Result<TypeCapabilities, FrontendError> {
        self.registry
            .catalog()
            .type_descriptor(ty)
            .map(|descriptor| descriptor.capabilities)
            .map_err(|source| {
                FrontendError::new(
                    FrontendErrorKind::TypeResolution,
                    span,
                    format!("missing type descriptor for {}: {source}", ty.0),
                )
            })
    }

    pub(crate) fn require_type_capability(
        &self,
        ty: hir::TypeRef,
        capability: TypeCapabilityKind,
        span: Span,
        message: &'static str,
    ) -> Result<(), FrontendError> {
        let capabilities = self.type_capabilities(ty, span)?;
        let ok = match capability {
            TypeCapabilityKind::Arithmetic => capabilities.arithmetic,
            TypeCapabilityKind::Equality => capabilities.equality,
            TypeCapabilityKind::Ordering => capabilities.ordering,
        };
        ensure_capability(ok, span, message)
    }

    pub(crate) fn validate_field_scheme(
        &self,
        ty: hir::TypeRef,
        scheme: &hir::SchemeRef,
        span: Span,
    ) -> Result<(), FrontendError> {
        let (resolved_id, _) = self.resolve_scheme(&scheme.symbol, span)?;
        if resolved_id != scheme.id {
            return Err(FrontendError::new(
                FrontendErrorKind::TypeMismatch,
                span,
                format!(
                    "field scheme {} resolved to {}, not {}",
                    scheme.symbol, resolved_id.0, scheme.id.0
                ),
            ));
        }
        let encoding_profile_id = self
            .registry
            .resolve_default_encoding(ty)
            .map_err(|source| {
                FrontendError::new(
                    FrontendErrorKind::TypeMismatch,
                    span,
                    format!("type {} has no default encoding profile: {source}", ty.0),
                )
            })?;
        self.registry
            .resolve_default_scheme_profile(scheme.id, encoding_profile_id)
            .map_err(|source| {
                FrontendError::new(
                    FrontendErrorKind::TypeMismatch,
                    span,
                    format!(
                        "scheme {} is not admissible for type {}: {source}",
                        scheme.symbol, ty.0
                    ),
                )
            })?;
        Ok(())
    }
}

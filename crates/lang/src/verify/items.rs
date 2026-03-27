#![allow(missing_docs)]

#[allow(clippy::wildcard_imports)]
use super::*;
use tabula_profile::is_bytes32_type;

impl<'a> VerifyCx<'a> {
    pub(super) fn verify_top_level_symbols(&mut self) -> Result<(), FrontendError> {
        let mut use_ids = BTreeSet::new();
        for use_decl in &self.program.uses {
            if !use_ids.insert(use_decl.capability.id) {
                return Err(FrontendError::new(
                    FrontendErrorKind::DuplicateSymbol,
                    use_decl.span,
                    format!("duplicate capability id {}", use_decl.capability.id.0),
                ));
            }
            if !self
                .top_level_symbols
                .insert(use_decl.capability.symbol.clone())
            {
                return Err(FrontendError::new(
                    FrontendErrorKind::DuplicateSymbol,
                    use_decl.span,
                    format!("duplicate top-level symbol {}", use_decl.capability.symbol),
                ));
            }
            if use_decl.capability.hash_family.is_some() {
                if use_decl.capability.outputs.len() != 1
                    || !is_bytes32_type(use_decl.capability.outputs[0])
                {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        use_decl.span,
                        "blessed hash capability must return bytes32",
                    ));
                }
                if use_decl.capability.totality != CapabilityTotality::Total {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        use_decl.span,
                        "blessed hash capability must be total",
                    ));
                }
                if use_decl.capability.query_policy != CapabilityQueryPolicy::QuerySafe {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        use_decl.span,
                        "blessed hash capability must be query-safe",
                    ));
                }
                if use_decl.capability.proof_visibility
                    != CapabilityProofVisibility::OpaqueRuntimeOnly
                {
                    return Err(FrontendError::new(
                        FrontendErrorKind::TypeMismatch,
                        use_decl.span,
                        "blessed hash capability must be runtime-opaque",
                    ));
                }
            }
        }

        if let Some(context) = &self.program.context {
            let mut context_field_ids = BTreeSet::new();
            let mut context_field_symbols = BTreeSet::new();
            for field in &context.fields {
                if !context_field_ids.insert(field.id) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        field.span,
                        format!("duplicate context field id {}", field.id.0),
                    ));
                }
                if !context_field_symbols.insert(field.symbol.clone()) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        field.span,
                        format!("duplicate context field {}", field.symbol),
                    ));
                }
            }
        }

        if let Some(state) = &self.program.state {
            let mut table_ids = BTreeSet::new();
            for table in &state.tables {
                if !table_ids.insert(table.id) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        table.span,
                        format!("duplicate table id {}", table.id.0),
                    ));
                }
                if !self.top_level_symbols.insert(table.symbol.clone()) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        table.span,
                        format!("duplicate top-level symbol {}", table.symbol),
                    ));
                }
            }
        }

        let mut const_ids = BTreeSet::new();
        let mut relation_ids = BTreeSet::new();
        let mut event_ids = BTreeSet::new();
        let mut callable_ids = BTreeSet::new();
        for item in &self.program.items {
            match item {
                Item::Const(decl) => {
                    if !const_ids.insert(decl.id) {
                        return Err(FrontendError::new(
                            FrontendErrorKind::DuplicateSymbol,
                            decl.span,
                            format!("duplicate const id {}", decl.id.0),
                        ));
                    }
                    if !self.top_level_symbols.insert(decl.symbol.clone()) {
                        return Err(FrontendError::new(
                            FrontendErrorKind::DuplicateSymbol,
                            decl.span,
                            format!("duplicate top-level symbol {}", decl.symbol),
                        ));
                    }
                }
                Item::Relation(decl) => {
                    if !relation_ids.insert(decl.id) {
                        return Err(FrontendError::new(
                            FrontendErrorKind::DuplicateSymbol,
                            decl.span,
                            format!("duplicate relation id {}", decl.id.0),
                        ));
                    }
                    if !self.top_level_symbols.insert(decl.symbol.clone()) {
                        return Err(FrontendError::new(
                            FrontendErrorKind::DuplicateSymbol,
                            decl.span,
                            format!("duplicate top-level symbol {}", decl.symbol),
                        ));
                    }
                }
                Item::Event(decl) => {
                    if !event_ids.insert(decl.id) {
                        return Err(FrontendError::new(
                            FrontendErrorKind::DuplicateSymbol,
                            decl.span,
                            format!("duplicate event id {}", decl.id.0),
                        ));
                    }
                    if !self.top_level_symbols.insert(decl.symbol.clone()) {
                        return Err(FrontendError::new(
                            FrontendErrorKind::DuplicateSymbol,
                            decl.span,
                            format!("duplicate top-level symbol {}", decl.symbol),
                        ));
                    }
                }
                Item::Callable(decl) => {
                    if !callable_ids.insert(decl.id) {
                        return Err(FrontendError::new(
                            FrontendErrorKind::DuplicateSymbol,
                            decl.span,
                            format!("duplicate callable id {}", decl.id.0),
                        ));
                    }
                    if !self.top_level_symbols.insert(decl.symbol.clone()) {
                        return Err(FrontendError::new(
                            FrontendErrorKind::DuplicateSymbol,
                            decl.span,
                            format!("duplicate top-level symbol {}", decl.symbol),
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    pub(super) fn verify_context(&self) -> Result<(), FrontendError> {
        let Some(context) = &self.program.context else {
            return Ok(());
        };
        let mut field_ids = BTreeSet::new();
        let mut field_symbols = BTreeSet::new();
        for field in &context.fields {
            if !field_ids.insert(field.id) {
                return Err(FrontendError::new(
                    FrontendErrorKind::DuplicateSymbol,
                    field.span,
                    format!("duplicate context field id {}", field.id.0),
                ));
            }
            if !field_symbols.insert(field.symbol.clone()) {
                return Err(FrontendError::new(
                    FrontendErrorKind::DuplicateSymbol,
                    field.span,
                    format!("duplicate context field {}", field.symbol),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn verify_state(&self) -> Result<(), FrontendError> {
        let Some(state) = &self.program.state else {
            return Ok(());
        };
        for table in &state.tables {
            let mut key_ids = BTreeSet::new();
            let mut key_symbols = BTreeSet::new();
            for key in &table.keys {
                if !key_ids.insert(key.id) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        key.span,
                        format!("duplicate state key id {}", key.id.0),
                    ));
                }
                if !key_symbols.insert(key.symbol.clone()) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        key.span,
                        format!("duplicate state key {}", key.symbol),
                    ));
                }
            }

            let mut field_ids = BTreeSet::new();
            let mut field_symbols = BTreeSet::new();
            for field in &table.fields {
                if !field_ids.insert(field.id) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        field.span,
                        format!("duplicate state field id {}", field.id.0),
                    ));
                }
                if !field_symbols.insert(field.symbol.clone()) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        field.span,
                        format!("duplicate state field {}", field.symbol),
                    ));
                }
                if let Some(scheme) = &field.scheme {
                    self.prelude
                        .validate_field_scheme(field.ty, scheme, field.span)?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn verify_consts(&self) -> Result<(), FrontendError> {
        for const_decl in self.consts.values() {
            let actual = self.verify_const_expr(&const_decl.value)?;
            ensure_type(
                actual,
                const_decl.ty,
                const_decl.span,
                "const value type mismatch",
            )?;
        }
        Ok(())
    }

    pub(super) fn verify_events(&self) -> Result<(), FrontendError> {
        for event in self.events.values() {
            let mut field_ids = BTreeSet::new();
            let mut field_symbols = BTreeSet::new();
            for field in &event.fields {
                if !field_ids.insert(field.id) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        field.span,
                        format!("duplicate event field id {}", field.id.0),
                    ));
                }
                if !field_symbols.insert(field.symbol.clone()) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        field.span,
                        format!("duplicate event field {}", field.symbol),
                    ));
                }
            }
        }
        Ok(())
    }
}

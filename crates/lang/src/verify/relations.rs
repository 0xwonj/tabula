#![allow(clippy::wildcard_imports)]
#![allow(missing_docs)]

use super::*;
use tabula_profile::{is_i64_type, is_u64_type};

impl<'a> VerifyCx<'a> {
    pub(super) fn verify_relations(&self) -> Result<(), FrontendError> {
        for relation in self.relations.values() {
            let mut param_ids = BTreeSet::new();
            let mut param_symbols = BTreeSet::new();
            for param in &relation.params {
                if !param_ids.insert(param.id) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        param.span,
                        format!("duplicate relation param id {}", param.id.0),
                    ));
                }
                if !param_symbols.insert(param.symbol.clone()) {
                    return Err(FrontendError::new(
                        FrontendErrorKind::DuplicateSymbol,
                        param.span,
                        format!("duplicate relation param {}", param.symbol),
                    ));
                }
            }
            let output_tys = relation
                .results
                .iter()
                .map(|result| result.ty)
                .collect::<Vec<_>>();
            let input_tys = relation
                .params
                .iter()
                .map(|param| param.ty)
                .collect::<Vec<_>>();
            match &relation.body {
                RelationBody::Enum { values } => {
                    if input_tys.len() != 1 || !output_tys.is_empty() {
                        return Err(FrontendError::new(
                            FrontendErrorKind::TypeMismatch,
                            relation.span,
                            "enum relations require exactly one input and no outputs",
                        ));
                    }
                    for value in values {
                        ensure_type(
                            self.verify_const_expr(value)?,
                            input_tys[0],
                            relation.span,
                            "enum relation literal type mismatch",
                        )?;
                    }
                }
                RelationBody::Range { start, end } => {
                    if input_tys.len() != 1 || !output_tys.is_empty() {
                        return Err(FrontendError::new(
                            FrontendErrorKind::TypeMismatch,
                            relation.span,
                            "range relations require exactly one input and no outputs",
                        ));
                    }
                    if !is_u64_type(input_tys[0]) && !is_i64_type(input_tys[0]) {
                        return Err(FrontendError::new(
                            FrontendErrorKind::TypeMismatch,
                            relation.span,
                            "range relations require u64 or i64 input type in V2",
                        ));
                    }
                    ensure_type(
                        self.verify_const_expr(start)?,
                        input_tys[0],
                        relation.span,
                        "range lower bound type mismatch",
                    )?;
                    ensure_type(
                        self.verify_const_expr(end)?,
                        input_tys[0],
                        relation.span,
                        "range upper bound type mismatch",
                    )?;
                }
                RelationBody::Map { entries } => {
                    let mut seen_inputs = BTreeSet::new();
                    for entry in entries {
                        if entry.inputs.len() != input_tys.len()
                            || entry.outputs.len() != output_tys.len()
                        {
                            return Err(FrontendError::new(
                                FrontendErrorKind::TypeMismatch,
                                relation.span,
                                "relation map entry arity mismatch",
                            ));
                        }
                        for (value, expected) in entry.inputs.iter().zip(&input_tys) {
                            ensure_type(
                                self.verify_const_expr(value)?,
                                *expected,
                                relation.span,
                                "relation map input type mismatch",
                            )?;
                        }
                        for (value, expected) in entry.outputs.iter().zip(&output_tys) {
                            ensure_type(
                                self.verify_const_expr(value)?,
                                *expected,
                                relation.span,
                                "relation map output type mismatch",
                            )?;
                        }
                        let input_fingerprint = entry
                            .inputs
                            .iter()
                            .map(value_to_fingerprint)
                            .collect::<Result<Vec<_>, _>>()?;
                        if !seen_inputs.insert(input_fingerprint) {
                            return Err(FrontendError::new(
                                FrontendErrorKind::DuplicateSymbol,
                                relation.span,
                                "relation map contains duplicate input tuple",
                            ));
                        }
                    }
                }
                RelationBody::Set { tuples } => {
                    if !output_tys.is_empty() {
                        return Err(FrontendError::new(
                            FrontendErrorKind::TypeMismatch,
                            relation.span,
                            "set relations require no outputs",
                        ));
                    }
                    for tuple in tuples {
                        if tuple.len() != input_tys.len() {
                            return Err(FrontendError::new(
                                FrontendErrorKind::TypeMismatch,
                                relation.span,
                                "relation set tuple arity mismatch",
                            ));
                        }
                        for (value, expected) in tuple.iter().zip(&input_tys) {
                            ensure_type(
                                self.verify_const_expr(value)?,
                                *expected,
                                relation.span,
                                "relation set input type mismatch",
                            )?;
                        }
                    }
                }
                RelationBody::Extern => {
                    return Err(FrontendError::new(
                        FrontendErrorKind::UnsupportedFeature,
                        relation.span,
                        "extern relations are intentionally deferred to a later phase",
                    ));
                }
            }
        }
        Ok(())
    }
}

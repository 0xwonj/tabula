use blake3::Hasher;
use std::collections::BTreeMap;

use tabula_core::PortableValue;
use tabula_ir as ir;
use tabula_lang::hir;

pub(crate) fn derive_program_id(program: &hir::VerifiedProgram) -> ir::ProgramId {
    let mut cx = FingerprintCx::new(program.program());
    cx.hash_program();
    let hash = cx.hasher.finalize();
    let bytes = hash.as_bytes();
    ir::ProgramId(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

struct FingerprintCx<'a> {
    program: &'a hir::Program,
    capability_paths: BTreeMap<hir::CapabilityRefId, &'a str>,
    context_field_symbols: BTreeMap<hir::ContextFieldId, &'a str>,
    table_symbols: BTreeMap<hir::TableId, &'a str>,
    field_symbols: BTreeMap<(hir::TableId, hir::FieldId), &'a str>,
    const_symbols: BTreeMap<hir::ConstId, &'a str>,
    relation_symbols: BTreeMap<hir::RelationId, &'a str>,
    event_symbols: BTreeMap<hir::EventId, &'a str>,
    callable_symbols: BTreeMap<hir::CallableId, &'a str>,
    hasher: Hasher,
}

impl<'a> FingerprintCx<'a> {
    fn new(program: &'a hir::Program) -> Self {
        let capability_paths = program
            .uses
            .iter()
            .map(|use_decl| (use_decl.capability.id, use_decl.capability.path.as_str()))
            .collect();
        let mut context_field_symbols = BTreeMap::new();
        if let Some(context) = &program.context {
            for field in &context.fields {
                context_field_symbols.insert(field.id, field.symbol.as_str());
            }
        }
        let mut table_symbols = BTreeMap::new();
        let mut field_symbols = BTreeMap::new();
        if let Some(state) = &program.state {
            for table in &state.tables {
                table_symbols.insert(table.id, table.symbol.as_str());
                for field in &table.fields {
                    field_symbols.insert((table.id, field.id), field.symbol.as_str());
                }
            }
        }
        let mut const_symbols = BTreeMap::new();
        let mut relation_symbols = BTreeMap::new();
        let mut event_symbols = BTreeMap::new();
        let mut callable_symbols = BTreeMap::new();
        for item in &program.items {
            match item {
                hir::Item::Const(decl) => {
                    const_symbols.insert(decl.id, decl.symbol.as_str());
                }
                hir::Item::Relation(decl) => {
                    relation_symbols.insert(decl.id, decl.symbol.as_str());
                }
                hir::Item::Event(decl) => {
                    event_symbols.insert(decl.id, decl.symbol.as_str());
                }
                hir::Item::Callable(decl) => {
                    callable_symbols.insert(decl.id, decl.symbol.as_str());
                }
            }
        }
        Self {
            program,
            capability_paths,
            context_field_symbols,
            table_symbols,
            field_symbols,
            const_symbols,
            relation_symbols,
            event_symbols,
            callable_symbols,
            hasher: Hasher::new(),
        }
    }

    fn hash_program(&mut self) {
        self.tag("program");
        self.string(&self.program.symbol);
        self.usize(self.program.uses.len());
        for use_decl in &self.program.uses {
            self.tag("use");
            self.string(&use_decl.capability.path);
            self.types(&use_decl.capability.inputs);
            self.types(&use_decl.capability.outputs);
            self.u8(match use_decl.capability.totality {
                hir::CapabilityTotality::Total => 0,
                hir::CapabilityTotality::Checked => 1,
            });
            self.u8(match use_decl.capability.query_policy {
                hir::CapabilityQueryPolicy::QuerySafe => 0,
                hir::CapabilityQueryPolicy::TxOnly => 1,
            });
            self.u8(match use_decl.capability.proof_visibility {
                hir::CapabilityProofVisibility::Journaled => 0,
                hir::CapabilityProofVisibility::OpaqueRuntimeOnly => 1,
            });
            self.u8(match use_decl.capability.hash_family {
                None => 0,
                Some(hir::HashFamily::Poseidon) => 1,
            });
        }
        match &self.program.context {
            Some(context) => {
                self.u8(1);
                self.usize(context.fields.len());
                for field in &context.fields {
                    self.tag("context-field");
                    self.string(&field.symbol);
                    self.ty(field.ty);
                }
            }
            None => self.u8(0),
        }
        match &self.program.state {
            Some(state) => {
                self.u8(1);
                self.usize(state.tables.len());
                for table in &state.tables {
                    self.tag("table");
                    self.string(&table.symbol);
                    self.usize(table.keys.len());
                    for key in &table.keys {
                        self.string(&key.symbol);
                        self.ty(key.ty);
                    }
                    self.usize(table.fields.len());
                    for field in &table.fields {
                        self.string(&field.symbol);
                        self.ty(field.ty);
                        match &field.scheme {
                            Some(scheme) => {
                                self.u8(1);
                                self.u64(u64::from(scheme.id.0));
                            }
                            None => self.u8(0),
                        }
                    }
                }
            }
            None => self.u8(0),
        }
        self.usize(self.program.items.len());
        for item in &self.program.items {
            match item {
                hir::Item::Const(decl) => {
                    self.tag("const");
                    self.string(&decl.symbol);
                    self.ty(decl.ty);
                    self.hash_const_expr(&decl.value);
                }
                hir::Item::Relation(decl) => {
                    self.tag("relation");
                    self.string(&decl.symbol);
                    self.usize(decl.params.len());
                    for param in &decl.params {
                        self.string(&param.symbol);
                        self.ty(param.ty);
                    }
                    self.usize(decl.results.len());
                    for result in &decl.results {
                        self.string(&result.symbol);
                        self.ty(result.ty);
                    }
                    match &decl.body {
                        hir::RelationBody::Enum { values } => {
                            self.tag("enum");
                            self.usize(values.len());
                            for value in values {
                                self.hash_const_expr(value);
                            }
                        }
                        hir::RelationBody::Range { start, end } => {
                            self.tag("range");
                            self.hash_const_expr(start);
                            self.hash_const_expr(end);
                        }
                        hir::RelationBody::Map { entries } => {
                            self.tag("map");
                            self.usize(entries.len());
                            for entry in entries {
                                self.usize(entry.inputs.len());
                                for input in &entry.inputs {
                                    self.hash_const_expr(input);
                                }
                                self.usize(entry.outputs.len());
                                for output in &entry.outputs {
                                    self.hash_const_expr(output);
                                }
                            }
                        }
                        hir::RelationBody::Set { tuples } => {
                            self.tag("set");
                            self.usize(tuples.len());
                            for tuple in tuples.iter() {
                                self.usize(tuple.len());
                                for value in tuple {
                                    self.hash_const_expr(value);
                                }
                            }
                        }
                        hir::RelationBody::Extern => self.tag("extern"),
                    }
                }
                hir::Item::Event(decl) => {
                    self.tag("event");
                    self.string(&decl.symbol);
                    self.usize(decl.fields.len());
                    for field in &decl.fields {
                        self.string(&field.symbol);
                        self.ty(field.ty);
                    }
                }
                hir::Item::Callable(decl) => {
                    self.tag("callable");
                    self.string(&decl.symbol);
                    self.u8(match decl.kind {
                        hir::CallableKind::Function => 0,
                        hir::CallableKind::Query => 1,
                        hir::CallableKind::Tx => 2,
                    });
                    self.usize(decl.params.len());
                    for param in &decl.params {
                        self.string(&param.symbol);
                        self.ty(param.ty);
                    }
                    self.types(&decl.returns);
                    let mut env = CallableFingerprintEnv::default();
                    for param in &decl.params {
                        env.params.insert(param.id, env.next_binding);
                        env.next_binding += 1;
                    }
                    self.hash_region(&decl.body.region, &mut env);
                }
            }
        }
    }

    fn hash_region(&mut self, region: &hir::Region, env: &mut CallableFingerprintEnv) {
        self.usize(region.statements.len());
        for statement in &region.statements {
            self.hash_stmt(statement, env);
        }
        match &region.terminator {
            hir::Terminator::Return { values, .. } => {
                self.tag("return");
                self.usize(values.len());
                for value in values {
                    self.hash_expr(value, env);
                }
            }
            hir::Terminator::Yield { values, .. } => {
                self.tag("yield");
                self.usize(values.len());
                for value in values {
                    self.hash_expr(value, env);
                }
            }
        }
    }

    fn hash_stmt(&mut self, statement: &hir::Stmt, env: &mut CallableFingerprintEnv) {
        match statement {
            hir::Stmt::Let(stmt) => {
                self.tag("let");
                self.string(&stmt.binding.symbol);
                self.ty(stmt.binding.ty);
                self.hash_expr(&stmt.value, env);
                env.bindings.insert(stmt.binding.id, env.next_binding);
                env.next_binding += 1;
            }
            hir::Stmt::StateAssign(stmt) => {
                self.tag("state-assign");
                self.string(self.table_symbols[&stmt.target.table]);
                self.usize(stmt.target.key.len());
                for key in &stmt.target.key {
                    self.hash_expr(key, env);
                }
                self.string(self.field_symbols[&(stmt.target.table, stmt.target.field)]);
                self.hash_expr(&stmt.value, env);
            }
            hir::Stmt::Assert(stmt) => match stmt {
                hir::AssertStmt::Expr { expr, .. } => {
                    self.tag("assert");
                    self.hash_expr(expr, env);
                }
                hir::AssertStmt::Relation { relation, args, .. } => {
                    self.tag("assert-relation");
                    self.string(self.relation_symbols[relation]);
                    self.usize(args.len());
                    for arg in args {
                        self.hash_expr(arg, env);
                    }
                }
            },
            hir::Stmt::Emit(stmt) => {
                self.tag("emit");
                self.string(self.event_symbols[&stmt.event]);
                self.usize(stmt.args.len());
                for arg in &stmt.args {
                    self.hash_expr(arg, env);
                }
            }
            hir::Stmt::If(stmt) => {
                self.tag("if");
                self.hash_expr(&stmt.cond, env);
                self.hash_nested_region(&stmt.then_region, env);
                self.hash_nested_region(&stmt.else_region, env);
            }
            hir::Stmt::Match(stmt) => {
                self.tag("match");
                self.hash_expr(&stmt.scrutinee, env);
                self.usize(stmt.arms.len());
                for arm in &stmt.arms {
                    match &arm.pattern {
                        hir::MatchPattern::Literal(value) => {
                            self.tag("literal");
                            self.hash_value(value);
                        }
                    }
                    self.hash_nested_region(&arm.region, env);
                }
                match &stmt.default {
                    Some(region) => {
                        self.tag("default");
                        self.hash_nested_region(region, env);
                    }
                    None => self.tag("no-default"),
                }
            }
            hir::Stmt::Expr(stmt) => {
                self.tag("expr");
                self.hash_expr(&stmt.expr, env);
            }
        }
    }

    fn hash_nested_region(&mut self, region: &hir::Region, env: &mut CallableFingerprintEnv) {
        let saved_bindings = env.bindings.clone();
        self.hash_region(region, env);
        env.bindings = saved_bindings;
    }

    fn hash_expr(&mut self, expr: &hir::Expr, env: &CallableFingerprintEnv) {
        match expr {
            hir::Expr::Literal(expr) => {
                self.tag("literal");
                self.hash_value(&expr.value);
            }
            hir::Expr::Local(expr) => match expr.local {
                hir::LocalRef::Param(id) => {
                    self.tag("param");
                    self.u32(env.params[&id]);
                }
                hir::LocalRef::Binding(id) => {
                    self.tag("binding");
                    self.u32(env.bindings[&id]);
                }
            },
            hir::Expr::Context(expr) => {
                self.tag("context");
                self.string(self.context_field_symbols[&expr.field]);
            }
            hir::Expr::Const(expr) => {
                self.tag("const-ref");
                self.string(self.const_symbols[&expr.const_id]);
            }
            hir::Expr::TableRead(expr) => {
                self.tag("table-read");
                self.string(self.table_symbols[&expr.table]);
                self.usize(expr.key.len());
                for key in &expr.key {
                    self.hash_expr(key, env);
                }
                self.string(self.field_symbols[&(expr.table, expr.field)]);
                self.ty(expr.ty);
            }
            hir::Expr::Unary(expr) => {
                self.tag("unary");
                self.u8(match expr.op {
                    hir::UnaryOp::Not => 0,
                    hir::UnaryOp::Neg => 1,
                });
                self.ty(expr.ty);
                self.hash_expr(&expr.expr, env);
            }
            hir::Expr::Binary(expr) => {
                self.tag("binary");
                self.u8(match expr.op {
                    hir::BinaryOp::Add => 0,
                    hir::BinaryOp::Sub => 1,
                    hir::BinaryOp::Mul => 2,
                    hir::BinaryOp::Div => 3,
                    hir::BinaryOp::Mod => 4,
                    hir::BinaryOp::Eq => 5,
                    hir::BinaryOp::Ne => 6,
                    hir::BinaryOp::Lt => 7,
                    hir::BinaryOp::Le => 8,
                    hir::BinaryOp::Gt => 9,
                    hir::BinaryOp::Ge => 10,
                    hir::BinaryOp::And => 11,
                    hir::BinaryOp::Or => 12,
                });
                self.ty(expr.ty);
                self.hash_expr(&expr.lhs, env);
                self.hash_expr(&expr.rhs, env);
            }
            hir::Expr::CallFunction(expr) => {
                self.tag("call-function");
                self.string(self.callable_symbols[&expr.callee]);
                self.types(&expr.returns);
                self.usize(expr.args.len());
                for arg in &expr.args {
                    self.hash_expr(arg, env);
                }
            }
            hir::Expr::CallCapability(expr) => {
                self.tag("call-capability");
                self.string(self.capability_paths[&expr.capability]);
                self.types(&expr.outputs);
                self.usize(expr.args.len());
                for arg in &expr.args {
                    self.hash_expr(arg, env);
                }
            }
            hir::Expr::Hash(expr) => {
                self.tag("hash");
                self.u8(match expr.family {
                    hir::HashFamily::Poseidon => 0,
                });
                self.types(&expr.inputs);
                self.usize(expr.args.len());
                for arg in &expr.args {
                    self.hash_expr(arg, env);
                }
            }
            hir::Expr::EvalRelation(expr) => {
                self.tag("eval-relation");
                self.string(self.relation_symbols[&expr.relation]);
                self.types(&expr.outputs);
                self.usize(expr.args.len());
                for arg in &expr.args {
                    self.hash_expr(arg, env);
                }
            }
            hir::Expr::Select(expr) => {
                self.tag("select");
                self.ty(expr.ty);
                self.hash_expr(&expr.cond, env);
                self.hash_expr(&expr.if_true, env);
                self.hash_expr(&expr.if_false, env);
            }
        }
    }

    fn hash_const_expr(&mut self, expr: &hir::ConstExpr) {
        match expr {
            hir::ConstExpr::Literal(value) => {
                self.tag("const-literal");
                self.hash_value(value);
            }
            hir::ConstExpr::Unary { op, expr } => {
                self.tag("const-unary");
                self.u8(match op {
                    hir::UnaryOp::Not => 0,
                    hir::UnaryOp::Neg => 1,
                });
                self.hash_const_expr(expr);
            }
            hir::ConstExpr::Binary { op, lhs, rhs } => {
                self.tag("const-binary");
                self.u8(match op {
                    hir::BinaryOp::Add => 0,
                    hir::BinaryOp::Sub => 1,
                    hir::BinaryOp::Mul => 2,
                    hir::BinaryOp::Div => 3,
                    hir::BinaryOp::Mod => 4,
                    hir::BinaryOp::Eq => 5,
                    hir::BinaryOp::Ne => 6,
                    hir::BinaryOp::Lt => 7,
                    hir::BinaryOp::Le => 8,
                    hir::BinaryOp::Gt => 9,
                    hir::BinaryOp::Ge => 10,
                    hir::BinaryOp::And => 11,
                    hir::BinaryOp::Or => 12,
                });
                self.hash_const_expr(lhs);
                self.hash_const_expr(rhs);
            }
        }
    }

    fn hash_value(&mut self, value: &PortableValue) {
        self.ty(value.type_id());
        self.usize(value.payload().len());
        self.hasher.update(value.payload());
    }

    fn types(&mut self, tys: &[ir::TypeRef]) {
        self.usize(tys.len());
        for ty in tys {
            self.ty(*ty);
        }
    }

    fn tag(&mut self, tag: &str) {
        self.string(tag);
    }

    fn string(&mut self, value: &str) {
        self.usize(value.len());
        self.hasher.update(value.as_bytes());
    }

    fn ty(&mut self, ty: ir::TypeRef) {
        self.u64(u64::from(ty.0));
    }

    fn usize(&mut self, value: usize) {
        self.hasher.update(&(value as u64).to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.hasher.update(&value.to_le_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.hasher.update(&value.to_le_bytes());
    }

    fn u8(&mut self, value: u8) {
        self.hasher.update(&[value]);
    }
}

#[derive(Default)]
struct CallableFingerprintEnv {
    params: BTreeMap<hir::ParamId, u32>,
    bindings: BTreeMap<hir::BindingId, u32>,
    next_binding: u32,
}

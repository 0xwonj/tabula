//! Tests for HIR build and verification stages.

mod common;

use common::{custom_no_eq_prelude, prelude};
use tabula_lang::hir::{CallableKind, Expr, Item, Stmt, Terminator};
use tabula_lang::{FrontendErrorKind, build_hir, compile_to_hir, parse_program, verify_hir};

#[test]
fn compile_to_hir_classifies_calls_and_state_ops() {
    let source = r#"
use capability poseidon_hash;

program Registry

state {
  table users(key id: u64) {
    active: bool @ssmc;
    tier: u64 @ssmc;
  }
}

const MAX_TIER: u64 = 3;

relation AllowedTier(tier: u64) = enum { 0, 1, 2, 3 };

fn validate_tier(tier: u64) {
  assert relation AllowedTier(tier);
  return;
}

tx register(id: u64, tier: u64) {
  validate_tier(tier);
  let digest = poseidon_hash(tier);
  let active = users[id].active;
  assert active;
  users[id].tier = select(true, tier, MAX_TIER);
  return;
}
"#;
    let program = compile_to_hir(source, &prelude()).expect("hir");
    assert_eq!(program.program().uses.len(), 1);
    let callable = program
        .program()
        .items
        .iter()
        .find_map(|item| match item {
            Item::Callable(callable) if callable.symbol == "register" => Some(callable),
            _ => None,
        })
        .expect("register callable");
    assert!(matches!(callable.body.region.statements[0], Stmt::Expr(_)));
    assert!(matches!(callable.body.region.statements[1], Stmt::Let(_)));
    assert!(matches!(callable.body.region.statements[2], Stmt::Let(_)));
    assert!(matches!(
        callable.body.region.statements[3],
        Stmt::Assert(_)
    ));
    assert!(matches!(
        callable.body.region.statements[4],
        Stmt::StateAssign(_)
    ));
}

#[test]
fn compile_to_hir_builds_v2_boundary_surface() {
    let source = r#"
program Registry

context {
  caller: u64;
}

event Registered(id: u64, actor: u64);

query current_actor(seed: u64) -> u64 {
  let actor = caller;
  return select(true, actor, seed);
}

tx register(id: u64) {
  emit Registered(id, caller);
  return;
}
"#;

    let program = compile_to_hir(source, &prelude()).expect("hir");
    assert!(program.program().context.is_some());
    assert!(
        program
            .program()
            .items
            .iter()
            .any(|item| matches!(item, Item::Event(_)))
    );

    let query = program
        .program()
        .items
        .iter()
        .find_map(|item| match item {
            Item::Callable(callable) if callable.kind == CallableKind::Query => Some(callable),
            _ => None,
        })
        .expect("query callable");
    assert!(matches!(query.body.region.statements[0], Stmt::Let(_)));
    let Stmt::Let(let_stmt) = &query.body.region.statements[0] else {
        unreachable!()
    };
    assert!(matches!(let_stmt.value, Expr::Context(_)));

    let tx = program
        .program()
        .items
        .iter()
        .find_map(|item| match item {
            Item::Callable(callable) if callable.kind == CallableKind::Tx => Some(callable),
            _ => None,
        })
        .expect("tx callable");
    assert!(matches!(tx.body.region.statements[0], Stmt::Emit(_)));
}

#[test]
fn compile_to_hir_builds_v3_statement_level_control_regions() {
    let source = r#"
program Control

tx choose(flag: bool, value: u64) {
  if flag {
    let selected = value;
  } else {
    let selected = 0;
  }
  match value {
    0 => {
      assert true;
    }
    _ => {
      assert true;
    }
  }
  return;
}
"#;

    let program = compile_to_hir(source, &prelude()).expect("hir");
    let tx = program
        .program()
        .items
        .iter()
        .find_map(|item| match item {
            Item::Callable(callable) if callable.kind == CallableKind::Tx => Some(callable),
            _ => None,
        })
        .expect("tx");

    assert!(matches!(tx.body.region.statements[0], Stmt::If(_)));
    assert!(matches!(tx.body.region.statements[1], Stmt::Match(_)));

    let Stmt::If(if_stmt) = &tx.body.region.statements[0] else {
        unreachable!()
    };
    assert!(matches!(
        if_stmt.then_region.terminator,
        Terminator::Yield { .. }
    ));
    assert!(matches!(
        if_stmt.else_region.terminator,
        Terminator::Yield { .. }
    ));

    let Stmt::Match(match_stmt) = &tx.body.region.statements[1] else {
        unreachable!()
    };
    assert_eq!(match_stmt.arms.len(), 1);
    assert!(match_stmt.default.is_some());
}

#[test]
fn compile_to_hir_rejects_pure_expression_statement() {
    let source = r#"
program Bad

tx broken() {
  1 + 2;
  return;
}
"#;
    assert!(compile_to_hir(source, &prelude()).is_err());
}

#[test]
fn build_hir_defers_pure_expression_statement_rejection_to_verify() {
    let source = r#"
program Bad

tx broken() {
  1 + 2;
  return;
}
"#;
    let ast = parse_program(source).expect("ast");
    let hir = build_hir(ast, &prelude()).expect("raw hir");
    assert!(verify_hir(hir, &prelude()).is_err());
}

#[test]
fn build_hir_defers_tx_bare_call_rejection_to_verify() {
    let source = r#"
program Bad

tx target() {
  return;
}

tx caller() {
  target();
  return;
}
"#;
    let ast = parse_program(source).expect("ast");
    let hir = build_hir(ast, &prelude()).expect("raw hir");
    assert!(verify_hir(hir, &prelude()).is_err());
}

#[test]
fn build_hir_defers_invalid_operator_capability_to_verify() {
    let source = r#"
program Bad

fn broken(x: OpaqueNoEq) -> bool {
  return x == x;
}
"#;
    let ast = parse_program(source).expect("ast");
    let hir = build_hir(ast, &custom_no_eq_prelude()).expect("raw hir");
    assert!(verify_hir(hir, &custom_no_eq_prelude()).is_err());
}

#[test]
fn build_hir_defers_invalid_field_scheme_to_verify() {
    let source = r#"
program Bad

state {
  table users(key id: u64) {
    digest: bytes32 @custom;
  }
}

tx noop() {
  return;
}
"#;
    let mut registry = tabula_profile::builtin_semantic_registry().expect("registry");
    registry
        .register_scheme_name("custom", tabula_core::SchemeId(42))
        .expect("scheme name");
    let prelude = tabula_lang::FrontendPrelude::new(registry, vec![]).expect("prelude");
    let ast = parse_program(source).expect("ast");
    let hir = build_hir(ast, &prelude).expect("raw hir");
    assert!(verify_hir(hir, &prelude).is_err());
}

#[test]
fn compile_to_hir_rejects_tuple_patterns_with_v2_diagnostic() {
    let source = r#"
program Bad

query pair(seed: u64) -> u64 {
  let (lhs, rhs) = select(true, seed, seed);
  return lhs;
}
"#;
    let err = compile_to_hir(source, &prelude()).expect_err("compile should fail");
    assert_eq!(err.kind, FrontendErrorKind::UnsupportedFeature);
    assert!(
        err.message
            .contains("tuple patterns are intentionally deferred to a later phase")
    );
}

#[test]
fn compile_to_hir_rejects_wildcard_match_arm_before_literal_arm() {
    let source = r#"
program Bad

tx broken(value: u64) {
  match value {
    _ => {
      assert true;
    }
    1 => {
      assert true;
    }
  }
  return;
}
"#;
    let err = compile_to_hir(source, &prelude()).expect_err("compile should fail");
    assert_eq!(err.kind, FrontendErrorKind::InvalidProgram);
    assert!(err.message.contains("wildcard match arm must be last"));
}

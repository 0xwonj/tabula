//! Tests for frontend verification rejections.

mod common;

use common::prelude;
use tabula_lang::{FrontendErrorKind, compile_to_hir};

#[test]
fn compile_to_hir_rejects_query_state_write() {
    let source = r#"
program Bad

state {
  table users(key id: u64) {
    tier: u64 @ssmc;
  }
}

query bad(id: u64) -> u64 {
  users[id].tier = id;
  return id;
}
"#;
    assert!(compile_to_hir(source, &prelude()).is_err());
}

#[test]
fn compile_to_hir_rejects_query_emit() {
    let source = r#"
program Bad

context {
  caller: u64;
}

event Registered(actor: u64);

query bad() -> u64 {
  emit Registered(caller);
  return caller;
}
"#;
    assert!(compile_to_hir(source, &prelude()).is_err());
}

#[test]
fn compile_to_hir_rejects_nested_return_inside_if_branch() {
    let source = r#"
program Bad

tx broken() {
  if true {
    return;
  }
  return;
}
"#;
    let err = compile_to_hir(source, &prelude()).expect_err("compile should fail");
    assert_eq!(err.kind, FrontendErrorKind::InvalidProgram);
    assert!(
        err.message
            .contains("return is not allowed inside nested if/match branches in exact V3")
    );
}

#[test]
fn compile_to_hir_rejects_query_state_write_under_control() {
    let source = r#"
program Bad

state {
  table users(key id: u64) {
    tier: u64 @ssmc;
  }
}

query bad(id: u64) -> u64 {
  if true {
    users[id].tier = id;
  }
  return id;
}
"#;
    assert!(compile_to_hir(source, &prelude()).is_err());
}

#[test]
fn compile_to_hir_rejects_query_emit_under_control() {
    let source = r#"
program Bad

context {
  caller: u64;
}

event Registered(actor: u64);

query bad(flag: bool) -> u64 {
  match flag {
    true => {
      emit Registered(caller);
    }
    _ => {
      assert true;
    }
  }
  return caller;
}
"#;
    assert!(compile_to_hir(source, &prelude()).is_err());
}

#[test]
fn compile_to_hir_rejects_param_shadowing_context_field() {
    let source = r#"
program Bad

context {
  caller: u64;
}

query bad(caller: u64) -> u64 {
  return caller;
}
"#;
    assert!(compile_to_hir(source, &prelude()).is_err());
}

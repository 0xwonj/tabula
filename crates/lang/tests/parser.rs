mod common;

use common::assert_unsupported;
use tabula_lang::parse_program;

#[test]
fn parser_accepts_v2_boundary_surface() {
    let source = r#"
use capability poseidon_hash;

program Registry

context {
  caller: u64;
}

state {
  table users(key id: u64) {
    active: bool @ssmc;
    tier: u64 @ssmc;
  }
}

const MAX_TIER: u64 = 3;

relation AllowedTier(tier: u64) = enum { 0, 1, 2, 3 };

event Registered(id: u64, actor: u64);

fn validate_tier(tier: u64) {
  assert relation AllowedTier(tier);
  return;
}

query current_actor(seed: u64) -> u64 {
  let actor = caller;
  return select(true, actor, seed);
}

tx register(id: u64, tier: u64) {
  validate_tier(tier);
  let digest = poseidon_hash(tier);
  emit Registered(id, caller);
  users[id].active = true;
  users[id].tier = select(true, tier, MAX_TIER);
  return;
}
"#;
    let ast = parse_program(source).expect("ast");
    assert_eq!(ast.symbol, "Registry");
    assert_eq!(ast.uses.len(), 1);
    assert_eq!(ast.decls.len(), 8);
}

#[test]
fn parser_accepts_v3_structured_control() {
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
    let ast = parse_program(source).expect("ast");
    assert_eq!(ast.symbol, "Control");
}

#[test]
fn parser_rejects_requires_as_deferred_beyond_v2() {
    assert_unsupported(
        r#"
program Deferred

query whoami() -> u64
requires true {
  return 0;
}
"#,
        "requires is intentionally deferred to a later phase",
    );
}

#[test]
fn parser_rejects_ensures_as_deferred_beyond_v2() {
    assert_unsupported(
        r#"
program Deferred

tx register()
ensures true {
  return;
}
"#,
        "ensures is intentionally deferred to a later phase",
    );
}

#[test]
fn parser_rejects_for_as_deferred_feature() {
    assert_unsupported(
        r#"
program Deferred

tx register() {
  for item in items {
    assert true;
  }
  return;
}
"#,
        "for is intentionally deferred to a later phase",
    );
}

#[test]
fn parser_rejects_v3_spec_forms_as_deferred() {
    assert_unsupported(
        r#"
program Deferred

predicate P();
"#,
        "predicate is intentionally deferred to a later phase",
    );
}

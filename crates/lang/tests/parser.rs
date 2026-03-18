#![allow(missing_docs)]
use tabula_lang::ast::*;
use tabula_lang::lexer::lex;
use tabula_lang::parser::parse;

fn parse_source(source: &str) -> Program {
    let tokens = lex(source).expect("lexer failed");
    parse(tokens).expect("parser failed")
}

// --- Table declarations ---

#[test]
fn test_parse_table_decl() {
    let prog = parse_source("table balances { balance: u64 }");
    assert_eq!(prog.tables.len(), 1);
    assert_eq!(prog.tables[0].name, "balances");
    assert_eq!(prog.tables[0].columns.len(), 1);
    assert_eq!(prog.tables[0].columns[0].name, "balance");
    assert_eq!(prog.tables[0].columns[0].ty, TypeName::U64);
    assert_eq!(prog.tables[0].columns[0].scheme, None);
}

#[test]
fn test_parse_table_multi_columns() {
    let prog = parse_source("table entity { hp: u64, atk: i64, alive: bool }");
    assert_eq!(prog.tables[0].columns.len(), 3);
    assert_eq!(prog.tables[0].columns[0].ty, TypeName::U64);
    assert_eq!(prog.tables[0].columns[1].ty, TypeName::I64);
    assert_eq!(prog.tables[0].columns[2].ty, TypeName::Bool);
}

#[test]
fn test_parse_table_trailing_comma() {
    let prog = parse_source("table t { a: u64, b: i64, }");
    assert_eq!(prog.tables[0].columns.len(), 2);
}

#[test]
fn test_parse_table_column_scheme_annotations() {
    let prog = parse_source("table t { a: u64 @ssmc, b: u64 @smt, c: u64 @scheme(42) }");
    assert_eq!(
        prog.tables[0].columns[0].scheme,
        Some(ColumnSchemeDecl::Ssmc)
    );
    assert_eq!(
        prog.tables[0].columns[1].scheme,
        Some(ColumnSchemeDecl::Smt)
    );
    assert_eq!(
        prog.tables[0].columns[2].scheme,
        Some(ColumnSchemeDecl::Numeric(42))
    );
}

// --- Tx declarations ---

#[test]
fn test_parse_empty_tx() {
    let prog = parse_source("tx noop() {}");
    assert_eq!(prog.transactions.len(), 1);
    assert_eq!(prog.transactions[0].name, "noop");
    assert!(prog.transactions[0].params.is_empty());
    assert!(prog.transactions[0].body.is_empty());
}

#[test]
fn test_parse_tx_with_params() {
    let prog = parse_source("tx transfer(from: u64, to: u64, amount: u64) {}");
    let tx = &prog.transactions[0];
    assert_eq!(tx.params.len(), 3);
    assert_eq!(tx.params[0].name, "from");
    assert_eq!(tx.params[0].ty, TypeName::U64);
    assert_eq!(tx.params[2].name, "amount");
}

// --- Let statements ---

#[test]
fn test_parse_let_simple() {
    let prog = parse_source("tx t(x: u64) { let y = x }");
    let body = &prog.transactions[0].body;
    assert_eq!(body.len(), 1);
    assert!(matches!(&body[0].kind, StmtKind::Let { name, .. } if name == "y"));
}

#[test]
fn test_parse_let_cell_read() {
    let prog = parse_source(
        "table balances { balance: u64 }\n\
         tx t(id: u64) { let bal = balances[id].balance }",
    );
    let body = &prog.transactions[0].body;
    assert!(matches!(
        &body[0].kind,
        StmtKind::Let { value, .. } if matches!(&value.kind, ExprKind::CellRead { table, col, .. } if table == "balances" && col == "balance")
    ));
}

#[test]
fn test_parse_let_arithmetic() {
    let prog = parse_source("tx t(a: u64, b: u64) { let c = a + b * 2 }");
    let body = &prog.transactions[0].body;
    // Should parse as a + (b * 2) due to precedence.
    if let StmtKind::Let { value, .. } = &body[0].kind {
        assert!(matches!(
            &value.kind,
            ExprKind::BinOp { op: BinOp::Add, .. }
        ));
    } else {
        panic!("expected let statement");
    }
}

// --- Assert ---

#[test]
fn test_parse_assert() {
    let prog = parse_source("tx t(x: u64, y: u64) { assert x >= y }");
    let body = &prog.transactions[0].body;
    if let StmtKind::Assert { condition } = &body[0].kind {
        assert!(matches!(
            &condition.kind,
            ExprKind::BinOp { op: BinOp::Gte, .. }
        ));
    } else {
        panic!("expected assert");
    }
}

#[test]
fn test_parse_assert_logical() {
    let prog = parse_source("tx t(x: u64) { assert x > 0 && x < 100 }");
    let body = &prog.transactions[0].body;
    if let StmtKind::Assert { condition } = &body[0].kind {
        assert!(matches!(
            &condition.kind,
            ExprKind::BinOp { op: BinOp::And, .. }
        ));
    } else {
        panic!("expected assert");
    }
}

// --- Assign ---

#[test]
fn test_parse_assign() {
    let prog = parse_source(
        "table t { v: u64 }\n\
         tx w(id: u64, val: u64) { t[id].v = val }",
    );
    let body = &prog.transactions[0].body;
    assert!(matches!(
        &body[0].kind,
        StmtKind::Assign { table, col, .. } if table == "t" && col == "v"
    ));
}

// --- Emit ---

#[test]
fn test_parse_emit() {
    let prog = parse_source("tx t(a: u64, b: u64) { emit \"transfer\" (a, b) }");
    let body = &prog.transactions[0].body;
    if let StmtKind::Emit { topic, args, .. } = &body[0].kind {
        assert_eq!(topic, "transfer");
        assert_eq!(args.len(), 2);
    } else {
        panic!("expected emit");
    }
}

// --- Destructuring ---

#[test]
fn test_parse_let_destructure() {
    let prog = parse_source("tx t(a: u64, b: u64) { let (q, r) = divmod(a, b) }");
    let body = &prog.transactions[0].body;
    assert!(matches!(
        &body[0].kind,
        StmtKind::LetDestructure { first, second, .. } if first == "q" && second == "r"
    ));
}

// --- Hash ---

#[test]
fn test_parse_hash_call() {
    let prog = parse_source("tx t(a: u64, b: u64) { let h = hash(a, b) }");
    let body = &prog.transactions[0].body;
    if let StmtKind::Let { value, .. } = &body[0].kind {
        assert!(matches!(&value.kind, ExprKind::Hash(args) if args.len() == 2));
    } else {
        panic!("expected let with hash");
    }
}

// --- Static read ---

#[test]
fn test_parse_static_read() {
    let prog = parse_source("tx t(k: u64) { let v = @config[k].flag }");
    let body = &prog.transactions[0].body;
    if let StmtKind::Let { value, .. } = &body[0].kind {
        assert!(matches!(
            &value.kind,
            ExprKind::StaticRead { table, col, .. } if table == "config" && col == "flag"
        ));
    } else {
        panic!("expected let with static read");
    }
}

// --- Complex program ---

#[test]
fn test_parse_full_transfer() {
    let source = "\
table balances { balance: u64 }

tx transfer(from: u64, to: u64, amount: u64) {
    let sender_bal = balances[from].balance
    let recv_bal = balances[to].balance
    assert sender_bal >= amount
    let new_sender = sender_bal - amount
    let new_recv = recv_bal + amount
    balances[from].balance = new_sender
    balances[to].balance = new_recv
    emit \"transfer\" (from, to, amount)
}";
    let prog = parse_source(source);
    assert_eq!(prog.tables.len(), 1);
    assert_eq!(prog.transactions.len(), 1);
    assert_eq!(prog.transactions[0].body.len(), 8);
}

// --- Unary operators ---

#[test]
fn test_parse_negation() {
    let prog = parse_source("tx t(x: i64) { let y = -x }");
    let body = &prog.transactions[0].body;
    if let StmtKind::Let { value, .. } = &body[0].kind {
        assert!(matches!(
            &value.kind,
            ExprKind::UnaryOp {
                op: UnaryOp::Neg,
                ..
            }
        ));
    } else {
        panic!("expected let with negation");
    }
}

#[test]
fn test_parse_not() {
    let prog = parse_source("tx t(x: bool) { assert !x }");
    let body = &prog.transactions[0].body;
    if let StmtKind::Assert { condition } = &body[0].kind {
        assert!(matches!(
            &condition.kind,
            ExprKind::UnaryOp {
                op: UnaryOp::Not,
                ..
            }
        ));
    } else {
        panic!("expected assert with not");
    }
}

// --- Select ---

#[test]
fn test_parse_select_call() {
    let prog = parse_source("tx t(c: bool, a: u64, b: u64) { let x = select(c, a, b) }");
    let body = &prog.transactions[0].body;
    if let StmtKind::Let { value, .. } = &body[0].kind {
        assert!(matches!(&value.kind, ExprKind::Select { .. }));
    } else {
        panic!("expected let with select");
    }
}

// --- Precompile ---

#[test]
fn test_parse_precompile_no_inputs() {
    let prog = parse_source("tx t() { @precompile(1, [out]) }");
    let body = &prog.transactions[0].body;
    assert_eq!(body.len(), 1);
    if let StmtKind::Precompile {
        id,
        dst_names,
        inputs,
    } = &body[0].kind
    {
        assert_eq!(*id, 1);
        assert_eq!(dst_names, &["out"]);
        assert!(inputs.is_empty());
    } else {
        panic!("expected precompile stmt");
    }
}

#[test]
fn test_parse_precompile_with_inputs() {
    let prog = parse_source("tx t(x: u64, y: u64) { @precompile(42, [a, b], x, y) }");
    let body = &prog.transactions[0].body;
    assert_eq!(body.len(), 1);
    if let StmtKind::Precompile {
        id,
        dst_names,
        inputs,
    } = &body[0].kind
    {
        assert_eq!(*id, 42);
        assert_eq!(dst_names, &["a", "b"]);
        assert_eq!(inputs.len(), 2);
        assert!(matches!(&inputs[0].kind, ExprKind::Ident(n) if n == "x"));
        assert!(matches!(&inputs[1].kind, ExprKind::Ident(n) if n == "y"));
    } else {
        panic!("expected precompile stmt");
    }
}

#[test]
fn test_parse_precompile_hex_id() {
    let prog = parse_source("tx t() { @precompile(0x0001, [r]) }");
    let body = &prog.transactions[0].body;
    if let StmtKind::Precompile { id, .. } = &body[0].kind {
        assert_eq!(*id, 1);
    } else {
        panic!("expected precompile stmt");
    }
}

#[test]
fn test_parse_precompile_error_bad_name() {
    let tokens = lex("tx t() { @foobar(1, [x]) }").unwrap();
    let result = parse(tokens);
    assert!(result.is_err());
}

// --- Error cases ---

#[test]
fn test_parse_error_missing_brace() {
    let tokens = lex("table t { a: u64").unwrap();
    let result = parse(tokens);
    assert!(result.is_err());
}

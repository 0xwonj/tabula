//! End-to-end STARK prover/verifier tests.
//!
//! Pipeline: DSL source -> compile -> execute -> witness -> trace bundle -> prove -> verify.

mod common;

use tabula_core::{ColId, RowKey, TableId, Transaction, TxTypeId, Value};

use common::{make_tx, make_tx_nonce, prove_and_verify};

#[test]
fn stark_prove_verify_read_write() {
    let source = "\
table accounts { balance: u64 }
tx touch(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
}";
    prove_and_verify(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(50))],
        vec![make_tx(vec![Value::U64(10)])],
    );
}

#[test]
fn stark_prove_verify_arith() {
    let source = "\
table t { val: u64 }
tx op(id: u64) {
    let x = t[id].val
    let y = x + x
    t[id].val = y - x
}";
    prove_and_verify(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(100))],
        vec![make_tx(vec![Value::U64(10)])],
    );
}

#[test]
fn stark_prove_verify_cmp_assert() {
    let source = "\
table t { val: u64 }
tx op(id: u64) {
    let x = t[id].val
    let y = x + x
    assert y >= x
    t[id].val = y - x
}";

    prove_and_verify(
        source,
        &[(TableId(0), ColId(0), RowKey(5), Value::U64(100))],
        vec![make_tx(vec![Value::U64(5)])],
    );
}

#[test]
fn stark_prove_verify_multi_tx_batch() {
    // Two independent transactions in one batch, each touching a different key.
    let source = "\
table t { val: u64 }
tx bump(id: u64) {
    let x = t[id].val
    t[id].val = x + x
}";
    prove_and_verify(
        source,
        &[
            (TableId(0), ColId(0), RowKey(1), Value::U64(10)),
            (TableId(0), ColId(0), RowKey(2), Value::U64(20)),
        ],
        vec![
            make_tx_nonce(vec![Value::U64(1)], 0),
            make_tx_nonce(vec![Value::U64(2)], 1),
        ],
    );
}

#[test]
fn stark_prove_verify_mul() {
    // Multiplication opcode.
    let source = "\
table t { val: u64 }
tx square(id: u64) {
    let x = t[id].val
    t[id].val = x * x
}";
    prove_and_verify(
        source,
        &[(TableId(0), ColId(0), RowKey(1), Value::U64(7))],
        vec![make_tx(vec![Value::U64(1)])],
    );
}

#[test]
fn stark_prove_verify_select() {
    // Conditional select opcode: pick between two read values.
    let source = "\
table t { val: u64 }
tx pick(id: u64, use_first: bool) {
    let x = t[id].val
    let result = select(use_first, x, 0)
    t[id].val = result
}";
    prove_and_verify(
        source,
        &[(TableId(0), ColId(0), RowKey(1), Value::U64(42))],
        vec![Transaction {
            tx_type: TxTypeId(0),
            params: vec![Value::U64(1), Value::Bool(true)],
            sender: [7u8; 32],
            nonce: 0,
            signature: vec![],
        }],
    );
}

//! Canonical program-source fixtures for black-box tests.

use tabula_compiler::TRANSFER_EXAMPLE_TAB_SOURCE;

const TOUCH_ACCOUNTS_SOURCE: &str = "\
table accounts { balance: u64 }
tx touch(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
}";

const TRANSFER_BALANCES_SOURCE: &str = "\
table balances { balance: u64 }
tx transfer(from: u64, to: u64, amount: u64) {
    let sender_bal = balances[from].balance
    let recv_bal = balances[to].balance
    assert sender_bal >= amount
    balances[from].balance = sender_bal - amount
    balances[to].balance = recv_bal + amount
}";

const PEEK_ACCOUNTS_SOURCE: &str = "\
table accounts { balance: u64 }
tx peek() {
    let _bal = accounts[0].balance
}";

const SHIELDED_PEEK_SOURCE: &str = "\
table balances { shielded: u64 @smt }
tx peek() {
    let _bal = balances[0].shielded
}";

const LIQUID_SHIELDED_BUMP_SOURCE: &str = "\
table balances {
    liquid: u64,
    shielded: u64 @smt,
}

tx bump(amount: u64) {
    let liquid_now = balances[0].liquid
    let shielded_now = balances[0].shielded
    balances[0].shielded = shielded_now + amount
}";

const ARITH_ADD_SUB_SOURCE: &str = "\
table t { val: u64 }
tx op(id: u64) {
    let x = t[id].val
    let y = x + x
    t[id].val = y - x
}";

const CMP_ASSERT_SOURCE: &str = "\
table t { val: u64 }
tx op(id: u64) {
    let x = t[id].val
    let y = x + x
    assert y >= x
    t[id].val = y - x
}";

pub fn touch_accounts_source() -> &'static str {
    TOUCH_ACCOUNTS_SOURCE
}

pub fn transfer_balances_source() -> &'static str {
    TRANSFER_BALANCES_SOURCE
}

pub fn transfer_with_emit_source() -> &'static str {
    TRANSFER_EXAMPLE_TAB_SOURCE
}

pub fn peek_accounts_source() -> &'static str {
    PEEK_ACCOUNTS_SOURCE
}

pub fn shielded_peek_source() -> &'static str {
    SHIELDED_PEEK_SOURCE
}

pub fn liquid_shielded_bump_source() -> &'static str {
    LIQUID_SHIELDED_BUMP_SOURCE
}

pub fn arith_add_sub_source() -> &'static str {
    ARITH_ADD_SUB_SOURCE
}

pub fn cmp_assert_source() -> &'static str {
    CMP_ASSERT_SOURCE
}

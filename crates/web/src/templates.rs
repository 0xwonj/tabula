use tabula_types::u64_portable;

use crate::models::{State, StateEntry, TransactionBatch, TransactionInput, WorkspaceDoc};

pub struct ScenarioTemplate {
    pub id: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub program_source: &'static str,
    pub state: State,
    pub batch: TransactionBatch,
}

pub fn built_in_templates() -> Vec<ScenarioTemplate> {
    vec![
        ScenarioTemplate {
            id: "token-transfer",
            title: "Token Transfer",
            description: "3개 계정 잔액 이동과 이벤트 emit 흐름",
            program_source: "table balances {\n    balance: u64,\n}\n\ntx transfer(from: u64, to: u64, amount: u64) {\n    let sender_bal = balances[from].balance\n    let recv_bal = balances[to].balance\n    assert sender_bal >= amount\n    balances[from].balance = sender_bal - amount\n    balances[to].balance = recv_bal + amount\n    emit \"transfer\" (from, to, amount)\n}\n",
            state: State {
                cells: vec![
                    StateEntry {
                        table: 0,
                        row: 0,
                        col: 0,
                        value: Some(u64_portable(1000)),
                    },
                    StateEntry {
                        table: 0,
                        row: 1,
                        col: 0,
                        value: Some(u64_portable(500)),
                    },
                    StateEntry {
                        table: 0,
                        row: 2,
                        col: 0,
                        value: Some(u64_portable(200)),
                    },
                ],
            },
            batch: TransactionBatch {
                transactions: vec![
                    TransactionInput {
                        tx_type: 0,
                        params: vec![u64_portable(0), u64_portable(1), u64_portable(300)],
                    },
                    TransactionInput {
                        tx_type: 0,
                        params: vec![u64_portable(1), u64_portable(2), u64_portable(200)],
                    },
                    TransactionInput {
                        tx_type: 0,
                        params: vec![u64_portable(2), u64_portable(0), u64_portable(50)],
                    },
                ],
            },
        },
        ScenarioTemplate {
            id: "fail-fast",
            title: "Insufficient Balance Fail",
            description: "첫 tx에서 assert 실패를 유도해 진단/trace 확인",
            program_source: "table balances {\n    balance: u64,\n}\n\ntx spend(account: u64, amount: u64) {\n    let bal = balances[account].balance\n    assert bal >= amount\n    balances[account].balance = bal - amount\n}\n",
            state: State {
                cells: vec![StateEntry {
                    table: 0,
                    row: 0,
                    col: 0,
                    value: Some(u64_portable(10)),
                }],
            },
            batch: TransactionBatch {
                transactions: vec![TransactionInput {
                    tx_type: 0,
                    params: vec![u64_portable(0), u64_portable(99)],
                }],
            },
        },
    ]
}

pub fn template_workspace(id: &str) -> Option<WorkspaceDoc> {
    let template = built_in_templates().into_iter().find(|t| t.id == id)?;
    let mut ws = WorkspaceDoc::defaults();
    ws.program_source = template.program_source.to_string();
    ws.state_json = serde_json::to_string_pretty(&template.state).ok()?;
    ws.batch_json = serde_json::to_string_pretty(&template.batch).ok()?;
    Some(ws)
}

pub fn default_workspace() -> WorkspaceDoc {
    template_workspace("token-transfer").unwrap_or_else(WorkspaceDoc::defaults)
}

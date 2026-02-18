use tabula_core::ProgramBudgets;
use tabula_proof::statement::ApplyBatchStatement;

#[test]
fn test_statement_construction() {
    let stmt = ApplyBatchStatement {
        old_state_root: [0u8; 32],
        new_state_root: [1u8; 32],
        program_root: [2u8; 32],
        applied_tx_digest: [3u8; 32],
        static_table_root: [4u8; 32],
        budgets: ProgramBudgets {
            max_ops: 1000,
            max_slots: 256,
            max_accesses: 500,
        },
    };
    assert_ne!(stmt.old_state_root, stmt.new_state_root);
    assert_eq!(stmt.budgets.max_ops, 1000);
}

#![allow(missing_docs)]
use tabula_contract::PublicInputs;
use tabula_core::ProgramBudgets;

#[test]
fn test_public_inputs_construction() {
    let inputs = PublicInputs {
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
    assert_ne!(inputs.old_state_root, inputs.new_state_root);
    assert_eq!(inputs.budgets.max_ops, 1000);
}

use super::*;

#[test]
fn inline_functions_eliminates_function_call_ops() {
    let mut program = base_program();
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "helper".into(),
        kind: CallableKind::Function,
        params: vec![ir::ParamDecl {
            id: ir::ParamId(0),
            symbol: "flag".into(),
            ty: TYPE_BOOL_ID,
        }],
        returns: vec![TYPE_BOOL_ID],
        body: Body {
            locals: vec![LocalDecl {
                id: ir::LocalId(0),
                symbol: None,
                ty: TYPE_BOOL_ID,
            }],
            region: Region {
                ops: vec![Op::BindValue {
                    dst: ir::LocalId(0),
                    value: ValueOp::Not {
                        src: ir::ValueRef::Param(ir::ParamId(0)),
                    },
                }],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(0))]),
                },
            },
        },
    });
    program.callables.push(Callable {
        id: CallableId(2),
        symbol: "q".into(),
        kind: CallableKind::Query,
        params: vec![ir::ParamDecl {
            id: ir::ParamId(0),
            symbol: "flag".into(),
            ty: TYPE_BOOL_ID,
        }],
        returns: vec![TYPE_BOOL_ID],
        body: Body {
            locals: vec![LocalDecl {
                id: ir::LocalId(0),
                symbol: None,
                ty: TYPE_BOOL_ID,
            }],
            region: Region {
                ops: vec![Op::CallFunction {
                    callee: CallableId(1),
                    inputs: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(0))]),
                    dsts: vec![ir::LocalId(0)],
                }],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(0))]),
                },
            },
        },
    });

    let analyzed = analyze_program(verify_program(program).expect("verified")).expect("analyzed");
    let inlined = inline_functions(&analyzed).expect("inlined");
    assert!(
        inlined
            .program()
            .callables
            .iter()
            .all(|callable| callable.kind != CallableKind::Function)
    );
    assert!(
        inlined
            .program()
            .callables
            .iter()
            .all(|callable| !region_contains_call_function(&callable.body.region))
    );
}

#[test]
fn lower_eliminates_function_calls_and_validates_canonical_output() {
    let mut program = base_program();
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "helper".into(),
        kind: CallableKind::Function,
        params: vec![ir::ParamDecl {
            id: ir::ParamId(0),
            symbol: "x".into(),
            ty: TYPE_BOOL_ID,
        }],
        returns: vec![TYPE_BOOL_ID],
        body: Body {
            locals: vec![LocalDecl {
                id: ir::LocalId(0),
                symbol: None,
                ty: TYPE_BOOL_ID,
            }],
            region: Region {
                ops: vec![Op::BindValue {
                    dst: ir::LocalId(0),
                    value: ValueOp::Not {
                        src: ir::ValueRef::Param(ir::ParamId(0)),
                    },
                }],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(0))]),
                },
            },
        },
    });
    program.callables.push(Callable {
        id: CallableId(2),
        symbol: "q".into(),
        kind: CallableKind::Query,
        params: vec![ir::ParamDecl {
            id: ir::ParamId(0),
            symbol: "flag".into(),
            ty: TYPE_BOOL_ID,
        }],
        returns: vec![TYPE_BOOL_ID],
        body: Body {
            locals: vec![LocalDecl {
                id: ir::LocalId(0),
                symbol: None,
                ty: TYPE_BOOL_ID,
            }],
            region: Region {
                ops: vec![Op::CallFunction {
                    callee: CallableId(1),
                    inputs: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(0))]),
                    dsts: vec![ir::LocalId(0)],
                }],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(0))]),
                },
            },
        },
    });

    let analyzed = analyze_program(verify_program(program).expect("verified")).expect("analyzed");
    let inlined = inline_functions(&analyzed).expect("inlined");
    let canonicalized = canonicalize_program(&inlined).expect("canonicalized");
    let analyzed = analyze_program(canonicalized).expect("reanalyzed");
    let canonical = lower_to_canonical(&analyzed).expect("lower to canonical");
    let validated = ir::ValidatedProgram::try_from(canonical).expect("validated canonical");
    let runtime_program =
        RuntimeProgram::from_validated_program(validated).expect("runtime program");
    assert_eq!(runtime_program.execution().program().entries.len(), 1);
}

#[test]
fn lower_if_and_match_value_regions() {
    let mut program = base_program();
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "q".into(),
        kind: CallableKind::Query,
        params: vec![
            ir::ParamDecl {
                id: ir::ParamId(0),
                symbol: "flag".into(),
                ty: TYPE_BOOL_ID,
            },
            ir::ParamDecl {
                id: ir::ParamId(1),
                symbol: "tag".into(),
                ty: TYPE_U64_ID,
            },
        ],
        returns: vec![TYPE_BOOL_ID, TYPE_BOOL_ID],
        body: Body {
            locals: vec![
                LocalDecl {
                    id: ir::LocalId(0),
                    symbol: None,
                    ty: TYPE_BOOL_ID,
                },
                LocalDecl {
                    id: ir::LocalId(1),
                    symbol: None,
                    ty: TYPE_BOOL_ID,
                },
                LocalDecl {
                    id: ir::LocalId(2),
                    symbol: None,
                    ty: TYPE_BOOL_ID,
                },
            ],
            region: Region {
                ops: vec![
                    Op::If {
                        dsts: vec![ir::LocalId(0)],
                        cond: ir::ValueRef::Param(ir::ParamId(0)),
                        then_region: Region {
                            ops: vec![Op::BindValue {
                                dst: ir::LocalId(1),
                                value: ValueOp::Not {
                                    src: ir::ValueRef::Param(ir::ParamId(0)),
                                },
                            }],
                            terminator: Terminator::Yield {
                                values: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(
                                    1,
                                ))]),
                            },
                        },
                        else_region: Region {
                            ops: vec![],
                            terminator: Terminator::Yield {
                                values: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(
                                    0,
                                ))]),
                            },
                        },
                    },
                    Op::Match {
                        dsts: vec![ir::LocalId(2)],
                        scrutinee: ir::ValueRef::Param(ir::ParamId(1)),
                        arms: vec![MatchArm {
                            pattern: MatchPattern::Literal(u64_lit(0)),
                            region: Region {
                                ops: vec![],
                                terminator: Terminator::Yield {
                                    values: ir::ValueTupleRef(vec![ir::ValueRef::Literal(
                                        bool_lit(true),
                                    )]),
                                },
                            },
                        }],
                        default: Some(Region {
                            ops: vec![],
                            terminator: Terminator::Yield {
                                values: ir::ValueTupleRef(vec![ir::ValueRef::Literal(bool_lit(
                                    false,
                                ))]),
                            },
                        }),
                    },
                ],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![
                        ir::ValueRef::Local(ir::LocalId(0)),
                        ir::ValueRef::Local(ir::LocalId(2)),
                    ]),
                },
            },
        },
    });

    let analyzed = analyze_program(verify_program(program).expect("verified")).expect("analyzed");
    let inlined = inline_functions(&analyzed).expect("inlined");
    let canonicalized = canonicalize_program(&inlined).expect("canonicalized");
    let analyzed = analyze_program(canonicalized).expect("reanalyzed");
    let canonical = lower_to_canonical(&analyzed).expect("lower");
    assert!(
        canonical.entries[0]
            .body
            .ops
            .iter()
            .any(|op| matches!(op, ir::Op::Select { .. }))
    );
    ir::ValidatedProgram::try_from(canonical).expect("validated canonical");
}

#[test]
fn lower_match_catch_all_respects_outer_guard() {
    let mut program = base_program();
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "tx".into(),
        kind: CallableKind::Tx,
        params: vec![
            ir::ParamDecl {
                id: ir::ParamId(0),
                symbol: "flag".into(),
                ty: TYPE_BOOL_ID,
            },
            ir::ParamDecl {
                id: ir::ParamId(1),
                symbol: "tag".into(),
                ty: TYPE_U64_ID,
            },
        ],
        returns: vec![],
        body: Body {
            locals: vec![],
            region: Region {
                ops: vec![Op::If {
                    dsts: vec![],
                    cond: ir::ValueRef::Param(ir::ParamId(0)),
                    then_region: Region {
                        ops: vec![Op::Match {
                            dsts: vec![],
                            scrutinee: ir::ValueRef::Param(ir::ParamId(1)),
                            arms: vec![MatchArm {
                                pattern: MatchPattern::Literal(u64_lit(0)),
                                region: Region {
                                    ops: vec![Op::EmitEvent {
                                        event: ir::EventId(1),
                                        args: ir::ValueTupleRef(vec![ir::ValueRef::Literal(
                                            bool_lit(true),
                                        )]),
                                    }],
                                    terminator: Terminator::Yield {
                                        values: ir::ValueTupleRef(vec![]),
                                    },
                                },
                            }],
                            default: Some(Region {
                                ops: vec![Op::EmitEvent {
                                    event: ir::EventId(1),
                                    args: ir::ValueTupleRef(vec![ir::ValueRef::Literal(bool_lit(
                                        false,
                                    ))]),
                                }],
                                terminator: Terminator::Yield {
                                    values: ir::ValueTupleRef(vec![]),
                                },
                            }),
                        }],
                        terminator: Terminator::Yield {
                            values: ir::ValueTupleRef(vec![]),
                        },
                    },
                    else_region: Region {
                        ops: vec![],
                        terminator: Terminator::Yield {
                            values: ir::ValueTupleRef(vec![]),
                        },
                    },
                }],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![]),
                },
            },
        },
    });

    let analyzed = analyze_program(verify_program(program).expect("verified")).expect("analyzed");
    let inlined = inline_functions(&analyzed).expect("inlined");
    let canonicalized = canonicalize_program(&inlined).expect("canonicalized");
    let analyzed = analyze_program(canonicalized).expect("reanalyzed");
    let canonical = lower_to_canonical(&analyzed).expect("lower");
    let validated = ir::ValidatedProgram::try_from(canonical).expect("validated");
    let runtime_program = RuntimeProgram::from_validated_program(validated).expect("runtime");
    let runtimes = TypeRuntimeRegistry::seeded().expect("runtimes");
    let state_runtime = IrStateRuntime {
        program: runtime_program.execution().program(),
    };
    let exec_ctx = exec::ExecContext {
        hasher: &Blake3Hasher,
        type_runtimes: &runtimes,
        capability_executor: None,
        state_runtime: &state_runtime,
    };
    let state = InMemoryState::new();
    let context = exec::ContextValues::new();
    let journal = exec::execute_batch(
        runtime_program.execution(),
        &[exec::TxCall {
            entry_id: ir::EntryId(1),
            params: vec![bool_typed(true), u64_typed(0)],
        }],
        &context,
        &state,
        &exec_ctx,
    )
    .expect("batch executes");

    let tx = journal.successful_txs().next().expect("successful tx");
    assert_eq!(tx.event_effects.len(), 1);
    assert_eq!(tx.event_effects[0].event, ir::EventId(1));
    assert_eq!(tx.event_effects[0].args.len(), 1);
    assert_eq!(tx.event_effects[0].args[0], bool_typed(true));
}

#[test]
fn lower_if_keeps_untaken_checked_and_effectful_branch_inactive() {
    let mut program = base_program();
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "tx".into(),
        kind: CallableKind::Tx,
        params: vec![ir::ParamDecl {
            id: ir::ParamId(0),
            symbol: "flag".into(),
            ty: TYPE_BOOL_ID,
        }],
        returns: vec![],
        body: Body {
            locals: vec![
                LocalDecl {
                    id: ir::LocalId(0),
                    symbol: None,
                    ty: TYPE_U64_ID,
                },
                LocalDecl {
                    id: ir::LocalId(1),
                    symbol: None,
                    ty: TYPE_U64_ID,
                },
                LocalDecl {
                    id: ir::LocalId(2),
                    symbol: None,
                    ty: TYPE_U64_ID,
                },
            ],
            region: Region {
                ops: vec![Op::If {
                    dsts: vec![],
                    cond: ir::ValueRef::Param(ir::ParamId(0)),
                    then_region: Region {
                        ops: vec![
                            Op::Assert {
                                cond: ir::ValueRef::Literal(bool_lit(false)),
                            },
                            Op::DivMod {
                                dst_q: ir::LocalId(0),
                                dst_r: ir::LocalId(1),
                                lhs: ir::ValueRef::Literal(u64_lit(7)),
                                rhs: ir::ValueRef::Literal(u64_lit(0)),
                            },
                            Op::CallCapability {
                                capability: ir::CapabilityId(1),
                                inputs: ir::ValueTupleRef(vec![ir::ValueRef::Literal(u64_lit(9))]),
                                dsts: vec![ir::LocalId(2)],
                            },
                        ],
                        terminator: Terminator::Yield {
                            values: ir::ValueTupleRef(vec![]),
                        },
                    },
                    else_region: Region {
                        ops: vec![],
                        terminator: Terminator::Yield {
                            values: ir::ValueTupleRef(vec![]),
                        },
                    },
                }],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![]),
                },
            },
        },
    });

    let analyzed = analyze_program(verify_program(program).expect("verified")).expect("analyzed");
    let inlined = inline_functions(&analyzed).expect("inlined");
    let canonicalized = canonicalize_program(&inlined).expect("canonicalized");
    let analyzed = analyze_program(canonicalized).expect("reanalyzed");
    let canonical = lower_to_canonical(&analyzed).expect("lower");
    let validated = ir::ValidatedProgram::try_from(canonical).expect("validated");
    let runtime_program = RuntimeProgram::from_validated_program(validated).expect("runtime");
    let runtimes = TypeRuntimeRegistry::seeded().expect("runtimes");
    let state_runtime = IrStateRuntime {
        program: runtime_program.execution().program(),
    };
    let exec_ctx = exec::ExecContext {
        hasher: &Blake3Hasher,
        type_runtimes: &runtimes,
        capability_executor: None,
        state_runtime: &state_runtime,
    };
    let journal = exec::execute_batch(
        runtime_program.execution(),
        &[exec::TxCall {
            entry_id: ir::EntryId(1),
            params: vec![bool_typed(false)],
        }],
        &exec::ContextValues::new(),
        &InMemoryState::new(),
        &exec_ctx,
    )
    .expect("untaken guarded branch should stay inactive");

    let tx = journal.successful_txs().next().expect("successful tx");
    assert!(tx.state_effects.is_empty());
    assert!(tx.capability_effects.is_empty());
    assert!(tx.event_effects.is_empty());
}

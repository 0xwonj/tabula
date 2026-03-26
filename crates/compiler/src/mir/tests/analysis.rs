use super::*;

#[test]
fn analyze_summaries_split_effect_failure_and_policy_axes() {
    let mut program = base_program();
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "helper".into(),
        kind: CallableKind::Function,
        params: vec![ir::ParamDecl {
            id: ir::ParamId(0),
            symbol: "x".into(),
            ty: TYPE_U64_ID,
        }],
        returns: vec![TYPE_U64_ID],
        body: Body {
            locals: vec![
                LocalDecl {
                    id: ir::LocalId(0),
                    symbol: Some("h".into()),
                    ty: TYPE_BYTES32_ID,
                },
                LocalDecl {
                    id: ir::LocalId(1),
                    symbol: Some("y".into()),
                    ty: TYPE_U64_ID,
                },
                LocalDecl {
                    id: ir::LocalId(2),
                    symbol: Some("z".into()),
                    ty: TYPE_U64_ID,
                },
            ],
            region: Region {
                ops: vec![
                    Op::BindValue {
                        dst: ir::LocalId(0),
                        value: ValueOp::Hash {
                            family: ir::HashFamily::Poseidon,
                            inputs: ir::ValueTupleRef(vec![ir::ValueRef::Literal(u64_lit(7))]),
                        },
                    },
                    Op::CallCapability {
                        capability: ir::CapabilityId(1),
                        inputs: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(0))]),
                        dsts: vec![ir::LocalId(1)],
                    },
                    Op::CallCapability {
                        capability: ir::CapabilityId(2),
                        inputs: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(1))]),
                        dsts: vec![ir::LocalId(2)],
                    },
                ],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(2))]),
                },
            },
        },
    });

    let analyzed = analyze_program(verify_program(program).expect("verified")).expect("analyzed");
    let effect = analyzed
        .effect_summary(CallableId(1))
        .expect("effect summary");
    let failure = analyzed
        .failure_summary(CallableId(1))
        .expect("failure summary");
    let policy = analyzed
        .policy_summary(CallableId(1))
        .expect("policy summary");
    assert!(effect.proof.capability_call);
    assert!(!effect.world.state_write);
    assert!(failure.semantic_may_fail);
    assert!(failure.host_contract_sensitive);
    assert!(policy.uses_builtin_hash);
    assert!(policy.uses_query_safe_capability);
    assert!(policy.uses_journaled_capability);
    assert!(policy.uses_opaque_runtime_capability);
    assert!(!policy.uses_tx_only_capability);
}

#[test]
fn analyze_marks_read_only_helper_query_legal_from_summary() {
    let mut program = base_program();
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "helper".into(),
        kind: CallableKind::Function,
        params: vec![ir::ParamDecl {
            id: ir::ParamId(0),
            symbol: "x".into(),
            ty: TYPE_U64_ID,
        }],
        returns: vec![TYPE_U64_ID],
        body: Body {
            locals: vec![LocalDecl {
                id: ir::LocalId(0),
                symbol: None,
                ty: TYPE_U64_ID,
            }],
            region: Region {
                ops: vec![Op::CallCapability {
                    capability: ir::CapabilityId(1),
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
    assert_eq!(analyzed.query_legal(CallableId(1)), Some(true));
}

#[test]
fn analyze_rejects_query_calling_tx_only_capability_through_function() {
    let mut program = base_program();
    program
        .capability_manifest
        .entries
        .push(ir::CapabilityDescriptor {
            id: ir::CapabilityId(3),
            symbol: "tx_only".into(),
            inputs: vec![TYPE_U64_ID],
            outputs: vec![TYPE_U64_ID],
            totality: ir::CapabilityTotality::Checked,
            query_policy: ir::CapabilityQueryPolicy::TxOnly,
            proof_visibility: ir::CapabilityProofVisibility::Journaled,
        });
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "helper".into(),
        kind: CallableKind::Function,
        params: vec![ir::ParamDecl {
            id: ir::ParamId(0),
            symbol: "x".into(),
            ty: TYPE_U64_ID,
        }],
        returns: vec![TYPE_U64_ID],
        body: Body {
            locals: vec![LocalDecl {
                id: ir::LocalId(0),
                symbol: None,
                ty: TYPE_U64_ID,
            }],
            region: Region {
                ops: vec![Op::CallCapability {
                    capability: ir::CapabilityId(3),
                    inputs: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(0))]),
                    dsts: vec![ir::LocalId(0)],
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
            symbol: "x".into(),
            ty: TYPE_U64_ID,
        }],
        returns: vec![TYPE_U64_ID],
        body: Body {
            locals: vec![LocalDecl {
                id: ir::LocalId(0),
                symbol: None,
                ty: TYPE_U64_ID,
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

    let verified = verify_program(program).expect("verified");
    assert!(analyze_program(verified).is_err());
}

#[test]
fn analyze_tracks_context_demands_transitively() {
    let mut program = base_program();
    program.context.fields = vec![ir::ContextField {
        id: ir::ContextFieldId(7),
        symbol: "caller".into(),
        ty: TYPE_U64_ID,
    }];
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "helper".into(),
        kind: CallableKind::Function,
        params: vec![],
        returns: vec![TYPE_U64_ID],
        body: Body {
            locals: vec![],
            region: Region {
                ops: vec![],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![ir::ValueRef::Context(ir::ContextFieldId(7))]),
                },
            },
        },
    });
    program.callables.push(Callable {
        id: CallableId(2),
        symbol: "q".into(),
        kind: CallableKind::Query,
        params: vec![],
        returns: vec![TYPE_U64_ID],
        body: Body {
            locals: vec![LocalDecl {
                id: ir::LocalId(0),
                symbol: None,
                ty: TYPE_U64_ID,
            }],
            region: Region {
                ops: vec![Op::CallFunction {
                    callee: CallableId(1),
                    inputs: ir::ValueTupleRef(vec![]),
                    dsts: vec![ir::LocalId(0)],
                }],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(0))]),
                },
            },
        },
    });

    let analyzed = analyze_program(verify_program(program).expect("verified")).expect("analyzed");
    let helper = analyzed
        .context_demand_summary(CallableId(1))
        .expect("helper context demand");
    let query = analyzed
        .context_demand_summary(CallableId(2))
        .expect("query context demand");
    assert_eq!(helper.fields, BTreeSet::from([ir::ContextFieldId(7)]));
    assert_eq!(query.fields, BTreeSet::from([ir::ContextFieldId(7)]));
}

#[test]
fn analyze_unions_context_demands_across_if_regions() {
    let mut program = base_program();
    program.context.fields = vec![
        ir::ContextField {
            id: ir::ContextFieldId(1),
            symbol: "left".into(),
            ty: TYPE_BOOL_ID,
        },
        ir::ContextField {
            id: ir::ContextFieldId(2),
            symbol: "right".into(),
            ty: TYPE_BOOL_ID,
        },
    ];
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "helper".into(),
        kind: CallableKind::Function,
        params: vec![],
        returns: vec![],
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
            ],
            region: Region {
                ops: vec![Op::If {
                    dsts: vec![],
                    cond: ir::ValueRef::Literal(bool_lit(true)),
                    then_region: Region {
                        ops: vec![Op::BindValue {
                            dst: ir::LocalId(0),
                            value: ValueOp::Not {
                                src: ir::ValueRef::Context(ir::ContextFieldId(1)),
                            },
                        }],
                        terminator: Terminator::Yield {
                            values: ir::ValueTupleRef(vec![]),
                        },
                    },
                    else_region: Region {
                        ops: vec![Op::BindValue {
                            dst: ir::LocalId(1),
                            value: ValueOp::Not {
                                src: ir::ValueRef::Context(ir::ContextFieldId(2)),
                            },
                        }],
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
    let summary = analyzed
        .context_demand_summary(CallableId(1))
        .expect("context demand");
    assert_eq!(
        summary.fields,
        BTreeSet::from([ir::ContextFieldId(1), ir::ContextFieldId(2)])
    );
}

#[test]
fn analyze_rejects_query_calling_event_emitting_function() {
    let mut program = base_program();
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "helper".into(),
        kind: CallableKind::Function,
        params: vec![],
        returns: vec![],
        body: Body {
            locals: vec![],
            region: Region {
                ops: vec![Op::EmitEvent {
                    event: ir::EventId(1),
                    args: ir::ValueTupleRef(vec![ir::ValueRef::Literal(bool_lit(true))]),
                }],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![]),
                },
            },
        },
    });
    program.callables.push(Callable {
        id: CallableId(2),
        symbol: "q".into(),
        kind: CallableKind::Query,
        params: vec![],
        returns: vec![],
        body: Body {
            locals: vec![],
            region: Region {
                ops: vec![Op::CallFunction {
                    callee: CallableId(1),
                    inputs: ir::ValueTupleRef(vec![]),
                    dsts: vec![],
                }],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![]),
                },
            },
        },
    });

    let verified = verify_program(program).expect("verified");
    assert!(analyze_program(verified).is_err());
}

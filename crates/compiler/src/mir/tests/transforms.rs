use super::*;

#[test]
fn canonicalize_folds_literals_and_removes_dead_pure_locals() {
    let mut program = base_program();
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "q".into(),
        kind: CallableKind::Query,
        params: vec![ir::ParamDecl {
            id: ir::ParamId(0),
            symbol: "flag".into(),
            ty: TYPE_BOOL_ID,
        }],
        returns: vec![TYPE_BOOL_ID],
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
                    Op::BindValue {
                        dst: ir::LocalId(0),
                        value: ValueOp::Arith {
                            op: ir::ArithOp::Add,
                            lhs: ir::ValueRef::Literal(u64_lit(1)),
                            rhs: ir::ValueRef::Literal(u64_lit(2)),
                        },
                    },
                    Op::BindValue {
                        dst: ir::LocalId(1),
                        value: ValueOp::Select {
                            cond: ir::ValueRef::Param(ir::ParamId(0)),
                            if_true: ir::ValueRef::Literal(bool_lit(true)),
                            if_false: ir::ValueRef::Literal(bool_lit(true)),
                        },
                    },
                    Op::BindValue {
                        dst: ir::LocalId(2),
                        value: ValueOp::Select {
                            cond: ir::ValueRef::Literal(bool_lit(false)),
                            if_true: ir::ValueRef::Literal(bool_lit(true)),
                            if_false: ir::ValueRef::Local(ir::LocalId(1)),
                        },
                    },
                ],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(2))]),
                },
            },
        },
    });

    let verified = verify_program(program).expect("verified");
    let canonicalized = canonicalize_program(&verified).expect("canonicalized");
    let callable = &canonicalized.program().callables[0];
    assert!(callable.body.locals.is_empty());
    assert!(callable.body.region.ops.is_empty());
    assert_eq!(
        callable.body.region.terminator,
        Terminator::Return {
            values: ir::ValueTupleRef(vec![ir::ValueRef::Literal(bool_lit(true))]),
        }
    );
}

#[test]
fn canonicalize_keeps_effectful_ops_even_when_results_are_unused() {
    let mut program = base_program();
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "q".into(),
        kind: CallableKind::Query,
        params: vec![ir::ParamDecl {
            id: ir::ParamId(0),
            symbol: "id".into(),
            ty: TYPE_U64_ID,
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
                    ty: TYPE_BOOL_ID,
                },
            ],
            region: Region {
                ops: vec![Op::ReadState {
                    dst_value: ir::LocalId(0),
                    dst_present: ir::LocalId(1),
                    table: ir::TableId(1),
                    key: ir::ValueTupleRef(vec![ir::ValueRef::Param(ir::ParamId(0))]),
                    field: ir::FieldId(0),
                }],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![]),
                },
            },
        },
    });

    let verified = verify_program(program).expect("verified");
    let canonicalized = canonicalize_program(&verified).expect("canonicalized");
    let callable = &canonicalized.program().callables[0];
    assert_eq!(callable.body.locals.len(), 2);
    assert!(matches!(callable.body.region.ops[0], Op::ReadState { .. }));
}

#[test]
fn canonicalize_removes_dead_pure_ops_exposed_by_inlining() {
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
                ops: vec![Op::BindValue {
                    dst: ir::LocalId(0),
                    value: ValueOp::Arith {
                        op: ir::ArithOp::Add,
                        lhs: ir::ValueRef::Param(ir::ParamId(0)),
                        rhs: ir::ValueRef::Literal(u64_lit(1)),
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
        symbol: "tx".into(),
        kind: CallableKind::Tx,
        params: vec![ir::ParamDecl {
            id: ir::ParamId(0),
            symbol: "x".into(),
            ty: TYPE_U64_ID,
        }],
        returns: vec![],
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
                    values: ir::ValueTupleRef(vec![]),
                },
            },
        },
    });

    let analyzed = analyze_program(verify_program(program).expect("verified")).expect("analyzed");
    let inlined = inline_functions(&analyzed).expect("inlined");
    let before_ops = inlined.program().callables[0].body.region.ops.len();
    let canonicalized = canonicalize_program(&inlined).expect("canonicalized");
    let callable = &canonicalized.program().callables[0];
    assert!(before_ops > callable.body.region.ops.len());
    assert!(callable.body.region.ops.is_empty());
    assert!(callable.body.locals.is_empty());
}

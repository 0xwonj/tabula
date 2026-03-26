use super::*;

#[test]
fn verify_rejects_else_using_then_local() {
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
                    dsts: vec![ir::LocalId(1)],
                    cond: ir::ValueRef::Param(ir::ParamId(0)),
                    then_region: Region {
                        ops: vec![Op::BindValue {
                            dst: ir::LocalId(0),
                            value: ValueOp::Not {
                                src: ir::ValueRef::Param(ir::ParamId(0)),
                            },
                        }],
                        terminator: Terminator::Yield {
                            values: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(0))]),
                        },
                    },
                    else_region: Region {
                        ops: vec![],
                        terminator: Terminator::Yield {
                            values: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(0))]),
                        },
                    },
                }],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(1))]),
                },
            },
        },
    });

    assert!(verify_program(program).is_err());
}

#[test]
fn verify_rejects_match_arm_using_sibling_local() {
    let mut program = base_program();
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "q".into(),
        kind: CallableKind::Query,
        params: vec![ir::ParamDecl {
            id: ir::ParamId(0),
            symbol: "tag".into(),
            ty: TYPE_U64_ID,
        }],
        returns: vec![TYPE_BOOL_ID],
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
                ops: vec![Op::Match {
                    dsts: vec![ir::LocalId(1)],
                    scrutinee: ir::ValueRef::Param(ir::ParamId(0)),
                    arms: vec![
                        MatchArm {
                            pattern: MatchPattern::Literal(u64_lit(0)),
                            region: Region {
                                ops: vec![Op::BindValue {
                                    dst: ir::LocalId(0),
                                    value: ValueOp::Not {
                                        src: ir::ValueRef::Literal(bool_lit(true)),
                                    },
                                }],
                                terminator: Terminator::Yield {
                                    values: ir::ValueTupleRef(vec![ir::ValueRef::Local(
                                        ir::LocalId(0),
                                    )]),
                                },
                            },
                        },
                        MatchArm {
                            pattern: MatchPattern::Wildcard,
                            region: Region {
                                ops: vec![],
                                terminator: Terminator::Yield {
                                    values: ir::ValueTupleRef(vec![ir::ValueRef::Local(
                                        ir::LocalId(0),
                                    )]),
                                },
                            },
                        },
                    ],
                    default: None,
                }],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![ir::ValueRef::Local(ir::LocalId(1))]),
                },
            },
        },
    });

    assert!(verify_program(program).is_err());
}

#[test]
fn verify_rejects_else_region_using_local_defined_only_in_then() {
    let mut program = base_program();
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "tx".into(),
        kind: CallableKind::Tx,
        params: vec![],
        returns: vec![],
        body: Body {
            locals: vec![LocalDecl {
                id: ir::LocalId(0),
                symbol: Some("branch_only".into()),
                ty: TYPE_BOOL_ID,
            }],
            region: Region {
                ops: vec![Op::If {
                    dsts: vec![],
                    cond: ir::ValueRef::Literal(bool_lit(true)),
                    then_region: Region {
                        ops: vec![Op::BindValue {
                            dst: ir::LocalId(0),
                            value: ValueOp::Not {
                                src: ir::ValueRef::Literal(bool_lit(false)),
                            },
                        }],
                        terminator: Terminator::Yield {
                            values: ir::ValueTupleRef(vec![]),
                        },
                    },
                    else_region: Region {
                        ops: vec![Op::Assert {
                            cond: ir::ValueRef::Local(ir::LocalId(0)),
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

    let err = verify_program(program).expect_err("verification should fail");
    assert!(
        err.to_string().contains("used before definition"),
        "unexpected error: {err}"
    );
}

#[test]
fn verify_rejects_match_arm_using_local_defined_only_in_sibling_arm() {
    let mut program = base_program();
    program.callables.push(Callable {
        id: CallableId(1),
        symbol: "tx".into(),
        kind: CallableKind::Tx,
        params: vec![],
        returns: vec![],
        body: Body {
            locals: vec![LocalDecl {
                id: ir::LocalId(0),
                symbol: Some("branch_only".into()),
                ty: TYPE_BOOL_ID,
            }],
            region: Region {
                ops: vec![Op::Match {
                    dsts: vec![],
                    scrutinee: ir::ValueRef::Literal(u64_lit(0)),
                    arms: vec![
                        MatchArm {
                            pattern: MatchPattern::Literal(u64_lit(0)),
                            region: Region {
                                ops: vec![Op::BindValue {
                                    dst: ir::LocalId(0),
                                    value: ValueOp::Not {
                                        src: ir::ValueRef::Literal(bool_lit(false)),
                                    },
                                }],
                                terminator: Terminator::Yield {
                                    values: ir::ValueTupleRef(vec![]),
                                },
                            },
                        },
                        MatchArm {
                            pattern: MatchPattern::Literal(u64_lit(1)),
                            region: Region {
                                ops: vec![Op::Assert {
                                    cond: ir::ValueRef::Local(ir::LocalId(0)),
                                }],
                                terminator: Terminator::Yield {
                                    values: ir::ValueTupleRef(vec![]),
                                },
                            },
                        },
                    ],
                    default: None,
                }],
                terminator: Terminator::Return {
                    values: ir::ValueTupleRef(vec![]),
                },
            },
        },
    });

    let err = verify_program(program).expect_err("verification should fail");
    assert!(
        err.to_string().contains("used before definition"),
        "unexpected error: {err}"
    );
}

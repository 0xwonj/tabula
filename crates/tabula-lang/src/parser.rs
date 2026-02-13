//! Recursive descent parser with Pratt expression parsing.
//!
//! Converts a token stream into an AST.

use crate::ast::*;
use crate::error::{CompileError, ErrorKind};
use crate::span::Span;
use crate::token::Token;

/// Parse a token stream into a program AST.
pub fn parse(tokens: Vec<(Token, Span)>) -> Result<Program, Vec<CompileError>> {
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program();
    if parser.errors.is_empty() {
        Ok(program)
    } else {
        Err(parser.errors)
    }
}

struct Parser {
    tokens: Vec<(Token, Span)>,
    pos: usize,
    errors: Vec<CompileError>,
}

impl Parser {
    fn new(tokens: Vec<(Token, Span)>) -> Self {
        Self {
            tokens,
            pos: 0,
            errors: Vec::new(),
        }
    }

    // --- Token access ---

    fn peek(&self) -> &Token {
        &self.tokens[self.pos].0
    }

    fn peek_span(&self) -> Span {
        self.tokens[self.pos].1
    }

    fn advance(&mut self) -> (Token, Span) {
        let tok = self.tokens[self.pos].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, expected: &Token) -> Result<Span, ()> {
        if self.peek() == expected {
            let (_, span) = self.advance();
            Ok(span)
        } else {
            self.errors.push(CompileError::new(
                ErrorKind::ExpectedToken,
                self.peek_span(),
                format!("expected {:?}, found {:?}", expected, self.peek()),
            ));
            Err(())
        }
    }

    fn expect_ident(&mut self) -> Result<(String, Span), ()> {
        if let Token::Ident(_) = self.peek() {
            let (tok, span) = self.advance();
            if let Token::Ident(name) = tok {
                Ok((name, span))
            } else {
                unreachable!()
            }
        } else {
            self.errors.push(CompileError::new(
                ErrorKind::ExpectedToken,
                self.peek_span(),
                format!("expected identifier, found {:?}", self.peek()),
            ));
            Err(())
        }
    }

    fn expect_string(&mut self) -> Result<(String, Span), ()> {
        if let Token::StringLit(_) = self.peek() {
            let (tok, span) = self.advance();
            if let Token::StringLit(s) = tok {
                Ok((s, span))
            } else {
                unreachable!()
            }
        } else {
            self.errors.push(CompileError::new(
                ErrorKind::ExpectedToken,
                self.peek_span(),
                format!("expected string literal, found {:?}", self.peek()),
            ));
            Err(())
        }
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek(), Token::Eof)
    }

    // --- Top-level ---

    fn parse_program(&mut self) -> Program {
        let mut tables = Vec::new();
        let mut transactions = Vec::new();

        while !self.at_eof() {
            match self.peek() {
                Token::Table => {
                    if let Some(t) = self.parse_table_decl() {
                        tables.push(t);
                    }
                }
                Token::Tx => {
                    if let Some(t) = self.parse_tx_decl() {
                        transactions.push(t);
                    }
                }
                _ => {
                    self.errors.push(CompileError::new(
                        ErrorKind::UnexpectedToken,
                        self.peek_span(),
                        format!(
                            "expected 'table' or 'tx' at top level, found {:?}",
                            self.peek()
                        ),
                    ));
                    self.advance();
                }
            }
        }

        Program {
            tables,
            transactions,
        }
    }

    // --- Table declaration ---

    fn parse_table_decl(&mut self) -> Option<TableDecl> {
        let start = self.peek_span();
        self.advance(); // consume 'table'
        let (name, _) = self.expect_ident().ok()?;
        self.expect(&Token::LBrace).ok()?;

        let mut columns = Vec::new();
        while !matches!(self.peek(), Token::RBrace | Token::Eof) {
            if let Some(col) = self.parse_column_decl() {
                columns.push(col);
            } else {
                // Skip to next column or closing brace.
                self.skip_to_recovery(&[Token::Comma, Token::RBrace]);
            }
            // Optional comma between columns.
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            }
        }
        let end = self.peek_span();
        self.expect(&Token::RBrace).ok()?;

        Some(TableDecl {
            name,
            columns,
            span: start.merge(end),
        })
    }

    fn parse_column_decl(&mut self) -> Option<ColumnDecl> {
        let (name, start) = self.expect_ident().ok()?;
        self.expect(&Token::Colon).ok()?;
        let (ty, ty_span) = self.parse_type_name()?;
        Some(ColumnDecl {
            name,
            ty,
            span: start.merge(ty_span),
        })
    }

    fn parse_type_name(&mut self) -> Option<(TypeName, Span)> {
        let span = self.peek_span();
        let ty = match self.peek() {
            Token::U64 => TypeName::U64,
            Token::I64 => TypeName::I64,
            Token::Bool => TypeName::Bool,
            Token::Bytes32 => TypeName::Bytes32,
            _ => {
                self.errors.push(CompileError::new(
                    ErrorKind::ExpectedToken,
                    span,
                    format!(
                        "expected type (u64, i64, bool, bytes32), found {:?}",
                        self.peek()
                    ),
                ));
                return None;
            }
        };
        self.advance();
        Some((ty, span))
    }

    // --- Transaction declaration ---

    fn parse_tx_decl(&mut self) -> Option<TxDecl> {
        let start = self.peek_span();
        self.advance(); // consume 'tx'
        let (name, _) = self.expect_ident().ok()?;

        // Parameters: ( name: type, ... )
        self.expect(&Token::LParen).ok()?;
        let mut params = Vec::new();
        while !matches!(self.peek(), Token::RParen | Token::Eof) {
            if let Some(p) = self.parse_param_decl() {
                params.push(p);
            } else {
                self.skip_to_recovery(&[Token::Comma, Token::RParen]);
            }
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            }
        }
        self.expect(&Token::RParen).ok()?;

        // Body: { statements... }
        self.expect(&Token::LBrace).ok()?;
        let mut body = Vec::new();
        while !matches!(self.peek(), Token::RBrace | Token::Eof) {
            if let Some(stmt) = self.parse_stmt() {
                body.push(stmt);
            } else {
                // Skip to next statement start or closing brace.
                self.skip_to_stmt_boundary();
            }
        }
        let end = self.peek_span();
        self.expect(&Token::RBrace).ok()?;

        Some(TxDecl {
            name,
            params,
            body,
            span: start.merge(end),
        })
    }

    fn parse_param_decl(&mut self) -> Option<ParamDecl> {
        let (name, start) = self.expect_ident().ok()?;
        self.expect(&Token::Colon).ok()?;
        let (ty, ty_span) = self.parse_type_name()?;
        Some(ParamDecl {
            name,
            ty,
            span: start.merge(ty_span),
        })
    }

    // --- Statements ---

    fn parse_stmt(&mut self) -> Option<Stmt> {
        match self.peek() {
            Token::Let => self.parse_let_stmt(),
            Token::Assert => self.parse_assert_stmt(),
            Token::Emit => self.parse_emit_stmt(),
            Token::Ident(_) => self.parse_assign_stmt(),
            _ => {
                self.errors.push(CompileError::new(
                    ErrorKind::UnexpectedToken,
                    self.peek_span(),
                    format!(
                        "expected statement (let, assert, emit, or assignment), found {:?}",
                        self.peek()
                    ),
                ));
                None
            }
        }
    }

    fn parse_let_stmt(&mut self) -> Option<Stmt> {
        let start = self.peek_span();
        self.advance(); // consume 'let'

        // Check for destructuring: let (a, b) = divmod(...)
        if matches!(self.peek(), Token::LParen) {
            return self.parse_let_destructure(start);
        }

        let (name, _) = self.expect_ident().ok()?;
        self.expect(&Token::Eq).ok()?;
        let value = self.parse_expr()?;
        let span = start.merge(value.span);
        Some(Stmt {
            kind: StmtKind::Let { name, value },
            span,
        })
    }

    fn parse_let_destructure(&mut self, start: Span) -> Option<Stmt> {
        self.advance(); // consume '('
        let (first, _) = self.expect_ident().ok()?;
        self.expect(&Token::Comma).ok()?;
        let (second, _) = self.expect_ident().ok()?;
        self.expect(&Token::RParen).ok()?;
        self.expect(&Token::Eq).ok()?;

        // Must be divmod(lhs, rhs)
        if !matches!(self.peek(), Token::Divmod) {
            self.errors.push(CompileError::new(
                ErrorKind::ExpectedToken,
                self.peek_span(),
                "destructuring assignment only supported with divmod()",
            ));
            return None;
        }
        self.advance(); // consume 'divmod'
        self.expect(&Token::LParen).ok()?;
        let lhs = self.parse_expr()?;
        self.expect(&Token::Comma).ok()?;
        let rhs = self.parse_expr()?;
        let end = self.peek_span();
        self.expect(&Token::RParen).ok()?;

        Some(Stmt {
            kind: StmtKind::LetDestructure {
                first,
                second,
                lhs,
                rhs,
            },
            span: start.merge(end),
        })
    }

    fn parse_assert_stmt(&mut self) -> Option<Stmt> {
        let start = self.peek_span();
        self.advance(); // consume 'assert'
        let condition = self.parse_expr()?;
        let span = start.merge(condition.span);
        Some(Stmt {
            kind: StmtKind::Assert { condition },
            span,
        })
    }

    fn parse_emit_stmt(&mut self) -> Option<Stmt> {
        let start = self.peek_span();
        self.advance(); // consume 'emit'
        let (topic, _) = self.expect_string().ok()?;
        self.expect(&Token::LParen).ok()?;
        let mut args = Vec::new();
        while !matches!(self.peek(), Token::RParen | Token::Eof) {
            let expr = self.parse_expr()?;
            args.push(expr);
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            }
        }
        let end = self.peek_span();
        self.expect(&Token::RParen).ok()?;

        Some(Stmt {
            kind: StmtKind::Emit { topic, args },
            span: start.merge(end),
        })
    }

    fn parse_assign_stmt(&mut self) -> Option<Stmt> {
        let start = self.peek_span();
        let (table, _) = self.expect_ident().ok()?;
        self.expect(&Token::LBracket).ok()?;
        let row = self.parse_expr()?;
        self.expect(&Token::RBracket).ok()?;
        self.expect(&Token::Dot).ok()?;
        let (col, _) = self.expect_ident().ok()?;
        self.expect(&Token::Eq).ok()?;
        let value = self.parse_expr()?;
        let span = start.merge(value.span);

        Some(Stmt {
            kind: StmtKind::Assign {
                table,
                row,
                col,
                value,
            },
            span,
        })
    }

    // --- Expressions (Pratt parser) ---

    fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_expr_bp(0)
    }

    /// Pratt parser: parse expression with minimum binding power `min_bp`.
    fn parse_expr_bp(&mut self, min_bp: u8) -> Option<Expr> {
        // Parse prefix (atom or unary).
        let mut lhs = self.parse_prefix()?;

        // Parse infix operators.
        loop {
            let (op, bp) = match self.peek() {
                Token::PipePipe => (BinOp::Or, (1, 2)),
                Token::AmpAmp => (BinOp::And, (3, 4)),
                Token::EqEq => (BinOp::Eq, (5, 6)),
                Token::BangEq => (BinOp::Neq, (5, 6)),
                Token::Lt => (BinOp::Lt, (7, 8)),
                Token::LtEq => (BinOp::Lte, (7, 8)),
                Token::Gt => (BinOp::Gt, (7, 8)),
                Token::GtEq => (BinOp::Gte, (7, 8)),
                Token::Plus => (BinOp::Add, (9, 10)),
                Token::Minus => (BinOp::Sub, (9, 10)),
                Token::Star => (BinOp::Mul, (11, 12)),
                Token::Slash => (BinOp::Div, (11, 12)),
                Token::Percent => (BinOp::Mod, (11, 12)),
                _ => break,
            };

            let (l_bp, r_bp) = bp;
            if l_bp < min_bp {
                break;
            }
            self.advance(); // consume operator

            let rhs = self.parse_expr_bp(r_bp)?;
            let span = lhs.span.merge(rhs.span);
            lhs = Expr {
                kind: ExprKind::BinOp {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                span,
            };
        }

        Some(lhs)
    }

    /// Parse a prefix expression (unary ops, atoms, parenthesized exprs).
    fn parse_prefix(&mut self) -> Option<Expr> {
        match self.peek().clone() {
            // Unary negation
            Token::Minus => {
                let start = self.peek_span();
                self.advance();
                let operand = self.parse_expr_bp(13)?; // high bp for unary
                let span = start.merge(operand.span);
                Some(Expr {
                    kind: ExprKind::UnaryOp {
                        op: UnaryOp::Neg,
                        operand: Box::new(operand),
                    },
                    span,
                })
            }
            // Logical not
            Token::Bang => {
                let start = self.peek_span();
                self.advance();
                let operand = self.parse_expr_bp(13)?;
                let span = start.merge(operand.span);
                Some(Expr {
                    kind: ExprKind::UnaryOp {
                        op: UnaryOp::Not,
                        operand: Box::new(operand),
                    },
                    span,
                })
            }
            // Parenthesized expression
            Token::LParen => {
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen).ok()?;
                Some(expr)
            }
            // Static table read: @table[key].col
            Token::At => self.parse_static_read(),
            // Integer literal
            Token::IntLit(_) => {
                let (tok, span) = self.advance();
                if let Token::IntLit(n) = tok {
                    Some(Expr {
                        kind: ExprKind::IntLit(n),
                        span,
                    })
                } else {
                    unreachable!()
                }
            }
            // Hex literal
            Token::HexLit(_) => {
                let (tok, span) = self.advance();
                if let Token::HexLit(b) = tok {
                    Some(Expr {
                        kind: ExprKind::HexLit(b),
                        span,
                    })
                } else {
                    unreachable!()
                }
            }
            // String literal — only valid in emit, but parser accepts it for simplicity
            Token::StringLit(_) => {
                self.errors.push(CompileError::new(
                    ErrorKind::UnexpectedToken,
                    self.peek_span(),
                    "string literals are only valid in emit statements",
                ));
                None
            }
            // Bool literals
            Token::True => {
                let span = self.peek_span();
                self.advance();
                Some(Expr {
                    kind: ExprKind::BoolLit(true),
                    span,
                })
            }
            Token::False => {
                let span = self.peek_span();
                self.advance();
                Some(Expr {
                    kind: ExprKind::BoolLit(false),
                    span,
                })
            }
            // Null
            Token::Null => {
                let span = self.peek_span();
                self.advance();
                Some(Expr {
                    kind: ExprKind::Null,
                    span,
                })
            }
            // Built-in: hash(...)
            Token::Hash => self.parse_hash_call(),
            // Built-in: divmod(...)
            Token::Divmod => self.parse_divmod_expr(),
            // Built-in: select(cond, if_true, if_false)
            Token::Select => self.parse_select_call(),
            // Identifier — could be a simple variable or cell read.
            Token::Ident(_) => self.parse_ident_or_cell_read(),
            _ => {
                self.errors.push(CompileError::new(
                    ErrorKind::UnexpectedToken,
                    self.peek_span(),
                    format!("expected expression, found {:?}", self.peek()),
                ));
                None
            }
        }
    }

    /// Parse `hash(expr, expr, ...)`
    fn parse_hash_call(&mut self) -> Option<Expr> {
        let start = self.peek_span();
        self.advance(); // consume 'hash'
        self.expect(&Token::LParen).ok()?;
        let mut args = Vec::new();
        while !matches!(self.peek(), Token::RParen | Token::Eof) {
            let arg = self.parse_expr()?;
            args.push(arg);
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            }
        }
        let end = self.peek_span();
        self.expect(&Token::RParen).ok()?;
        Some(Expr {
            kind: ExprKind::Hash(args),
            span: start.merge(end),
        })
    }

    /// Parse `divmod(expr, expr)` as an expression (e.g. in `let (q,r) = divmod(a,b)`)
    fn parse_divmod_expr(&mut self) -> Option<Expr> {
        let start = self.peek_span();
        self.advance(); // consume 'divmod'
        self.expect(&Token::LParen).ok()?;
        let lhs = self.parse_expr()?;
        self.expect(&Token::Comma).ok()?;
        let rhs = self.parse_expr()?;
        let end = self.peek_span();
        self.expect(&Token::RParen).ok()?;
        Some(Expr {
            kind: ExprKind::Divmod {
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span: start.merge(end),
        })
    }

    /// Parse `select(cond, if_true, if_false)`
    fn parse_select_call(&mut self) -> Option<Expr> {
        let start = self.peek_span();
        self.advance(); // consume 'select'
        self.expect(&Token::LParen).ok()?;
        let cond = self.parse_expr()?;
        self.expect(&Token::Comma).ok()?;
        let if_true = self.parse_expr()?;
        self.expect(&Token::Comma).ok()?;
        let if_false = self.parse_expr()?;
        let end = self.peek_span();
        self.expect(&Token::RParen).ok()?;
        Some(Expr {
            kind: ExprKind::Select {
                cond: Box::new(cond),
                if_true: Box::new(if_true),
                if_false: Box::new(if_false),
            },
            span: start.merge(end),
        })
    }

    /// Parse an identifier that may be followed by `[row].col` (cell read).
    fn parse_ident_or_cell_read(&mut self) -> Option<Expr> {
        let (tok, start) = self.advance();
        let name = if let Token::Ident(name) = tok {
            name
        } else {
            unreachable!()
        };

        // Check for cell read: name[row].col
        if matches!(self.peek(), Token::LBracket) {
            self.advance(); // consume '['
            let row = self.parse_expr()?;
            self.expect(&Token::RBracket).ok()?;
            self.expect(&Token::Dot).ok()?;
            let (col, col_span) = self.expect_ident().ok()?;
            Some(Expr {
                kind: ExprKind::CellRead {
                    table: name,
                    row: Box::new(row),
                    col,
                },
                span: start.merge(col_span),
            })
        } else {
            Some(Expr {
                kind: ExprKind::Ident(name),
                span: start,
            })
        }
    }

    /// Parse `@table[key].col`
    fn parse_static_read(&mut self) -> Option<Expr> {
        let start = self.peek_span();
        self.advance(); // consume '@'
        let (table, _) = self.expect_ident().ok()?;
        self.expect(&Token::LBracket).ok()?;
        let key = self.parse_expr()?;
        self.expect(&Token::RBracket).ok()?;
        self.expect(&Token::Dot).ok()?;
        let (col, col_span) = self.expect_ident().ok()?;
        Some(Expr {
            kind: ExprKind::StaticRead {
                table,
                key: Box::new(key),
                col,
            },
            span: start.merge(col_span),
        })
    }

    // --- Error recovery ---

    fn skip_to_recovery(&mut self, tokens: &[Token]) {
        while !self.at_eof() {
            if tokens.iter().any(|t| self.peek() == t) {
                return;
            }
            self.advance();
        }
    }

    fn skip_to_stmt_boundary(&mut self) {
        while !self.at_eof() {
            match self.peek() {
                Token::Let | Token::Assert | Token::Emit | Token::RBrace => return,
                Token::Ident(_) => return,
                _ => {
                    self.advance();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::lex;

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

    // --- Error cases ---

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

    // --- Error cases ---

    #[test]
    fn test_parse_error_missing_brace() {
        let tokens = lex("table t { a: u64").unwrap();
        let result = parse(tokens);
        assert!(result.is_err());
    }
}

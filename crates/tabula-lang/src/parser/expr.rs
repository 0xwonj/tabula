//! Expression parsing methods for Parser (Pratt parser).

use crate::ast::*;
use crate::error::{CompileError, ErrorKind};
use crate::token::Token;

use super::Parser;

impl Parser {
    pub(super) fn parse_expr(&mut self) -> Option<Expr> {
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
}

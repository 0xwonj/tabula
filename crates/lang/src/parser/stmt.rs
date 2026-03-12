//! Statement parsing methods for Parser.

use crate::ast::{Stmt, StmtKind};
use crate::error::{CompileError, ErrorKind};
use crate::span::Span;
use crate::token::Token;

use super::Parser;

impl Parser {
    pub(super) fn parse_stmt(&mut self) -> Option<Stmt> {
        match self.peek() {
            Token::Let => self.parse_let_stmt(),
            Token::Assert => self.parse_assert_stmt(),
            Token::Emit => self.parse_emit_stmt(),
            Token::At => self.parse_precompile_stmt(),
            Token::Ident(_) => self.parse_assign_stmt(),
            _ => {
                self.errors.push(CompileError::new(
                    ErrorKind::UnexpectedToken,
                    self.peek_span(),
                    format!(
                        "expected statement (let, assert, emit, @precompile, or assignment), found {:?}",
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

    fn parse_precompile_stmt(&mut self) -> Option<Stmt> {
        let start = self.peek_span();
        self.advance(); // consume '@'

        let (name, _) = self.expect_ident().ok()?;
        if name != "precompile" {
            self.errors.push(CompileError::new(
                ErrorKind::UnexpectedToken,
                self.peek_span(),
                format!("expected 'precompile' after '@', found '{name}'"),
            ));
            return None;
        }

        self.expect(&Token::LParen).ok()?;

        // Parse ID (hex or decimal integer literal).
        let id_expr = self.parse_expr()?;
        self.expect(&Token::Comma).ok()?;

        // Parse destination names: [name1, name2, ...]
        self.expect(&Token::LBracket).ok()?;
        let mut dst_names = Vec::new();
        while !matches!(self.peek(), Token::RBracket | Token::Eof) {
            let (ident, _) = self.expect_ident().ok()?;
            dst_names.push(ident);
            if matches!(self.peek(), Token::Comma) {
                self.advance();
            }
        }
        self.expect(&Token::RBracket).ok()?;

        // Parse inputs: , expr, expr, ...
        let mut inputs = Vec::new();
        while matches!(self.peek(), Token::Comma) {
            self.advance(); // consume ','
            if matches!(self.peek(), Token::RParen | Token::Eof) {
                break;
            }
            let arg = self.parse_expr()?;
            inputs.push(arg);
        }
        let end = self.peek_span();
        self.expect(&Token::RParen).ok()?;

        // Extract ID from expression (integer or small hex literal).
        let id = match &id_expr.kind {
            crate::ast::ExprKind::IntLit(n) => {
                if *n > u16::MAX as u64 {
                    self.errors.push(CompileError::new(
                        ErrorKind::UnexpectedToken,
                        id_expr.span,
                        format!("precompile ID {n} exceeds u16::MAX"),
                    ));
                    return None;
                }
                *n as u16
            }
            crate::ast::ExprKind::HexLit(bytes) => {
                // Only the last 2 bytes may be non-zero for a valid u16 ID.
                if bytes[..30].iter().any(|&b| b != 0) {
                    self.errors.push(CompileError::new(
                        ErrorKind::UnexpectedToken,
                        id_expr.span,
                        "precompile ID exceeds u16::MAX",
                    ));
                    return None;
                }
                u16::from_be_bytes([bytes[30], bytes[31]])
            }
            _ => {
                self.errors.push(CompileError::new(
                    ErrorKind::ExpectedToken,
                    id_expr.span,
                    "precompile ID must be an integer literal",
                ));
                return None;
            }
        };

        Some(Stmt {
            kind: StmtKind::Precompile {
                id,
                dst_names,
                inputs,
            },
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
}

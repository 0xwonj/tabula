//! Recursive descent parser with Pratt expression parsing.
//!
//! Converts a token stream into an AST.

mod expr;
mod stmt;

use crate::ast::{ColumnDecl, ColumnSchemeDecl, ParamDecl, Program, TableDecl, TxDecl, TypeName};
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
    pub(super) tokens: Vec<(Token, Span)>,
    pub(super) pos: usize,
    pub(super) errors: Vec<CompileError>,
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
        let scheme = self.parse_column_scheme_annotation();
        let end = scheme.map_or(ty_span, |(_, span)| span);
        Some(ColumnDecl {
            name,
            ty,
            scheme: scheme.map(|(scheme, _)| scheme),
            span: start.merge(end),
        })
    }

    fn parse_column_scheme_annotation(&mut self) -> Option<(ColumnSchemeDecl, Span)> {
        if !matches!(self.peek(), Token::At) {
            return None;
        }
        let start = self.peek_span();
        self.advance(); // consume '@'

        let Ok((name, name_span)) = self.expect_ident() else {
            return None;
        };

        match name.as_str() {
            "ssmc" => Some((ColumnSchemeDecl::Ssmc, start.merge(name_span))),
            "smt" => Some((ColumnSchemeDecl::Smt, start.merge(name_span))),
            "scheme" => self.parse_numeric_scheme_annotation(start),
            _ => {
                self.errors.push(CompileError::new(
                    ErrorKind::UnexpectedToken,
                    start,
                    format!(
                        "expected column scheme annotation '@ssmc', '@smt', or '@scheme(<u16>)', found '@{name}'"
                    ),
                ));
                None
            }
        }
    }

    fn parse_numeric_scheme_annotation(&mut self, start: Span) -> Option<(ColumnSchemeDecl, Span)> {
        self.expect(&Token::LParen).ok()?;
        let value_span = self.peek_span();
        let scheme_id = match self.peek() {
            Token::IntLit(value) => {
                let value = *value;
                self.advance();
                match u16::try_from(value) {
                    Ok(ok) => ok,
                    Err(_) => {
                        self.errors.push(CompileError::new(
                            ErrorKind::UnexpectedToken,
                            value_span,
                            format!("scheme id {value} does not fit into u16"),
                        ));
                        return None;
                    }
                }
            }
            _ => {
                self.errors.push(CompileError::new(
                    ErrorKind::ExpectedToken,
                    value_span,
                    format!(
                        "expected numeric scheme id inside '@scheme(...)', found {:?}",
                        self.peek()
                    ),
                ));
                return None;
            }
        };
        let end = self.expect(&Token::RParen).ok()?;
        Some((ColumnSchemeDecl::Numeric(scheme_id), start.merge(end)))
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
                Token::Let | Token::Assert | Token::Emit | Token::At | Token::RBrace => return,
                Token::Ident(_) => return,
                _ => {
                    self.advance();
                }
            }
        }
    }
}

//! Hand-rolled lexer for the Tabula DSL.
//!
//! Converts source text into a sequence of `(Token, Span)` pairs.

use crate::error::{CompileError, ErrorKind};
use crate::span::Span;
use crate::token::Token;

/// Tokenize source text into a token stream.
///
/// Returns the tokens on success. On lexer errors, returns the tokens
/// produced so far along with all errors encountered.
pub fn lex(source: &str) -> Result<Vec<(Token, Span)>, Vec<CompileError>> {
    let mut lexer = Lexer::new(source);
    lexer.run();
    if lexer.errors.is_empty() {
        Ok(lexer.tokens)
    } else {
        Err(lexer.errors)
    }
}

struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<(Token, Span)>,
    errors: Vec<CompileError>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            pos: 0,
            tokens: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn run(&mut self) {
        while self.pos < self.bytes.len() {
            self.skip_whitespace_and_comments();
            if self.pos >= self.bytes.len() {
                break;
            }
            let start = self.pos;
            let b = self.bytes[self.pos];

            match b {
                b'{' => self.single(Token::LBrace, start),
                b'}' => self.single(Token::RBrace, start),
                b'(' => self.single(Token::LParen, start),
                b')' => self.single(Token::RParen, start),
                b'[' => self.single(Token::LBracket, start),
                b']' => self.single(Token::RBracket, start),
                b'.' => self.single(Token::Dot, start),
                b',' => self.single(Token::Comma, start),
                b':' => self.single(Token::Colon, start),
                b'@' => self.single(Token::At, start),
                b'+' => self.single(Token::Plus, start),
                b'-' => self.single(Token::Minus, start),
                b'*' => self.single(Token::Star, start),
                b'%' => self.single(Token::Percent, start),
                b'/' => self.lex_slash(start),
                b'=' => self.lex_eq(start),
                b'!' => self.lex_bang(start),
                b'<' => self.lex_lt(start),
                b'>' => self.lex_gt(start),
                b'&' => self.lex_amp(start),
                b'|' => self.lex_pipe(start),
                b'"' => self.lex_string(start),
                b'0' if self.peek_at(1) == Some(b'x') => self.lex_hex(start),
                b'0'..=b'9' => self.lex_integer(start),
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.lex_ident(start),
                _ => {
                    self.errors.push(CompileError::new(
                        ErrorKind::UnexpectedChar,
                        Span::new(start, start + 1),
                        format!("unexpected character '{}'", b as char),
                    ));
                    self.pos += 1;
                }
            }
        }
        self.tokens
            .push((Token::Eof, Span::new(self.pos, self.pos)));
    }

    // --- Helpers ---

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn single(&mut self, tok: Token, start: usize) {
        self.pos += 1;
        self.tokens.push((tok, Span::new(start, self.pos)));
    }

    fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
                self.pos += 1;
            } else if b == b'/' && self.peek_at(1) == Some(b'/') {
                // Line comment: skip to end of line.
                self.pos += 2;
                while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    // --- Multi-character tokens ---

    fn lex_slash(&mut self, start: usize) {
        // '/' — not a comment (comments handled in skip_whitespace_and_comments)
        self.pos += 1;
        self.tokens.push((Token::Slash, Span::new(start, self.pos)));
    }

    fn lex_eq(&mut self, start: usize) {
        self.pos += 1;
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'=' {
            self.pos += 1;
            self.tokens.push((Token::EqEq, Span::new(start, self.pos)));
        } else {
            self.tokens.push((Token::Eq, Span::new(start, self.pos)));
        }
    }

    fn lex_bang(&mut self, start: usize) {
        self.pos += 1;
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'=' {
            self.pos += 1;
            self.tokens
                .push((Token::BangEq, Span::new(start, self.pos)));
        } else {
            self.tokens.push((Token::Bang, Span::new(start, self.pos)));
        }
    }

    fn lex_lt(&mut self, start: usize) {
        self.pos += 1;
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'=' {
            self.pos += 1;
            self.tokens.push((Token::LtEq, Span::new(start, self.pos)));
        } else {
            self.tokens.push((Token::Lt, Span::new(start, self.pos)));
        }
    }

    fn lex_gt(&mut self, start: usize) {
        self.pos += 1;
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'=' {
            self.pos += 1;
            self.tokens.push((Token::GtEq, Span::new(start, self.pos)));
        } else {
            self.tokens.push((Token::Gt, Span::new(start, self.pos)));
        }
    }

    fn lex_amp(&mut self, start: usize) {
        self.pos += 1;
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'&' {
            self.pos += 1;
            self.tokens
                .push((Token::AmpAmp, Span::new(start, self.pos)));
        } else {
            self.errors.push(CompileError::new(
                ErrorKind::UnexpectedChar,
                Span::new(start, self.pos),
                "expected '&&', got single '&'",
            ));
        }
    }

    fn lex_pipe(&mut self, start: usize) {
        self.pos += 1;
        if self.pos < self.bytes.len() && self.bytes[self.pos] == b'|' {
            self.pos += 1;
            self.tokens
                .push((Token::PipePipe, Span::new(start, self.pos)));
        } else {
            self.errors.push(CompileError::new(
                ErrorKind::UnexpectedChar,
                Span::new(start, self.pos),
                "expected '||', got single '|'",
            ));
        }
    }

    // --- Literals ---

    fn lex_string(&mut self, start: usize) {
        self.pos += 1; // skip opening '"'
        let content_start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos] != b'"' {
            if self.bytes[self.pos] == b'\n' {
                self.errors.push(CompileError::new(
                    ErrorKind::UnterminatedString,
                    Span::new(start, self.pos),
                    "unterminated string literal (newline before closing quote)",
                ));
                return;
            }
            self.pos += 1;
        }
        if self.pos >= self.bytes.len() {
            self.errors.push(CompileError::new(
                ErrorKind::UnterminatedString,
                Span::new(start, self.pos),
                "unterminated string literal",
            ));
            return;
        }
        let content = self.source[content_start..self.pos].to_string();
        self.pos += 1; // skip closing '"'
        self.tokens
            .push((Token::StringLit(content), Span::new(start, self.pos)));
    }

    fn lex_hex(&mut self, start: usize) {
        self.pos += 2; // skip '0x'
        let hex_start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_hexdigit() {
            self.pos += 1;
        }
        let hex_str = &self.source[hex_start..self.pos];
        if hex_str.is_empty() || hex_str.len() > 64 {
            self.errors.push(CompileError::new(
                ErrorKind::InvalidHexLiteral,
                Span::new(start, self.pos),
                format!("hex literal must be 1-64 hex chars, got {}", hex_str.len()),
            ));
            return;
        }
        // Left-pad to 64 hex chars (32 bytes).
        let padded = format!("{hex_str:0>64}");
        let mut bytes = [0u8; 32];
        for (i, chunk) in padded.as_bytes().chunks(2).enumerate() {
            let s = std::str::from_utf8(chunk).expect("valid hex chars");
            bytes[i] = u8::from_str_radix(s, 16).expect("valid hex");
        }
        self.tokens
            .push((Token::HexLit(bytes), Span::new(start, self.pos)));
    }

    fn lex_integer(&mut self, start: usize) {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        let text = &self.source[start..self.pos];
        match text.parse::<u64>() {
            Ok(n) => self
                .tokens
                .push((Token::IntLit(n), Span::new(start, self.pos))),
            Err(_) => self.errors.push(CompileError::new(
                ErrorKind::IntegerOverflow,
                Span::new(start, self.pos),
                format!("integer literal '{text}' overflows u64"),
            )),
        }
    }

    fn lex_ident(&mut self, start: usize) {
        while self.pos < self.bytes.len()
            && (self.bytes[self.pos].is_ascii_alphanumeric() || self.bytes[self.pos] == b'_')
        {
            self.pos += 1;
        }
        let text = &self.source[start..self.pos];
        let tok = Token::keyword(text).unwrap_or_else(|| Token::Ident(text.to_string()));
        self.tokens.push((tok, Span::new(start, self.pos)));
    }
}

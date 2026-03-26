use crate::span::Span;

use crate::error::{FrontendError, FrontendErrorKind};
use crate::syntax::token::Token;

pub fn lex(source: &str) -> Result<Vec<(Token, Span)>, FrontendError> {
    let mut lexer = Lexer::new(source);
    lexer.run()
}

struct Lexer<'a> {
    source: &'a str,
    bytes: &'a [u8],
    pos: usize,
    tokens: Vec<(Token, Span)>,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            pos: 0,
            tokens: Vec::new(),
        }
    }

    fn run(&mut self) -> Result<Vec<(Token, Span)>, FrontendError> {
        while self.pos < self.bytes.len() {
            self.skip_whitespace_and_comments();
            if self.pos >= self.bytes.len() {
                break;
            }
            let start = self.pos;
            match self.bytes[self.pos] {
                b'{' => self.single(Token::LBrace, start),
                b'}' => self.single(Token::RBrace, start),
                b'(' => self.single(Token::LParen, start),
                b')' => self.single(Token::RParen, start),
                b'[' => self.single(Token::LBracket, start),
                b']' => self.single(Token::RBracket, start),
                b',' => self.single(Token::Comma, start),
                b':' => self.lex_colon(start),
                b';' => self.single(Token::Semi, start),
                b'.' => self.single(Token::Dot, start),
                b'@' => self.single(Token::At, start),
                b'+' => self.single(Token::Plus, start),
                b'-' => self.lex_minus(start),
                b'*' => self.single(Token::Star, start),
                b'/' => self.single(Token::Slash, start),
                b'%' => self.single(Token::Percent, start),
                b'=' => self.lex_eq(start),
                b'!' => self.lex_bang(start),
                b'<' => self.lex_lt(start),
                b'>' => self.lex_gt(start),
                b'&' => self.lex_amp(start)?,
                b'|' => self.lex_pipe(start)?,
                b'0' if self.peek_at(1) == Some(b'x') => self.lex_hex(start)?,
                b'0'..=b'9' => self.lex_integer(start)?,
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.lex_ident(start),
                _ => {
                    return Err(FrontendError::new(
                        FrontendErrorKind::UnexpectedChar,
                        Span::new(start, start + 1),
                        format!("unexpected character '{}'", self.bytes[self.pos] as char),
                    ));
                }
            }
        }
        self.tokens
            .push((Token::Eof, Span::new(self.pos, self.pos)));
        Ok(std::mem::take(&mut self.tokens))
    }

    fn skip_whitespace_and_comments(&mut self) {
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            if matches!(b, b' ' | b'\t' | b'\n' | b'\r') {
                self.pos += 1;
            } else if b == b'/' && self.peek_at(1) == Some(b'/') {
                self.pos += 2;
                while self.pos < self.bytes.len() && self.bytes[self.pos] != b'\n' {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.bytes.get(self.pos + offset).copied()
    }

    fn single(&mut self, token: Token, start: usize) {
        self.pos += 1;
        self.tokens.push((token, Span::new(start, self.pos)));
    }

    fn lex_colon(&mut self, start: usize) {
        self.pos += 1;
        if self.peek_at(0) == Some(b':') {
            self.pos += 1;
            self.tokens
                .push((Token::PathSep, Span::new(start, self.pos)));
        } else {
            self.tokens.push((Token::Colon, Span::new(start, self.pos)));
        }
    }

    fn lex_minus(&mut self, start: usize) {
        self.pos += 1;
        if self.peek_at(0) == Some(b'>') {
            self.pos += 1;
            self.tokens.push((Token::Arrow, Span::new(start, self.pos)));
        } else {
            self.tokens.push((Token::Minus, Span::new(start, self.pos)));
        }
    }

    fn lex_eq(&mut self, start: usize) {
        self.pos += 1;
        match self.peek_at(0) {
            Some(b'=') => {
                self.pos += 1;
                self.tokens.push((Token::EqEq, Span::new(start, self.pos)));
            }
            Some(b'>') => {
                self.pos += 1;
                self.tokens
                    .push((Token::FatArrow, Span::new(start, self.pos)));
            }
            _ => self.tokens.push((Token::Eq, Span::new(start, self.pos))),
        }
    }

    fn lex_bang(&mut self, start: usize) {
        self.pos += 1;
        if self.peek_at(0) == Some(b'=') {
            self.pos += 1;
            self.tokens
                .push((Token::BangEq, Span::new(start, self.pos)));
        } else {
            self.tokens.push((Token::Bang, Span::new(start, self.pos)));
        }
    }

    fn lex_lt(&mut self, start: usize) {
        self.pos += 1;
        if self.peek_at(0) == Some(b'=') {
            self.pos += 1;
            self.tokens.push((Token::LtEq, Span::new(start, self.pos)));
        } else {
            self.tokens.push((Token::Lt, Span::new(start, self.pos)));
        }
    }

    fn lex_gt(&mut self, start: usize) {
        self.pos += 1;
        if self.peek_at(0) == Some(b'=') {
            self.pos += 1;
            self.tokens.push((Token::GtEq, Span::new(start, self.pos)));
        } else {
            self.tokens.push((Token::Gt, Span::new(start, self.pos)));
        }
    }

    fn lex_amp(&mut self, start: usize) -> Result<(), FrontendError> {
        self.pos += 1;
        if self.peek_at(0) == Some(b'&') {
            self.pos += 1;
            self.tokens
                .push((Token::AmpAmp, Span::new(start, self.pos)));
            Ok(())
        } else {
            Err(FrontendError::new(
                FrontendErrorKind::UnexpectedChar,
                Span::new(start, self.pos),
                "expected '&&'",
            ))
        }
    }

    fn lex_pipe(&mut self, start: usize) -> Result<(), FrontendError> {
        self.pos += 1;
        if self.peek_at(0) == Some(b'|') {
            self.pos += 1;
            self.tokens
                .push((Token::PipePipe, Span::new(start, self.pos)));
            Ok(())
        } else {
            Err(FrontendError::new(
                FrontendErrorKind::UnexpectedChar,
                Span::new(start, self.pos),
                "expected '||'",
            ))
        }
    }

    fn lex_hex(&mut self, start: usize) -> Result<(), FrontendError> {
        self.pos += 2;
        let hex_start = self.pos;
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_hexdigit() {
            self.pos += 1;
        }
        let hex = &self.source[hex_start..self.pos];
        if hex.is_empty() || hex.len() > 64 {
            return Err(FrontendError::new(
                FrontendErrorKind::InvalidHexLiteral,
                Span::new(start, self.pos),
                format!("hex literal must be 1-64 chars, got {}", hex.len()),
            ));
        }
        let padded = format!("{hex:0>64}");
        let mut bytes = [0u8; 32];
        for (index, chunk) in padded.as_bytes().chunks(2).enumerate() {
            let text = std::str::from_utf8(chunk).expect("hex chunk");
            bytes[index] = u8::from_str_radix(text, 16).expect("valid hex");
        }
        self.tokens
            .push((Token::HexLit(bytes), Span::new(start, self.pos)));
        Ok(())
    }

    fn lex_integer(&mut self, start: usize) -> Result<(), FrontendError> {
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        let text = &self.source[start..self.pos];
        let value = text.parse::<u64>().map_err(|_| {
            FrontendError::new(
                FrontendErrorKind::IntegerOverflow,
                Span::new(start, self.pos),
                format!("integer literal '{text}' does not fit in u64"),
            )
        })?;
        self.tokens
            .push((Token::IntLit(value), Span::new(start, self.pos)));
        Ok(())
    }

    fn lex_ident(&mut self, start: usize) {
        self.pos += 1;
        while self.pos < self.bytes.len()
            && (self.bytes[self.pos].is_ascii_alphanumeric() || self.bytes[self.pos] == b'_')
        {
            self.pos += 1;
        }
        let text = &self.source[start..self.pos];
        let token = Token::keyword(text).unwrap_or_else(|| Token::Ident(text.to_string()));
        self.tokens.push((token, Span::new(start, self.pos)));
    }
}

//! Token types for the Tabula DSL lexer.

/// A token produced by the lexer.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // --- Keywords ---
    /// `table`
    Table,
    /// `tx`
    Tx,
    /// `let`
    Let,
    /// `assert`
    Assert,
    /// `emit`
    Emit,
    /// `null`
    Null,
    /// `true`
    True,
    /// `false`
    False,

    // --- Type keywords ---
    /// `u64`
    U64,
    /// `i64`
    I64,
    /// `bool`
    Bool,
    /// `bytes32`
    Bytes32,

    // --- Built-in functions ---
    /// `hash`
    Hash,
    /// `divmod`
    Divmod,

    // --- Identifiers and literals ---
    /// User-defined identifier.
    Ident(String),
    /// Integer literal (always non-negative; negation is a unary op).
    IntLit(u64),
    /// Hex literal: `0x...` (padded to 32 bytes).
    HexLit([u8; 32]),
    /// String literal (for emit topics).
    StringLit(String),

    // --- Delimiters ---
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `[`
    LBracket,
    /// `]`
    RBracket,

    // --- Punctuation ---
    /// `.`
    Dot,
    /// `,`
    Comma,
    /// `:`
    Colon,
    /// `@`
    At,
    /// `=`
    Eq,

    // --- Arithmetic operators ---
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,

    // --- Comparison operators ---
    /// `==`
    EqEq,
    /// `!=`
    BangEq,
    /// `<`
    Lt,
    /// `<=`
    LtEq,
    /// `>`
    Gt,
    /// `>=`
    GtEq,

    // --- Logical operators ---
    /// `&&`
    AmpAmp,
    /// `||`
    PipePipe,
    /// `!`
    Bang,

    // --- End of file ---
    /// Signals the end of input.
    Eof,
}

impl Token {
    /// Return the keyword token for a given identifier, or `None`.
    pub fn keyword(s: &str) -> Option<Token> {
        match s {
            "table" => Some(Token::Table),
            "tx" => Some(Token::Tx),
            "let" => Some(Token::Let),
            "assert" => Some(Token::Assert),
            "emit" => Some(Token::Emit),
            "null" => Some(Token::Null),
            "true" => Some(Token::True),
            "false" => Some(Token::False),
            "u64" => Some(Token::U64),
            "i64" => Some(Token::I64),
            "bool" => Some(Token::Bool),
            "bytes32" => Some(Token::Bytes32),
            "hash" => Some(Token::Hash),
            "divmod" => Some(Token::Divmod),
            _ => None,
        }
    }
}

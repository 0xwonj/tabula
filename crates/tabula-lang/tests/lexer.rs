use tabula_lang::error::ErrorKind;
use tabula_lang::lexer::lex;
use tabula_lang::span::Span;
use tabula_lang::token::Token;

fn tok_types(source: &str) -> Vec<Token> {
    lex(source).unwrap().into_iter().map(|(t, _)| t).collect()
}

#[test]
fn test_keywords() {
    let tokens = tok_types("table tx let assert emit");
    assert_eq!(
        tokens,
        vec![
            Token::Table,
            Token::Tx,
            Token::Let,
            Token::Assert,
            Token::Emit,
            Token::Eof
        ]
    );
}

#[test]
fn test_type_keywords() {
    let tokens = tok_types("u64 i64 bool bytes32");
    assert_eq!(
        tokens,
        vec![
            Token::U64,
            Token::I64,
            Token::Bool,
            Token::Bytes32,
            Token::Eof
        ]
    );
}

#[test]
fn test_builtins() {
    let tokens = tok_types("hash divmod");
    assert_eq!(tokens, vec![Token::Hash, Token::Divmod, Token::Eof]);
}

#[test]
fn test_identifier() {
    let tokens = tok_types("foo bar_baz _x");
    assert_eq!(
        tokens,
        vec![
            Token::Ident("foo".into()),
            Token::Ident("bar_baz".into()),
            Token::Ident("_x".into()),
            Token::Eof,
        ]
    );
}

#[test]
fn test_integer_literal() {
    let tokens = tok_types("42 0 18446744073709551615");
    assert_eq!(
        tokens,
        vec![
            Token::IntLit(42),
            Token::IntLit(0),
            Token::IntLit(u64::MAX),
            Token::Eof,
        ]
    );
}

#[test]
fn test_integer_overflow() {
    let result = lex("99999999999999999999");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err()[0].kind, ErrorKind::IntegerOverflow);
}

#[test]
fn test_hex_literal() {
    let tokens = tok_types("0xff");
    let expected = {
        let mut b = [0u8; 32];
        b[31] = 0xff;
        b
    };
    assert_eq!(tokens, vec![Token::HexLit(expected), Token::Eof]);
}

#[test]
fn test_hex_literal_full() {
    let hex = format!("0x{}", "ab".repeat(32));
    let tokens = tok_types(&hex);
    assert_eq!(tokens, vec![Token::HexLit([0xab; 32]), Token::Eof]);
}

#[test]
fn test_string_literal() {
    let tokens = tok_types(r#""hello""#);
    assert_eq!(tokens, vec![Token::StringLit("hello".into()), Token::Eof]);
}

#[test]
fn test_unterminated_string() {
    let result = lex(r#""hello"#);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err()[0].kind, ErrorKind::UnterminatedString);
}

#[test]
fn test_operators() {
    let tokens = tok_types("+ - * / % == != < <= > >= && || !");
    assert_eq!(
        tokens,
        vec![
            Token::Plus,
            Token::Minus,
            Token::Star,
            Token::Slash,
            Token::Percent,
            Token::EqEq,
            Token::BangEq,
            Token::Lt,
            Token::LtEq,
            Token::Gt,
            Token::GtEq,
            Token::AmpAmp,
            Token::PipePipe,
            Token::Bang,
            Token::Eof,
        ]
    );
}

#[test]
fn test_delimiters_and_punctuation() {
    let tokens = tok_types("{ } ( ) [ ] . , : @ =");
    assert_eq!(
        tokens,
        vec![
            Token::LBrace,
            Token::RBrace,
            Token::LParen,
            Token::RParen,
            Token::LBracket,
            Token::RBracket,
            Token::Dot,
            Token::Comma,
            Token::Colon,
            Token::At,
            Token::Eq,
            Token::Eof,
        ]
    );
}

#[test]
fn test_line_comment() {
    let tokens = tok_types("let // this is a comment\nassert");
    assert_eq!(tokens, vec![Token::Let, Token::Assert, Token::Eof]);
}

#[test]
fn test_full_table_decl() {
    let tokens = tok_types("table balances { balance: u64 }");
    assert_eq!(
        tokens,
        vec![
            Token::Table,
            Token::Ident("balances".into()),
            Token::LBrace,
            Token::Ident("balance".into()),
            Token::Colon,
            Token::U64,
            Token::RBrace,
            Token::Eof,
        ]
    );
}

#[test]
fn test_cell_access() {
    let tokens = tok_types("balances[from].balance");
    assert_eq!(
        tokens,
        vec![
            Token::Ident("balances".into()),
            Token::LBracket,
            Token::Ident("from".into()),
            Token::RBracket,
            Token::Dot,
            Token::Ident("balance".into()),
            Token::Eof,
        ]
    );
}

#[test]
fn test_spans_are_correct() {
    let tokens = lex("let x").unwrap();
    assert_eq!(tokens[0], (Token::Let, Span::new(0, 3)));
    assert_eq!(tokens[1], (Token::Ident("x".into()), Span::new(4, 5)));
}

#[test]
fn test_unexpected_char() {
    let result = lex("let x = ~");
    assert!(result.is_err());
    assert_eq!(result.unwrap_err()[0].kind, ErrorKind::UnexpectedChar);
}

#[test]
fn test_bool_and_null_literals() {
    let tokens = tok_types("true false null");
    assert_eq!(
        tokens,
        vec![Token::True, Token::False, Token::Null, Token::Eof]
    );
}

#[test]
fn test_static_table_access() {
    let tokens = tok_types("@ranges[key].value");
    assert_eq!(
        tokens,
        vec![
            Token::At,
            Token::Ident("ranges".into()),
            Token::LBracket,
            Token::Ident("key".into()),
            Token::RBracket,
            Token::Dot,
            Token::Ident("value".into()),
            Token::Eof,
        ]
    );
}

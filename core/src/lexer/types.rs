use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // literals
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),

    // keywords
    Var,
    If,
    Else,
    True,
    False,
    Func,
    Return,
    Null,
    While,
    For,
    Break,
    Continue,
    Match,
    Case,
    In,

    // punctuation
    Semicolon, // ;
    LParen,    // (
    RParen,    // )
    LBrace,    // {
    RBrace,    // }
    LBracket,  // [
    RBracket,  // ]
    Dot,       // .
    Comma,     // ,
    Colon,     // :

    // operators
    Assign,     // =
    Eq,         // ==
    NotEq,      // !=
    Lt,         // <
    LtEq,       // <=
    Gt,         // >
    GtEq,       // >=
    Not,        // !
    AndAnd,     // &&
    OrOr,       // ||
    Plus,       // +
    Minus,      // -
    Star,       // *
    Slash,      // /
    Percent,    // %
    PlusEq,     // +=
    MinusEq,    // -=
    StarEq,     // *=
    SlashEq,    // /=
    PercentEq,  // %=
    PlusPlus,   // ++
    MinusMinus, // --

    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TokenKind::Ident(name) => write!(f, "identifier '{}'", name),
            TokenKind::Int(v) => write!(f, "integer '{}'", v),
            TokenKind::Float(v) => write!(f, "float '{}'", v),
            TokenKind::Str(v) => write!(f, "string \"{}\"", v),

            TokenKind::Var => write!(f, "'var'"),
            TokenKind::If => write!(f, "'if'"),
            TokenKind::Else => write!(f, "'else'"),
            TokenKind::True => write!(f, "'true'"),
            TokenKind::False => write!(f, "'false'"),
            TokenKind::Func => write!(f, "'func'"),
            TokenKind::Return => write!(f, "'return'"),
            TokenKind::Null => write!(f, "'null'"),
            TokenKind::While => write!(f, "'while'"),
            TokenKind::For => write!(f, "'for'"),
            TokenKind::Break => write!(f, "'break'"),
            TokenKind::Continue => write!(f, "'continue'"),
            TokenKind::Match => write!(f, "'match'"),
            TokenKind::Case => write!(f, "'case'"),
            TokenKind::In => write!(f, "'in'"),

            TokenKind::Semicolon => write!(f, "';'"),
            TokenKind::LParen => write!(f, "'('"),
            TokenKind::RParen => write!(f, "')'"),
            TokenKind::LBrace => write!(f, "'{{'"),
            TokenKind::RBrace => write!(f, "'}}'"),
            TokenKind::LBracket => write!(f, "'['"),
            TokenKind::RBracket => write!(f, "']'"),
            TokenKind::Dot => write!(f, "'.'"),
            TokenKind::Comma => write!(f, "','"),
            TokenKind::Colon => write!(f, "':'"),

            TokenKind::Assign => write!(f, "'='"),
            TokenKind::Eq => write!(f, "'=='"),
            TokenKind::NotEq => write!(f, "'!='"),
            TokenKind::Lt => write!(f, "'<'"),
            TokenKind::LtEq => write!(f, "'<='"),
            TokenKind::Gt => write!(f, "'>'"),
            TokenKind::GtEq => write!(f, "'>='"),
            TokenKind::Not => write!(f, "'!'"),
            TokenKind::AndAnd => write!(f, "'&&'"),
            TokenKind::OrOr => write!(f, "'||'"),
            TokenKind::Plus => write!(f, "'+'"),
            TokenKind::Minus => write!(f, "'-'"),
            TokenKind::Star => write!(f, "'*'"),
            TokenKind::Slash => write!(f, "'/'"),
            TokenKind::Percent => write!(f, "'%'"),
            TokenKind::PlusEq => write!(f, "'+='"),
            TokenKind::MinusEq => write!(f, "'-='"),
            TokenKind::StarEq => write!(f, "'*='"),
            TokenKind::SlashEq => write!(f, "'/='"),
            TokenKind::PercentEq => write!(f, "'%='"),
            TokenKind::PlusPlus => write!(f, "'++'"),
            TokenKind::MinusMinus => write!(f, "'--'"),

            TokenKind::Eof => write!(f, "end of input"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?} ({}:{})", self.kind, self.line, self.col)
    }
}

#[derive(Debug)]
pub struct LexError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Lex error at {}:{}: {}",
            self.line, self.col, self.message
        )
    }
}

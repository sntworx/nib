use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Neg, // -expr
    Not, // !expr
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add, // +
    Sub, // -
    Mul, // *
    Div, // /
    Mod, // %

    Eq,    // ==
    NotEq, // !=
    Lt,    // <
    LtEq,  // <=
    Gt,    // >
    GtEq,  // >=

    And, // &&
    Or,  // ||
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Ident(String),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Assign {
        name: String,
        value: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    IndexAssign {
        object: Box<Expr>,
        index: Box<Expr>,
        // Some(op) for `arr[i] += value` (and -=/*=//=), None for plain `=`
        op: Option<BinaryOp>,
        value: Box<Expr>,
    },
    Array(Vec<Expr>),
    // `{key: value, ...}` - keys are static (string literal or bare ident,
    // captured as a plain String at parse time), values are arbitrary
    // expressions.
    Map(Vec<(String, Expr)>),
    Grouping(Box<Expr>),
    // `target.method(args)` - a closed, interpreter-known set of pseudo-
    // methods on built-in types (see `Value::call_method`), not general
    // member access or user-extensible dispatch. `.` is otherwise unused.
    MethodCall {
        target: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarAssign {
    pub name: String,
    pub value: Expr,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_branch: Vec<AstNode>,
    pub else_branch: Option<Vec<AstNode>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FuncDecl {
    pub name: String,
    pub params: Vec<String>,
    pub body: Vec<AstNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WhileStmt {
    pub condition: Expr,
    pub body: Vec<AstNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForStmt {
    pub init: Option<Box<AstNode>>,
    pub condition: Option<Expr>,
    pub post: Option<Expr>,
    pub body: Vec<AstNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ForInStmt {
    pub var_name: String,
    pub iterable: Expr,
    pub body: Vec<AstNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Expr,
    pub body: Vec<AstNode>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchStmt {
    pub subject: Expr,
    pub arms: Vec<MatchArm>,
    pub default_branch: Option<Vec<AstNode>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstNodeKind {
    VarAssign(VarAssign),
    ExprStmt(Expr),
    If(IfStmt),
    Block(Vec<AstNode>),
    FuncDecl(FuncDecl),
    Return(Option<Expr>),
    While(WhileStmt),
    For(ForStmt),
    ForIn(ForInStmt),
    Match(MatchStmt),
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstNode {
    pub kind: AstNodeKind,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Parse error at {}:{}: {}",
            self.line, self.col, self.message
        )
    }
}

#[derive(Debug, PartialEq)]
pub enum Expr {
    IntLiteral(i64),
    StringLiteral(String),
    UnaryOp(UnaryOp, Box<Expr>),
    BinaryOp(BinaryOp, Box<Expr>, Box<Expr>),
    Var(String),
    FunctionCall { name: String, args: Vec<Expr> },
    Assign(String, Box<Expr>), // name = value
}

#[derive(Debug, PartialEq)]
pub enum UnaryOp {
    Negate,     // -
    BitNot,     // ~
    LogNot,     // !
}

#[derive(Debug, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Lt,
    Lte,
    Gt,
    Gte,
    Eq,
    Neq,
    LogicalAnd, // &&
    LogicalOr,  // ||
}

#[derive(Debug, PartialEq)]
pub enum Stmt {
    Return(Expr),
    Expr(Expr),
    VarDecl {
        name: String,
        init: Option<Expr>,
    },
    If {
        cond: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Vec<Stmt>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    For {
        init: Box<Stmt>,   // int i = 0
        cond: Expr,        // i < x
        update: Box<Stmt>, // i++
        body: Vec<Stmt>,   // { ... }
    },
}

#[derive(Debug, PartialEq)]
pub struct Function {
    pub name: String,
    pub body: Vec<Stmt>,
}

#[derive(Debug, PartialEq)]
pub struct Program {
    pub functions: Vec<Function>,
}
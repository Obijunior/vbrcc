use crate::diagnostic::{Span, Spanned};

#[derive(Debug, PartialEq)]
pub enum Expr {
    IntLiteral(i64),
    StringLiteral(String),
    UnaryOp(UnaryOp, Box<Spanned<Expr>>),
    BinaryOp(BinaryOp, Box<Spanned<Expr>>, Box<Spanned<Expr>>),
    Var(String),
    FunctionCall {
        name: String,
        args: Vec<Spanned<Expr>>,
    },
    Assign(String, Box<Spanned<Expr>>), // name = value
}

#[derive(Debug, PartialEq)]
pub enum UnaryOp {
    Negate, // -
    BitNot, // ~
    LogNot, // !
}

#[derive(Debug, PartialEq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod,
    Lt, Lte, Gt, Gte, Eq, Neq,
    LogicalAnd, LogicalOr,
}

#[derive(Debug, PartialEq)]
pub enum Stmt {
    Return(Spanned<Expr>),
    Expr(Spanned<Expr>),
    VarDecl { name: String, init: Option<Spanned<Expr>> },
    If {
        cond: Spanned<Expr>,
        then_branch: Vec<Spanned<Stmt>>,
        else_branch: Vec<Spanned<Stmt>>,
    },
    While { cond: Spanned<Expr>, body: Vec<Spanned<Stmt>> },
    For {
        init: Box<Spanned<Stmt>>,
        cond: Spanned<Expr>,
        update: Box<Spanned<Stmt>>,
        body: Vec<Spanned<Stmt>>,
    },
}

#[derive(Debug, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub return_type: String,
    pub body: Vec<Spanned<Stmt>>,
    pub span: Span,
}

#[derive(Debug, PartialEq)]
pub struct Program {
    pub functions: Vec<Function>,
}

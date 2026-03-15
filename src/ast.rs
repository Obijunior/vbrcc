#[derive(Debug, PartialEq)]
pub enum Expr {
    IntLiteral(i64),
    StringLiteral(String),
    UnaryOp(UnaryOp, Box<Expr>),
    BinaryOp(BinaryOp, Box<Expr>, Box<Expr>),
    Var(String),
    FunctionCall { name: String, args: Vec<Expr> },
}

#[derive(Debug, PartialEq)]
pub enum UnaryOp {
    Negate,  // -
    BitNot,  // ~
    LogNot,  // !
}

#[derive(Debug, PartialEq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, PartialEq)]
pub enum Stmt {
    Return(Expr),
    Expr(Expr),
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
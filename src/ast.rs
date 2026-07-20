use crate::diagnostic::{Span, Spanned};

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Char,
    Long,
    Void,
    Pointer(Box<Type>),
    Array(Box<Type>, usize),
    Unknown,
}

impl Type {
    pub fn size(&self) -> usize {
        match self {
            Type::Array(elem, len) => elem.size() * len,
            _ => 8,
        }
    }

    /// Loose alignment: 8 for scalars, element alignment for arrays.
    pub fn align(&self) -> usize {
        match self {
            Type::Array(elem, _) => elem.align(),
            _ => 8,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loose_sizes_are_eight_for_scalars() {
        assert_eq!(Type::Int.size(), 8);
        assert_eq!(Type::Char.size(), 8);
        assert_eq!(Type::Long.size(), 8);
        assert_eq!(Type::Pointer(Box::new(Type::Int)).size(), 8);
    }

    #[test]
    fn array_size_is_element_times_len() {
        assert_eq!(Type::Array(Box::new(Type::Int), 10).size(), 80);
    }

    #[test]
    fn scalar_align_is_eight() {
        assert_eq!(Type::Int.align(), 8);
        assert_eq!(Type::Pointer(Box::new(Type::Char)).align(), 8);
    }
}
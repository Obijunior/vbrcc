//! The abstract syntax tree and the type system.
//!
//! Every stage after the parser works on these structures. [`Program`] holds a list of
//! [`Function`] values and a list of [`FuncDecl`] values, which are the prototypes. A
//! function body is a list of [`Stmt`], and a statement holds [`Expr`] trees. Each
//! expression sits in a [`TypedExpr`], which adds a [`Span`] and a [`Type`].
//!
//! `Program` keeps prototypes out of `functions` on purpose. [`crate::codegen`] emits a
//! label and a frame for each entry in `functions`, and a prototype must emit nothing.
//! Its symbol comes from the C runtime.
//!
//! The parser sets every `ty` field to [`Type::Unknown`], and [`crate::typeck`] fills
//! it in. By the time [`crate::codegen`] reads the tree, every expression has a real
//! type. The code generator uses that type to scale pointer arithmetic and to decide
//! when an array decays.
//!
//! # Type sizes
//!
//! [`Type::size`] and [`Type::align`] are the **one** place that decides type widths.

use crate::diagnostic::{Span, Spanned};

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Int,
    Char,
    Bool,
    Long,
    Void,
    Pointer(Box<Type>),
    Array(Box<Type>, usize),
    Struct { tag: Option<String>, fields: Vec<StructField>, size: usize, align: usize },
    Unknown,
}

impl Type {
    pub fn size(&self) -> usize {
        match self {
            Type::Char => 1,
            Type::Int => 4,
            Type::Bool => 1,
            Type::Long | Type::Pointer(_) | Type::Void => 8,
            Type::Array(elem, len) => elem.size() * len,
            Type::Struct { size, .. } => *size,
            Type::Unknown => 8,
        }
    }

    pub fn align(&self) -> usize {
        match self {
            Type::Char => 1,
            Type::Int => 4,
            Type::Bool => 1,
            Type::Long | Type::Pointer(_) | Type::Void => 8,
            Type::Array(elem, _) => elem.align(),
            Type::Struct { align, .. } => *align,
            Type::Unknown => 8,
        }
    }
    /// Array-to-pointer decay: an array used as a value becomes a pointer to its element.
    pub fn decay(&self) -> Type {
        match self {
            Type::Array(elem, _) => Type::Pointer(elem.clone()),
            other => other.clone(),
        }
    }

    /// The pointed-to type, after decay. `int*` -> `int`, `int[10]` -> `int`.
    pub fn pointee(&self) -> Option<Type> {
        match self.decay() {
            Type::Pointer(t) => Some(*t),
            _ => None,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Type::Int => "int".to_string(),
            Type::Char => "char".to_string(),
            Type::Bool => "_Bool".to_string(),
            Type::Long => "long".to_string(),
            Type::Void => "void".to_string(),
            Type::Pointer(t) => format!("{}*", t.describe()),
            Type::Array(t, n) => format!("{}[{}]", t.describe(), n),
            Type::Struct { tag, .. } => match tag {
                Some(name) => format!("struct {name}"),
                None => "struct <anonymous>".to_string(),
            },
            Type::Unknown => "<unknown>".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypedExpr {
    pub node: Expr,
    pub span: Span,
    pub ty: Type,
}

impl TypedExpr {
    pub fn new(node: Expr, span: Span) -> Self {
        TypedExpr { node, span, ty: Type::Unknown }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    IntLiteral(i64),
    StringLiteral(String),
    UnaryOp(UnaryOp, Box<TypedExpr>),
    BinaryOp(BinaryOp, Box<TypedExpr>, Box<TypedExpr>),
    Var(String), 
    FunctionCall { name: String, args: Vec<TypedExpr> },
    Assign(Box<TypedExpr>, Box<TypedExpr>), // lvalue = value
    AddressOf(Box<TypedExpr>),              // &expr
    Deref(Box<TypedExpr>),                  // *expr
    Index(Box<TypedExpr>, Box<TypedExpr>),  // base[idx]
    Cast(Type, Box<TypedExpr>),             // (T)expr
    PostIncDec(IncDec, Box<TypedExpr>),     // expr++ or expr--
    Member(Box<TypedExpr>, String),         // expr.field
}

#[derive(Debug, Clone, PartialEq)]
pub enum IncDec {
    Inc, // ++
    Dec, // --
}

impl IncDec {
    /// The operator as it appears in source, for diagnostics.
    pub fn describe(&self) -> &'static str {
        match self {
            IncDec::Inc => "++",
            IncDec::Dec => "--",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOp {
    Negate, // -
    BitNot, // ~
    LogNot, // !
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod,
    Lt, Lte, Gt, Gte, Eq, Neq,
    LogicalAnd, LogicalOr,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Return(TypedExpr),
    Expr(TypedExpr),
    VarDecl { ty: Type, name: String, init: Option<TypedExpr> },
    If {
        cond: TypedExpr,
        then_branch: Vec<Spanned<Stmt>>,
        else_branch: Vec<Spanned<Stmt>>,
    },
    While { cond: TypedExpr, body: Vec<Spanned<Stmt>> },
    For {
        init: Box<Spanned<Stmt>>,
        cond: TypedExpr,
        update: Box<Spanned<Stmt>>,
        body: Vec<Spanned<Stmt>>,
    },
}

/// A function definition: a name, parameters, return type, and body.
#[derive(Debug, PartialEq)]
pub struct Function {
    pub name: String,
    pub params: Vec<(Type, String)>,
    pub return_type: Type,
    pub body: Vec<Spanned<Stmt>>,
    pub span: Span,
}

/// A function declared but not defined here: a prototype.
#[derive(Debug, Clone, PartialEq)]
pub struct FuncDecl {
    pub name: String,
    pub return_type: Type,
    pub params: Vec<(Type, String)>,
    pub variadic: bool,
    pub span: Span,
}

/// The whole program: a list of functions, prototypes, and global variables.
#[derive(Debug, PartialEq)]
pub struct Program {
    pub functions: Vec<Function>,
    pub decls: Vec<FuncDecl>,
    pub globals: Vec<GlobalVar>,
}


/// A global variable, which may be initialized.
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalVar {
    pub ty: Type,
    pub name: String,
    pub init: Option<TypedExpr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: String,
    pub ty: Type,
    pub offset: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_sizes_are_c_widths() {
        assert_eq!(Type::Int.size(), 4);
        assert_eq!(Type::Char.size(), 1);
        assert_eq!(Type::Bool.size(), 1);
        assert_eq!(Type::Long.size(), 8);
        assert_eq!(Type::Pointer(Box::new(Type::Int)).size(), 8);
    }

    #[test]
    fn array_size_is_element_times_len() {
        assert_eq!(Type::Array(Box::new(Type::Int), 10).size(), 40);
    }

    #[test]
    fn scalar_align_is_c() {
        assert_eq!(Type::Int.align(), 4);
        assert_eq!(Type::Bool.align(), 1);
        assert_eq!(Type::Pointer(Box::new(Type::Char)).align(), 8);
    }
}
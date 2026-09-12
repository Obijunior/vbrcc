//! Stage 3: the type checker.
//!
//! [`check`] walks the AST from [`crate::parser`] and changes it in place. It writes a
//! resolved [`Type`] into the `ty` field of every `TypedExpr`. It runs before code
//! generation, so the code generator can trust that every expression has a type.
//!
//! # Errors reported here
//!
//! - A variable that no declaration introduces.
//! - A dereference of a value that is not a pointer.
//! - An index of a value that is not a pointer or an array.
//! - An assignment to something that is not an lvalue. An lvalue is a variable, a
//!   dereference, or an index. The parser accepts any expression to the left of `=`,
//!   and this stage rejects the invalid ones.
//! - A call with the wrong number of arguments. The signature comes from a prototype
//!   or from a definition in the same file. A variadic function needs at least its
//!   named parameters. A call to a name with no declaration is legal, because C89
//!   permits it, and programs written before `#include` worked depend on it.
//!
//! # Scope
//!
//! One flat `HashMap<String, Type>` holds the scope of a whole function. Block-level
//! scope does not exist yet. A variable declared inside an `if` or a loop body stays
//! visible after that block, and an inner declaration overwrites an outer one of the
//! same name. To add real scope, replace this map with a stack of maps and push a frame
//! for each block.

use crate::ast::*;
use crate::diagnostic::{CompileError, Spanned};
use std::collections::HashMap;

/// What a call site needs to know about a function.
struct Sig {
    return_type: Type,
    /// The count of named parameters. A variadic function may take more.
    arity: usize,
    variadic: bool,
}

type Sigs = HashMap<String, Sig>;

/// Every function this translation unit knows: the prototypes it declared and
/// the functions it defined.
///
/// A name that appears in neither is left alone. C89 lets a call stand without
/// a declaration, and every program written before `#include` worked did so.
fn signatures(program: &Program) -> Sigs {
    let mut sigs = Sigs::new();
    for d in &program.decls {
        sigs.insert(
            d.name.clone(),
            Sig { return_type: d.return_type.clone(), arity: d.params.len(), variadic: d.variadic },
        );
    }
    for f in &program.functions {
        sigs.insert(
            f.name.clone(),
            Sig { return_type: f.return_type.clone(), arity: f.params.len(), variadic: false },
        );
    }
    sigs
}

pub fn check(program: &mut Program) -> Result<(), CompileError> {
    let sigs = signatures(program);

    // File scope, pass 1: every global's declared type.
    let mut file_scope: HashMap<String, Type> = HashMap::new();
    for g in &program.globals {
        file_scope.insert(g.name.clone(), g.ty.clone());
    }

    // Pass 2: validate each initializer. This may refine an unsized
    // `char[]` length on `g.ty`.
    for g in &mut program.globals {
        check_global(g, &file_scope, &sigs)?;
    }

    // Pass 3: rebuild the file scope with refined types, then check bodies.
    let mut file_scope: HashMap<String, Type> = HashMap::new();
    for g in &program.globals {
        file_scope.insert(g.name.clone(), g.ty.clone());
    }
    for func in &mut program.functions {
        let mut scope: HashMap<String, Type> = file_scope.clone();
        for (ty, name) in &func.params {
            scope.insert(name.clone(), ty.clone());
        }
        check_block(&mut func.body, &mut scope, &sigs)?;
    }
    Ok(())
}

/// Type-check one global's initializer and require it to be a constant.
/// A `None` initializer is zero-initialized and needs no check.
fn check_global(
    g: &mut GlobalVar,
    file_scope: &HashMap<String, Type>,
    sigs: &Sigs,
) -> Result<(), CompileError> {
    let init = match &mut g.init {
        Some(e) => e,
        None => return Ok(()),
    };

    let mut scratch = file_scope.clone();

    // A brace list has its own shape rules. Each leaf must fold to a constant,
    // which `crate::codegen::gen_global_init` checks when it emits the data.
    if matches!(init.node, Expr::InitList(_)) {
        g.ty = check_initializer(init, &g.ty, &mut scratch, sigs)?;
        return Ok(());
    }

    check_expr(init, &mut scratch, sigs)?;

    match crate::constfold::eval_const(init)? {
        crate::constfold::ConstValue::Bytes(bytes) => match &g.ty {
            Type::Array(elem, declared) if **elem == Type::Char => {
                if *declared == 0 {
                    g.ty = Type::Array(Box::new(Type::Char), bytes.len());
                } else if bytes.len() > *declared {
                    return Err(CompileError::new(
                        format!(
                            "initializer-string for `{}` is too long: {} bytes into {}",
                            g.name,
                            bytes.len(),
                            declared
                        ),
                        init.span,
                    ));
                }
            }
            _ => {
                return Err(CompileError::new(
                    format!(
                        "a string initializer needs a `char` array, not `{}`",
                        g.ty.describe()
                    ),
                    init.span,
                ));
            }
        },
        crate::constfold::ConstValue::Int(_) => {
            let scalar = matches!(
                g.ty,
                Type::Int | Type::Char | Type::Bool | Type::Long | Type::LongLong | Type::Enum { .. } | Type::Pointer(_)
            );
            if !scalar {
                return Err(CompileError::new(
                    format!(
                        "invalid initializer for `{}` of type `{}`",
                        g.name,
                        g.ty.describe()
                    ),
                    init.span,
                ));
            }
        }
    }
    Ok(())
}

/// Match a declaration initializer against its declared type.
///
/// Returns the concrete type. That is `target` itself, except an unsized
/// outer array, whose length comes from the number of elements in the list.
/// Only the outer dimension can be unsized; C needs every inner dimension to
/// compute a row stride, and the parser already refuses an empty inner `[]`.
///
/// The list nests with the type: a `{...}` element goes against the element
/// type, so `int a[2][3]` takes two lists of three integers.
fn check_initializer(
    init: &mut TypedExpr,
    target: &Type,
    scope: &mut HashMap<String, Type>,
    sigs: &Sigs,
) -> Result<Type, CompileError> {
    let elems = match &mut init.node {
        Expr::InitList(elems) => elems,
        // A single expression. Valid for a scalar, wrong for an array.
        _ => {
            if matches!(target, Type::Array(_, _)) {
                let hint = if matches!(init.node, Expr::StringLiteral(_)) {
                    "a string initializer works only on a global `char` array"
                } else {
                    "write the elements in braces"
                };
                return Err(CompileError::new(
                    format!("`{}` needs a brace initializer", target.describe()),
                    init.span,
                )
                .with_label(hint));
            }
            check_expr(init, scope, sigs)?;
            return Ok(target.clone());
        }
    };

    let (elem_ty, declared) = match target {
        Type::Array(elem, len) => ((**elem).clone(), *len),
        _ => {
            return Err(CompileError::new(
                format!(
                    "a brace initializer needs an array type, not `{}`",
                    target.describe()
                ),
                init.span,
            ));
        }
    };

    // `declared == 0` is an unsized outer dimension waiting for a length.
    if declared != 0 && elems.len() > declared {
        return Err(CompileError::new(
            format!(
                "too many initializers: {} values into `{}`",
                elems.len(),
                target.describe()
            ),
            init.span,
        ));
    }

    for elem in elems.iter_mut() {
        check_initializer(elem, &elem_ty, scope, sigs)?;
    }

    let concrete = if declared == 0 {
        Type::Array(Box::new(elem_ty), elems.len())
    } else {
        target.clone()
    };
    init.ty = concrete.clone();
    Ok(concrete)
}

fn check_block(
    stmts: &mut [Spanned<Stmt>],
    scope: &mut HashMap<String, Type>,
    sigs: &Sigs,
) -> Result<(), CompileError> {
    for stmt in stmts {
        check_stmt(&mut stmt.node, scope, sigs)?;
    }
    Ok(())
}

fn check_stmt(
    stmt: &mut Stmt,
    scope: &mut HashMap<String, Type>,
    sigs: &Sigs,
) -> Result<(), CompileError> {
    match stmt {
        Stmt::Return(e) | Stmt::Expr(e) => check_expr(e, scope, sigs)?,
        Stmt::VarDecl { ty, name, init } => {
            if let Some(e) = init {
                // An unsized local array has no length to infer from, so the
                // returned type only differs for a list against `[0]`.
                *ty = check_initializer(e, ty, scope, sigs)?;
            }
            scope.insert(name.clone(), ty.clone());
        }
        Stmt::If { cond, then_branch, else_branch } => {
            check_expr(cond, scope, sigs)?;
            check_block(then_branch, scope, sigs)?;
            check_block(else_branch, scope, sigs)?;
        }
        Stmt::While { cond, body } => {
            check_expr(cond, scope, sigs)?;
            check_block(body, scope, sigs)?;
        }
        Stmt::For { init, cond, update, body } => {
            check_stmt(&mut init.node, scope, sigs)?;
            check_expr(cond, scope, sigs)?;
            check_stmt(&mut update.node, scope, sigs)?;
            check_block(body, scope, sigs)?;
        }
    }
    Ok(())
}

fn is_lvalue(e: &Expr) -> bool {
    matches!(e, Expr::Var(_) | Expr::Deref(_) | Expr::Index(_, _) | Expr::Member(_, _))
}

fn check_expr(
    expr: &mut TypedExpr,
    scope: &mut HashMap<String, Type>,
    sigs: &Sigs,
) -> Result<(), CompileError> {
    let span = expr.span;
    let ty: Type = match &mut expr.node {
        Expr::IntLiteral(_) => Type::Int,
        Expr::InitList(_) => {
            return Err(CompileError::new(
                "a brace initializer is only valid in a variable declaration",
                span,
            ));
        }
        Expr::StringLiteral(_) => Type::Pointer(Box::new(Type::Char)),
        Expr::Var(name) => scope.get(name).cloned().ok_or_else(|| {
            CompileError::new(format!("undefined variable `{name}`"), span)
                .with_label("not found in this scope")
        })?,
        Expr::UnaryOp(_, inner) => {
            check_expr(inner, scope, sigs)?;
            Type::Int
        }
        Expr::BinaryOp(op, l, r) => {
            check_expr(l, scope, sigs)?;
            check_expr(r, scope, sigs)?;
            if matches!(op, BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor
                          | BinaryOp::Shl | BinaryOp::Shr)
            {
                let is_int = |t: &Type| matches!(t, Type::Int | Type::Char | Type::Bool | Type::Long | Type::LongLong | Type::Enum { .. });
                if !is_int(&l.ty) || !is_int(&r.ty) {
                    return Err(CompileError::new(
                        format!(
                            "a bitwise operator needs integer operands, not `{}` and `{}`",
                            l.ty.describe(), r.ty.describe()
                        ),
                        span,
                    ));
                }
            }
            let lt = l.ty.decay();
            if matches!(lt, Type::Pointer(_)) && matches!(op, BinaryOp::Add | BinaryOp::Sub) {
                lt
            } else {
                Type::Int
            }
        }
        Expr::AddressOf(inner) => {
            check_expr(inner, scope, sigs)?;
            if !is_lvalue(&inner.node) {
                return Err(CompileError::new("cannot take the address of this expression", span)
                    .with_label("not an lvalue"));
            }
            Type::Pointer(Box::new(inner.ty.clone()))
        }
        Expr::Deref(inner) => {
            check_expr(inner, scope, sigs)?;
            match inner.ty.pointee() {
                Some(t) => t,
                None => {
                    return Err(CompileError::new(
                        format!("cannot dereference value of type `{}`", inner.ty.describe()),
                        span,
                    )
                    .with_label("expected a pointer"));
                }
            }
        }
        Expr::Index(base, idx) => {
            check_expr(base, scope, sigs)?;
            check_expr(idx, scope, sigs)?;
            match base.ty.pointee() {
                Some(t) => t,
                None => {
                    return Err(CompileError::new(
                        format!("cannot index value of type `{}`", base.ty.describe()),
                        span,
                    )
                    .with_label("expected a pointer or array"));
                }
            }
        }
        Expr::Cast(t, inner) => {
            check_expr(inner, scope, sigs)?;
            t.clone()
        }
        Expr::PostIncDec(op, inner) => {
            check_expr(inner, scope, sigs)?;
            if !is_lvalue(&inner.node) {
                return Err(CompileError::new(
                    format!("cannot apply `{}` to this expression", op.describe()),
                    span,
                )
                .with_label("not an lvalue"));
            }
            // `x++` has the type of `x`, and yields the value it held before.
            inner.ty.clone()
        }
        Expr::Assign(lval, rhs) => {
            check_expr(lval, scope, sigs)?;
            check_expr(rhs, scope, sigs)?;
            if !is_lvalue(&lval.node) {
                return Err(CompileError::new("cannot assign to this expression", span)
                    .with_label("not an lvalue"));
            }
            lval.ty.clone()
        }
        Expr::FunctionCall { name, args } => {
            for a in args.iter_mut() {
                check_expr(a, scope, sigs)?;
            }
            match sigs.get(name) {
                None => Type::Int,
                Some(sig) => {
                    let ok = if sig.variadic {
                        args.len() >= sig.arity
                    } else {
                        args.len() == sig.arity
                    };
                    if !ok {
                        return Err(CompileError::new(
                            format!(
                                "`{name}` takes {}{} argument{}, but {} given",
                                if sig.variadic { "at least " } else { "" },
                                sig.arity,
                                if sig.arity == 1 { "" } else { "s" },
                                args.len()
                            ),
                            span,
                        ));
                    }
                    sig.return_type.clone()
                }
            }
        }
        Expr::Member(base, field) => {
            check_expr(base, scope, sigs)?;
            let fields = match &base.ty {
                Type::Struct { fields, .. } => fields,
                other => {
                    return Err(CompileError::new(
                        format!("`{}` is not a struct", other.describe()),
                        span,
                    )
                    .with_label("has no members"));
                }
            };
            match fields.iter().find(|f| f.name == *field) {
                Some(f) => f.ty.clone(),
                None => {
                    return Err(CompileError::new(
                        format!("no member `{field}` on `{}`", base.ty.describe()),
                        span,
                    ));
                }
            }
        }
        Expr::Ternary(cond, then_e, else_e) => {
            check_expr(cond, scope, sigs)?;
            check_expr(then_e, scope, sigs)?;
            check_expr(else_e, scope, sigs)?;
            let is_int = |t: &Type| matches!(t, Type::Int | Type::Char | Type::Bool | Type::Long | Type::LongLong | Type::Enum { .. });
            if then_e.ty == else_e.ty {
                then_e.ty.clone()
            } else if is_int(&then_e.ty) && is_int(&else_e.ty) {
                Type::Int
            } else {
                then_e.ty.clone()
            }
        }
    };
    expr.ty = ty;
    Ok(())
}


/* ===================================== */
//                                       //
//        Unit tests for type checker    //
//                                       // 
/* ===================================== */

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;
    use crate::parser::Parser;

    fn typecheck(src: &str) -> Result<Program, CompileError> {
        let tokens = Lexer::new(src).tokenize().unwrap();
        let mut program = Parser::new(tokens).parse_program().unwrap();
        check(&mut program)?;
        Ok(program)
    }

    /// Parse `src` without type-checking it, so a test can run `check` itself
    /// and inspect the error (or success) directly.
    fn parse(src: &str) -> Program {
        let tokens = Lexer::new(src).tokenize().unwrap();
        Parser::new(tokens).parse_program().unwrap()
    }

    #[test]
    fn initializer_list_length_must_fit() {
        let mut program = parse("int main() { int a[2] = {1, 2, 3}; return 0; }");
        let err = check(&mut program).unwrap_err();
        assert!(err.message.contains("too many"), "got: {}", err.message);
    }

    #[test]
    fn an_array_needs_a_brace_initializer() {
        let mut program = parse("int main() { int a[3] = 5; return 0; }");
        let err = check(&mut program).unwrap_err();
        assert!(err.message.contains("brace initializer"), "got: {}", err.message);
    }

    #[test]
    fn a_scalar_rejects_a_brace_initializer() {
        let mut program = parse("int main() { int x = {1}; return 0; }");
        let err = check(&mut program).unwrap_err();
        assert!(err.message.contains("array type"), "got: {}", err.message);
    }

    #[test]
    fn nested_initializer_list_type_checks() {
        let mut program = parse("int main() { int a[2][2] = {{1, 2}, {3, 4}}; return 0; }");
        assert!(check(&mut program).is_ok());
    }

    #[test]
    fn a_nested_list_too_deep_is_rejected() {
        let mut program = parse("int main() { int a[2] = {{1}, {2}}; return 0; }");
        let err = check(&mut program).unwrap_err();
        assert!(err.message.contains("array type"), "got: {}", err.message);
    }

    #[test]
    fn unsized_global_array_length_is_inferred_from_its_initializer() {
        let mut program = parse("int g[] = {1, 2, 3, 4};");
        check(&mut program).unwrap();
        assert_eq!(program.globals[0].ty, Type::Array(Box::new(Type::Int), 4));
    }

    /// The element count fills only the outer dimension, so the type keeps
    /// the declared row width.
    #[test]
    fn unsized_global_array_of_rows_infers_only_the_outer_length() {
        let mut program = parse("int g[][2] = {{1, 2}, {3, 4}, {5, 6}};");
        check(&mut program).unwrap();
        assert_eq!(
            program.globals[0].ty,
            Type::Array(Box::new(Type::Array(Box::new(Type::Int), 2)), 3)
        );
    }

    /// A local declaration must carry the checked type forward, because
    /// codegen sizes the frame slot from it.
    #[test]
    fn a_local_initializer_list_leaves_the_declared_type_intact() {
        let mut program = parse("int main() { int a[3] = {1}; return 0; }");
        check(&mut program).unwrap();
        match &program.functions[0].body[0].node {
            Stmt::VarDecl { ty, .. } => {
                assert_eq!(*ty, Type::Array(Box::new(Type::Int), 3));
            }
            other => panic!("expected VarDecl, got {other:?}"),
        }
    }

    #[test]
    fn a_call_with_the_wrong_argument_count_is_rejected() {
        let err = typecheck("int add(int a, int b) { return a + b; }\n\
                             int main() { return add(1); }").unwrap_err();
        assert!(err.message.contains("takes 2 arguments"), "got: {}", err.message);
    }

    #[test]
    fn a_call_matching_its_prototype_is_accepted() {
        assert!(typecheck("int puts(const char *s);\n\
                           int main() { return puts(\"hi\"); }").is_ok());
    }

    #[test]
    fn a_variadic_call_needs_only_its_named_arguments() {
        assert!(typecheck("int printf(const char *fmt, ...);\n\
                           int main() { printf(\"hi\"); return printf(\"%d\", 1); }").is_ok());
    }

    #[test]
    fn a_variadic_call_still_needs_its_named_arguments() {
        let err = typecheck("int printf(const char *fmt, ...);\n\
                             int main() { return printf(); }").unwrap_err();
        assert!(err.message.contains("at least 1"), "got: {}", err.message);
    }

    #[test]
    fn a_call_to_an_undeclared_function_is_still_allowed() {
        // Every program written before `#include` worked calls printf bare.
        assert!(typecheck("int main() { return printf(\"hi\"); }").is_ok());
    }

    #[test]
    fn a_call_takes_the_declared_return_type() {
        let program = typecheck("char *strcpy(char *d, const char *s);\n\
                                 int main() { char *p = strcpy(0, 0); return 0; }").unwrap();
        match &program.functions[0].body[0].node {
            Stmt::VarDecl { init: Some(e), .. } => {
                assert_eq!(e.ty, Type::Pointer(Box::new(Type::Char)));
            }
            other => panic!("expected a declaration, got {other:?}"),
        }
    }

    #[test]
    fn well_typed_program_annotates_int() {
        let program = typecheck("int main() { int x = 5; return x; }").unwrap();
        let body = &program.functions[0].body;
        match &body[1].node {
            Stmt::Return(e) => assert_eq!(e.ty, Type::Int),
            other => panic!("expected return, got {:?}", other),
        }
    }

    #[test]
    fn undefined_variable_is_located_error() {
        let src = "int main() { return y; }";
        let err = typecheck(src).unwrap_err();
        assert!(err.message.contains('y'), "message: {}", err.message);
        assert_eq!(err.span.start, src.find('y').unwrap());
    }

        #[test]
    fn address_of_yields_pointer() {
        let program = typecheck("int main() { int x = 1; int *p = &x; return 0; }").unwrap();
        // the initializer `&x` on body[1] is a pointer-to-int
        if let Stmt::VarDecl { init: Some(e), .. } = &program.functions[0].body[1].node {
            assert_eq!(e.ty, Type::Pointer(Box::new(Type::Int)));
        } else { panic!("expected var decl with init"); }
    }

    #[test]
    fn deref_of_non_pointer_is_error() {
        let src = "int main() { int x = 1; return *x; }";
        let err = typecheck(src).unwrap_err();
        assert!(err.message.contains("dereference"), "message: {}", err.message);
    }

    #[test]
    fn index_of_non_pointer_is_error() {
        let src = "int main() { int x = 1; return x[0]; }";
        let err = typecheck(src).unwrap_err();
        assert!(err.message.contains("index"), "message: {}", err.message);
    }

    #[test]
    fn assign_to_non_lvalue_is_error() {
        let src = "int main() { 1 = 2; return 0; }";
        let err = typecheck(src).unwrap_err();
        assert!(err.message.contains("assign"), "message: {}", err.message);
    }

    #[test]
    fn a_constant_folded_global_initializer_is_accepted() {
        let program = typecheck("int g = 2 + 3 * 4; int main() { return g; }").unwrap();
        match &program.functions[0].body[0].node {
            Stmt::Return(e) => assert_eq!(e.ty, Type::Int),
            other => panic!("expected return, got {other:?}"),
        }
    }

    #[test]
    fn a_non_constant_global_initializer_is_rejected() {
        let err = typecheck("int f() { return 1; } int g = f(); int main() { return g; }")
            .unwrap_err();
        assert!(err.message.contains("not a constant"), "got: {}", err.message);
    }

    #[test]
    fn a_global_that_reads_another_global_is_not_constant() {
        let err = typecheck("int a = 1; int b = a; int main() { return b; }").unwrap_err();
        assert!(err.message.contains("not a constant"), "got: {}", err.message);
    }

    #[test]
    fn an_unsized_char_array_takes_its_length_from_the_string() {
        let program = typecheck("char s[] = \"abc\"; int main() { return 0; }").unwrap();
        assert_eq!(program.globals[0].ty, Type::Array(Box::new(Type::Char), 4));
    }

    #[test]
    fn a_string_initializer_that_overflows_its_array_is_rejected() {
        let err = typecheck("char s[2] = \"abc\"; int main() { return 0; }").unwrap_err();
        assert!(err.message.contains("too long"), "got: {}", err.message);
    }

    #[test]
    fn a_global_is_visible_inside_a_function() {
        assert!(typecheck("int counter = 3; int main() { return counter; }").is_ok());
    }

    #[test]
    fn a_local_shadows_a_global_of_the_same_name() {
        assert!(typecheck("int x = 1; int main() { int x = 2; return x; }").is_ok());
    }

    #[test]
    fn an_int_initializer_for_an_array_is_rejected() {
        let err = typecheck("int a[2] = 5; int main() { return 0; }").unwrap_err();
        assert!(err.message.contains("invalid initializer"), "got: {}", err.message);
    }
    
    #[test]
    fn member_access_types_to_the_field() {
        let src = "struct P { int x; long y; } ; int main() { struct P p; return p.x; }";
        let mut program = parse(src);
        assert!(super::check(&mut program).is_ok());
    }

    #[test]
    fn unknown_member_is_an_error() {
        let src = "struct P { int x; } ; int main() { struct P p; return p.z; }";
        let mut program = parse(src);
        let err = super::check(&mut program).unwrap_err();
        assert!(err.message.contains("no member `z`"), "got: {}", err.message);
    }

    #[test]
    fn member_of_a_non_struct_is_an_error() {
        let src = "int main() { int n; return n.x; }";
        let mut program = parse(src);
        let err = super::check(&mut program).unwrap_err();
        assert!(err.message.contains("not a struct"), "got: {}", err.message);
    }

    #[test]
    fn arrow_through_a_struct_pointer_types() {
        let src = "struct P { int x; } ; int get(struct P *p) { return p->x; }";
        let mut program = parse(src);
        assert!(super::check(&mut program).is_ok());
    }

    #[test]
    fn whole_struct_assignment_type_checks() {
        let src = "struct P { int x; int y; } ; \
                   int main() { struct P a; struct P b; b = a; return 0; }";
        let mut program = parse(src);
        assert!(super::check(&mut program).is_ok());
    }

    #[test]
    fn bitwise_on_ints_is_int() {
        let program = typecheck("int main() { int a = 6; int b = 3; return a & b; }").unwrap();
        match &program.functions[0].body[2].node {
            Stmt::Return(e) => assert_eq!(e.ty, Type::Int),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn bitwise_on_a_pointer_is_an_error() {
        let err = typecheck("int main() { int x; int *p = &x; return p & 1; }").unwrap_err();
        assert!(err.message.contains("bitwise"), "got: {}", err.message);
    }

    #[test]
    fn ternary_of_two_ints_is_int() {
        let program = typecheck("int main() { int a = 1; return a ? 2 : 3; }").unwrap();
        match &program.functions[0].body[1].node {
            Stmt::Return(e) => assert_eq!(e.ty, Type::Int),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn ternary_of_two_pointers_keeps_the_pointer_type() {
        let program = typecheck(
            "int main() { int x; int y; int *p = &x; int *q = &y; int *r = (1 ? p : q); return 0; }"
        ).unwrap();
        match &program.functions[0].body[4].node {
            Stmt::VarDecl { init: Some(e), .. } => {
                assert_eq!(e.ty, Type::Pointer(Box::new(Type::Int)));
            }
            other => panic!("got {other:?}"),
        }
    }

}
use crate::ast::*;
use crate::diagnostic::{CompileError, Spanned};
use std::collections::HashMap;

pub struct Codegen {
    output: String,
    data_section: String,
    string_count: usize,
    variables: HashMap<String, i64>, // name -> rbp offset
    stack_offset: i64, 
    label_count: usize,
}

impl Codegen {
    pub fn new() -> Self {
        Codegen { 
            output: String::new(), 
            data_section: String::new(),
            string_count: 0,
            variables: HashMap::new(),
            stack_offset: 0,
            label_count: 0,
        }
    }

    fn emit(&mut self, line: &str) {
        self.output.push_str(line);
        self.output.push('\n');
    }

    fn emit_data(&mut self, line: &str) {
        self.data_section.push_str(line);
        self.data_section.push('\n');
    }

    fn add_string(&mut self, s: &str) -> String {
        let label = format!("str_{}", self.string_count);
        self.string_count += 1;
        self.emit_data(&format!("  .section .data"));
        self.emit_data(&format!("{}:", label));
        self.emit_data(&format!("    .ascii \"{}\\0\"", s.escape_default()));
        label
    }
    
    fn align_up(value: i64, align: i64) -> i64 {
        (value + align - 1) & !(align - 1)
    }

    fn emit_epilogue(&mut self) {
        self.emit("  mov rsp, rbp");
        self.emit("  pop rbp");
        self.emit("  ret");
    }

    pub fn generate(&mut self, program: &Program) -> Result<String, CompileError> {
        // Reserve space for data section (filled in as we go)
        for function in &program.functions {
            self.gen_function(function)?;
        }

        // Assemble final output: data section first, then text
        let mut final_output = String::new();
        
        if !self.data_section.is_empty() {
            final_output.push_str(".section .data\n");
            final_output.push_str(&self.data_section);
            final_output.push('\n');
        }

        final_output.push_str(".section .text\n");
        final_output.push_str(&self.output);

        Ok(final_output)
    }


    fn gen_function(&mut self, func: &Function) -> Result<(), CompileError> {
        self.variables.clear();
        self.stack_offset = 0;

        // Header + prologue, up to but NOT including the frame reservation.
        self.emit("  .intel_syntax noprefix");
        self.emit(&format!("  .globl {}", func.name));
        self.emit(&format!("{}:", func.name));
        self.emit("  push rbp");
        self.emit("  mov rbp, rsp");

        // Divert emission into a scratch buffer while we generate params + body,
        // so stack_offset reaches its final (most-negative) value before we size the frame.
        let outer = std::mem::take(&mut self.output);

        let arg_regs = ["rcx", "rdx", "r8", "r9"];
        for (i, param) in func.params.iter().enumerate() {
            if i >= arg_regs.len() {
                return Err(CompileError::new(
                    format!("functions with more than {} parameters are not supported", arg_regs.len()),
                    func.span,
                ));
            }
            self.stack_offset -= 8;
            let offset = self.stack_offset;
            self.variables.insert(param.clone(), offset);
            self.emit(&format!("  mov [rbp - {}], {}", -offset, arg_regs[i]));
        }

        for stmt in &func.body {
            self.gen_statement(stmt)?;
        }

        let body = std::mem::replace(&mut self.output, outer);

        // Frame = locals/params bytes + 32 shadow space, rounded up to 16-byte alignment.
        let locals_bytes = -self.stack_offset;            // >= 0
        let frame = Codegen::align_up(locals_bytes + 32, 16);
        self.emit(&format!("  sub rsp, {}", frame));
        self.output.push_str(&body);

        // fail-safe epilogue in case the function doesn't return explicitly
        self.emit("  xor rax, rax");
        self.emit_epilogue();

        Ok(())
    }
    fn gen_statement(&mut self, stmt: &Spanned<Stmt>) -> Result<(), CompileError> {
        match &stmt.node {
            Stmt::Return(expr) => {
                self.gen_expr(expr)?;
                self.emit_epilogue();
            }
            Stmt::For { init, cond, update, body } => {
                let id = self.label_count;
                self.label_count += 1;

                self.gen_statement(init)?; // init
                self.emit(&format!("loop_{}_start:", id)); // loop header label

                // condition
                self.gen_expr(cond)?;
                self.emit("  cmp rax, 0");
                self.emit(&format!("  je loop_{}_end", id)); // exit if condition is false

                for stmt in body {
                    self.gen_statement(stmt)?; // loop body
                }
                self.gen_statement(update)?; // update (e.g. i++)
                self.emit(&format!("  jmp loop_{}_start", id)); // jump back to loop header
                self.emit(&format!("loop_{}_end:", id)); // loop exit label

            }
            Stmt::Expr(expr) => {
                self.gen_expr(expr)?;
            }
            Stmt::VarDecl { name, init } => {
                self.stack_offset -= 8;
                let offset = self.stack_offset;
                self.variables.insert(name.clone(), offset);
                if let Some(expr) = init {
                    self.gen_expr(expr)?;
                    self.emit(&format!("  mov [rbp - {}], rax", -offset));
                }
            }
            Stmt::While { cond, body } => {
                let id = self.label_count;
                self.label_count += 1;

                self.emit(&format!("loop_{}_start:", id));
                self.gen_expr(cond)?;
                self.emit("  cmp rax, 0");
                self.emit(&format!("  je loop_{}_end", id));

                for stmt in body {
                    self.gen_statement(stmt)?;
                }

                self.emit(&format!("  jmp loop_{}_start", id));
                self.emit(&format!("loop_{}_end:", id));
            }
            Stmt::If { cond, then_branch, else_branch } => {
                let id = self.label_count;
                self.label_count += 1;

                self.gen_expr(cond)?;
                self.emit("  cmp rax, 0");

                if else_branch.is_empty() {
                    self.emit(&format!("  je if_{}_end", id));
                    for stmt in then_branch {
                        self.gen_statement(stmt)?;
                    }
                    self.emit(&format!("if_{}_end:", id));
                } else {
                    self.emit(&format!("  je if_{}_else", id));
                    for stmt in then_branch {
                        self.gen_statement(stmt)?;
                    }
                    self.emit(&format!("  jmp if_{}_end", id));
                    self.emit(&format!("if_{}_else:", id));
                    for stmt in else_branch {
                        self.gen_statement(stmt)?;
                    }
                    self.emit(&format!("if_{}_end:", id));
                }
            }
        }
        Ok(())
    }

    fn gen_expr(&mut self, expr: &TypedExpr) -> Result<(), CompileError> {
        match &expr.node {
            Expr::IntLiteral(n) => {
                // Move the literal directly into %rax
                self.emit(&format!("  mov rax, {}", n));
            }

            Expr::StringLiteral(s) => {
                let label = self.add_string(s);
                self.emit(&format!("  lea rax, [rip + {}]", label));
            }

            Expr::FunctionCall {name, args} => {
                // Windows x64 calling convention: RCX, RDX, R8, R9
                let arg_regs = ["rcx", "rdx", "r8", "r9"];
                if args.len() > arg_regs.len() {
                    return Err(CompileError::new(
                        format!("function calls with more than {} arguments are not supported", arg_regs.len()),
                        expr.span,
                    ));
                }

                for (i, arg) in args.iter().enumerate() {
                    self.gen_expr(arg)?;
                    self.emit(&format!("  mov {}, rax", arg_regs[i]));
                    // Home space for varargs: spill register args into shadow space
                    self.emit(&format!("  mov [rsp + {}], {}", i * 8, arg_regs[i]));
                }

                self.emit(&format!("  call {}", name));
            }

            Expr::UnaryOp(op, inner) => {
                // Evaluate the inner expression first (result in %rax)
                self.gen_expr(inner)?;
                match op {
                    UnaryOp::Negate => self.emit("  neg rax"),
                    UnaryOp::BitNot => self.emit("  not rax"),
                    UnaryOp::LogNot => {
                        // !x: set %rax to 1 if %rax == 0, else 0
                        self.emit("  cmp rax, 0");
                        self.emit("  mov rax, 0");
                        self.emit("  sete al");
                    }
                }
            }

            Expr::BinaryOp(op, left, right) => {
                // logical AND and OR short-circuiting
                // they aren't really binary operations in the same sense as +, -, *, /, etc, so we short circuit them here 
                // before evaluating the left and right operands
                
                if *op == BinaryOp::LogicalAnd {
                    let id = self.label_count;
                    self.label_count += 1;
                    self.gen_expr(left)?;
                    self.emit("  cmp rax, 0");
                    self.emit(&format!("  je and_{}_false", id));
                    self.gen_expr(right)?;
                    self.emit("  cmp rax, 0");
                    self.emit(&format!("  je and_{}_false", id));
                    self.emit("  mov rax, 1");
                    self.emit(&format!("  jmp and_{}_end", id));
                    self.emit(&format!("and_{}_false:", id));
                    self.emit("  mov rax, 0");
                    self.emit(&format!("and_{}_end:", id));
                    return Ok(());
                }
                if *op == BinaryOp::LogicalOr {
                    let id = self.label_count;
                    self.label_count += 1;
                    self.gen_expr(left)?;
                    self.emit("  cmp rax, 0");
                    self.emit(&format!("  jne or_{}_true", id));
                    self.gen_expr(right)?;
                    self.emit("  cmp rax, 0");
                    self.emit(&format!("  jne or_{}_true", id));
                    self.emit("  mov rax, 0");
                    self.emit(&format!("  jmp or_{}_end", id));
                    self.emit(&format!("or_{}_true:", id));
                    self.emit("  mov rax, 1");
                    self.emit(&format!("or_{}_end:", id));
                    return Ok(());
                }
                // Evaluate left, push to stack
                // Evaluate right, result in rax
                // Pop left into rcx, perform op
                self.gen_expr(left)?;
                self.emit("  push rax");
                self.gen_expr(right)?;
                self.emit("  mov rcx, rax"); // right operand in rcx
                self.emit("  pop rax");        // left operand back in rax

                match op {
                    BinaryOp::Add => self.emit("  add rax, rcx"),
                    BinaryOp::Sub => self.emit("  sub rax, rcx"),
                    BinaryOp::Mul => self.emit("  imul rax, rcx"),
                    BinaryOp::Div => {
                        // idivq divides rdx:rax by the operand
                        // cqo sign-extends rax into rdx first
                        self.emit("  cqo");
                        self.emit("  idiv rcx");
                        // quotient is left in rax automatically
                    }
                    BinaryOp::Mod => {
                        self.emit("  cqo");
                        self.emit("  idiv rcx");
                        // remainder is left in rdx
                        self.emit("  mov rax, rdx");
                    }
                    BinaryOp::Eq => {
                        self.emit("  cmp rax, rcx");
                        self.emit("  sete al"); // set low byte to 1 if equal, else 0
                        self.emit("  movzx rax, al"); // zero-extend to full 64-bit
                    }
                    BinaryOp::Neq => {
                        self.emit("  cmp rax, rcx");
                        self.emit("  setne al"); // set low byte to 1 if not equal, else 0
                        self.emit("  movzx rax, al"); // zero-extend to full 64-bit
                    }
                    BinaryOp::Lt => {
                        self.emit("  cmp rax, rcx");
                        self.emit("  setl al"); // set low byte to 1 if rax < rcx, else 0
                        self.emit("  movzx rax, al"); // zero-extend to full 64-bit
                    }
                    BinaryOp::Lte => {
                        self.emit("  cmp rax, rcx");
                        self.emit("  setle al");
                        self.emit("  movzx rax, al");
                    }
                    BinaryOp::Gt => {
                        self.emit("  cmp rax, rcx");
                        self.emit("  setg al");
                        self.emit("  movzx rax, al");
                    }
                    BinaryOp::Gte => {
                        self.emit("  cmp rax, rcx");
                        self.emit("  setge al");
                        self.emit("  movzx rax, al");
                    }
                    BinaryOp::LogicalAnd | BinaryOp::LogicalOr => unreachable!(),
                }
            }
            Expr::Assign(name, value) => {
                self.gen_expr(value)?;
                let offset = *self.variables.get(name).ok_or_else(|| {
                    CompileError::new(format!("undefined variable `{name}`"), expr.span)
                        .with_label("not found in this scope")
                })?;
                self.emit(&format!("  mov [rbp - {}], rax", -offset));
            }
            Expr::Var(name) => {
                let offset = *self.variables.get(name).ok_or_else(|| {
                    CompileError::new(format!("undefined variable `{name}`"), expr.span)
                        .with_label("not found in this scope")
                })?;
                self.emit(&format!("  mov rax, [rbp - {}]", -offset));
            }
        }
        Ok(())
    }
}

/*********************************
*           UNIT TESTS           *
**********************************/

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(source: &str) -> String {
        let mut lexer = crate::lexer::Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = crate::parser::Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let mut codegen = Codegen::new();
        codegen.generate(&program).unwrap()
    }

    fn compile_err(source: &str) -> crate::diagnostic::CompileError {
        let mut lexer = crate::lexer::Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = crate::parser::Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let mut codegen = Codegen::new();
        codegen.generate(&program).unwrap_err()
    }

    #[test]
    fn undefined_variable_error_points_at_identifier() {
        let src = "int main() { return y; }";
        let err = compile_err(src);
        assert!(err.message.contains('y'), "message: {}", err.message);
        assert_eq!(err.span.start, src.find('y').unwrap()); // offset 20
    }

    #[test]
    fn test_return_literal() {
        let asm = compile("int main() { return 42; }");
        assert!(asm.contains("mov rax, 42"));
        assert!(asm.contains("ret"));
    }

    #[test]
    fn test_negate() {
        let asm = compile("int main() { return -42; }");
        assert!(asm.contains("mov rax, 42"));
        assert!(asm.contains("neg rax"));
    }

    #[test]
    fn test_addition() {
        let asm = compile("int main() { return 1 + 2; }");
        assert!(asm.contains("add rax, rcx"));
    }

    #[test]
    fn test_division() {
        let asm = compile("int main() { return 10 / 2; }");
        assert!(asm.contains("idiv rcx"));
    }

    #[test]
    fn test_var_decl_and_return() {
        let asm = compile("int main() { int x = 5; return x; }");
        assert!(asm.contains("mov rax, 5"));
        assert!(asm.contains("mov [rbp - 8], rax"));
        assert!(asm.contains("mov rax, [rbp - 8]"));
        assert!(asm.contains("ret"));
    }

    #[test]
    fn test_assignment() {
        let asm = compile("int main() { int x = 1; x = 2; return x; }");
        assert!(asm.contains("mov rax, 1"));
        assert!(asm.contains("mov rax, 2"));
        assert!(asm.contains("ret"));
    }

    #[test]
    fn test_less_than_comparison() {
        let asm = compile("int main() { return 1 < 2; }");
        assert!(asm.contains("cmp rax, rcx"));
        assert!(asm.contains("setl al"));
        assert!(asm.contains("movzx rax, al"));
    }

    #[test]
    fn test_equal_comparison() {
        let asm = compile("int main() { return 1 == 2; }");
        assert!(asm.contains("cmp rax, rcx"));
        assert!(asm.contains("sete al"));
        assert!(asm.contains("movzx rax, al"));
    }

    #[test]
    fn test_not_equal_comparison() {
        let asm = compile("int main() { return 1 != 2; }");
        assert!(asm.contains("setne al"));
    }

    #[test]
    fn test_less_equal_comparison() {
        let asm = compile("int main() { return 1 <= 2; }");
        assert!(asm.contains("setle al"));
    }

    #[test]
    fn test_greater_than_comparison() {
        let asm = compile("int main() { return 1 > 2; }");
        assert!(asm.contains("setg al"));
    }

    #[test]
    fn test_greater_equal_comparison() {
        let asm = compile("int main() { return 1 >= 2; }");
        assert!(asm.contains("setge al"));
    }

    #[test]
    fn test_for_loop_generates_labels_and_jumps() {
        let asm = compile("int main() { int s = 0; for (int i = 0; i < 10; i++) { s += i; } return s; }");
        assert!(asm.contains("loop_0_start:"));
        assert!(asm.contains("je loop_0_end"));
        assert!(asm.contains("jmp loop_0_start"));
        assert!(asm.contains("loop_0_end:"));
    }

    #[test]
    fn test_while_loop_generates_labels_and_jumps() {
        let asm = compile("int main() { int i = 0; while (i < 5) { i++; } return i; }");
        assert!(asm.contains("loop_0_start:"));
        assert!(asm.contains("je loop_0_end"));
        assert!(asm.contains("jmp loop_0_start"));
        assert!(asm.contains("loop_0_end:"));
    }

    #[test]
    fn test_if_without_else() {
        let asm = compile("int main() { int x = 0; if (x < 1) { x = 1; } return x; }");
        assert!(asm.contains("je if_0_end"));
        assert!(asm.contains("if_0_end:"));
        assert!(!asm.contains("if_0_else:"));
    }

    #[test]
    fn test_if_with_else() {
        let asm = compile("int main() { int x = 0; if (x < 1) { x = 1; } else { x = 2; } return x; }");
        assert!(asm.contains("je if_0_else"));
        assert!(asm.contains("jmp if_0_end"));
        assert!(asm.contains("if_0_else:"));
        assert!(asm.contains("if_0_end:"));
    }

    #[test]
    fn test_modulo() {
        let asm = compile("int main() { return 10 % 3; }");
        assert!(asm.contains("idiv rcx"));
        assert!(asm.contains("mov rax, rdx"));
    }

    #[test]
    fn test_logical_and() {
        let asm = compile("int main() { return 1 && 2; }");
        assert!(asm.contains("je and_0_false"));
        assert!(asm.contains("and_0_false:"));
        assert!(asm.contains("and_0_end:"));
        assert!(!asm.contains("push rax"), "logical AND must not use the evaluate-both-sides pattern");
    }

    #[test]
    fn test_logical_or() {
        let asm = compile("int main() { return 0 || 1; }");
        assert!(asm.contains("jne or_0_true"));
        assert!(asm.contains("or_0_true:"));
        assert!(asm.contains("or_0_end:"));
        assert!(!asm.contains("push rax"), "logical OR must not use the evaluate-both-sides pattern");
    }
}

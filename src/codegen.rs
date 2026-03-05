use crate::ast::*;

pub struct Codegen {
    output: String,
}

impl Codegen {
    pub fn new() -> Self {
        Codegen { output: String::new() }
    }

    fn emit(&mut self, line: &str) {
        self.output.push_str(line);
        self.output.push('\n');
    }

    pub fn generate(&mut self, program: &Program) -> Result<String, String> {
        for function in &program.functions {
            self.gen_function(function)?;
        }
        Ok(self.output.clone())
    }

    fn gen_function(&mut self, func: &Function) -> Result<(), String> {
        // Emit the function label, visible to the linker
        self.emit(&format!("  .globl {}", func.name));
        self.emit(&format!("{}:", func.name));

        // Function prologue — set up the stack frame
        self.emit("  pushq %rbp");
        self.emit("  movq %rsp, %rbp");

        for stmt in &func.body {
            self.gen_statement(stmt)?;
        }

        Ok(())
    }

    fn gen_statement(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Return(expr) => {
                // Evaluate expr, result ends up in %rax
                self.gen_expr(expr)?;

                // Function epilogue — restore stack and return
                self.emit("  popq %rbp");
                self.emit("  ret");
            }
        }
        Ok(())
    }

    fn gen_expr(&mut self, expr: &Expr) -> Result<(), String> {
        match expr {
            Expr::IntLiteral(n) => {
                // Move the literal directly into %rax
                self.emit(&format!("  movq ${}, %rax", n));
            }

            Expr::UnaryOp(op, inner) => {
                // Evaluate the inner expression first (result in %rax)
                self.gen_expr(inner)?;
                match op {
                    UnaryOp::Negate => self.emit("  negq %rax"),
                    UnaryOp::BitNot => self.emit("  notq %rax"),
                    UnaryOp::LogNot => {
                        // !x: set %rax to 1 if %rax == 0, else 0
                        self.emit("  cmpq $0, %rax");
                        self.emit("  mov rax, $0");
                        self.emit("  sete %al");
                    }
                }
            }

            Expr::BinaryOp(op, left, right) => {
                // Evaluate left, push to stack
                // Evaluate right, result in %rax
                // Pop left into %rcx, perform op
                self.gen_expr(left)?;
                self.emit("  push rax");
                self.gen_expr(right)?;
                self.emit("  mov rcx, rax"); // right operand in rcx
                self.emit("  pop rax");        // left operand back in rax

                match op {
                    BinaryOp::Add => self.emit("  addq %rcx, %rax"),
                    BinaryOp::Sub => self.emit("  subq %rcx, %rax"),
                    BinaryOp::Mul => self.emit("  imulq %rcx, %rax"),
                    BinaryOp::Div => {
                        // idivq divides rdx:rax by the operand
                        // cqto sign-extends rax into rdx first
                        self.emit("  cqto");
                        self.emit("  idivq %rcx");
                        // quotient is left in %rax automatically
                    }
                }
            }

            Expr::Var(name) => {
                return Err(format!("Variables not yet supported: {}", name));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(source: &str) -> String {
        let mut lexer = crate::lexer::Lexer::new(source);
        let tokens = lexer.tokenize();
        let mut parser = crate::parser::Parser::new(tokens);
        let program = parser.parse_program().unwrap();
        let mut codegen = Codegen::new();
        codegen.generate(&program).unwrap()
    }

    #[test]
    fn test_return_literal() {
        let asm = compile("int main() { return 42; }");
        assert!(asm.contains("movq $42, %rax"));
        assert!(asm.contains("ret"));
    }

    #[test]
    fn test_negate() {
        let asm = compile("int main() { return -42; }");
        assert!(asm.contains("movq $42, %rax"));
        assert!(asm.contains("negq %rax"));
    }

    #[test]
    fn test_addition() {
        let asm = compile("int main() { return 1 + 2; }");
        assert!(asm.contains("addq %rcx, %rax"));
    }

    #[test]
    fn test_division() {
        let asm = compile("int main() { return 10 / 2; }");
        assert!(asm.contains("idivq %rcx"));
    }
}
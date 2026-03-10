use crate::ast::*;

pub struct Codegen {
    output: String,
    data_section: String,
    string_count: usize,
}

impl Codegen {
    pub fn new() -> Self {
        Codegen { 
            output: String::new(), 
            data_section: String::new(),
            string_count: 0,
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

    pub fn generate(&mut self, program: &Program) -> Result<String, String> {
        // Reserve space for data section (filled in as we go)
        for function in &program.functions {
            self.gen_function(function)?;
        }

        // Assemble final output: data section first, then text
        let mut final_output = String::new();
        
        if !self.data_section.is_empty() {
            final_output.push_str("section .data\n");
            final_output.push_str(&self.data_section);
            final_output.push('\n');
        }

        final_output.push_str("section .text\n");
        final_output.push_str(&self.output);

        Ok(final_output)
    }

    fn gen_function(&mut self, func: &Function) -> Result<(), String> {

        // make sure gcc can assemble our output by using Intel syntax and no prefixes
        self.emit("  .intel_syntax noprefix");

        // Emit the function label, visible to the linker
        self.emit(&format!("  .globl {}", func.name));
        self.emit(&format!("{}:", func.name));

        // Function prologue — set up the stack frame
        self.emit("  push rbp");
        self.emit("  mov rbp, rsp");

        self.emit("  and rsp, -16"); // align stack to 16 bytes for calls

        for stmt in &func.body {
            self.gen_statement(stmt)?;
        }

        Ok(())
    }

    fn gen_statement(&mut self, stmt: &Stmt) -> Result<(), String> {
        match stmt {
            Stmt::Return(expr) => {
                self.gen_expr(expr)?;
                self.emit(" mov rsp, rbp");
                self.emit("  pop rbp");
                self.emit("  ret");
            }
            Stmt::Expr(expr) => {
                self.gen_expr(expr)?;
            }
        }
        Ok(())
    }

    fn gen_expr(&mut self, expr: &Expr) -> Result<(), String> {
        match expr {
            Expr::IntLiteral(n) => {
                // Move the literal directly into %rax
                self.emit(&format!("  mov rax, {}", n));
            }

            Expr::StringLiteral(s) => {
                let label = self.add_string(s);
                self.emit(&format!("  lea rax, [{}]", label));
            }

            Expr::FunctionCall {name, args} => {
                let arg_regs = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];
                if args.len() > arg_regs.len() {
                    return Err(format!("[ ERROR ] :: Function calls with more than {} arguments not supported", arg_regs.len()));
                }

                for (i, arg) in args.iter().enumerate() {
                    self.gen_expr(arg)?;
                    self.emit(&format!("  mov {}, rax", arg_regs[i]));
                }

                for i in (0..args.len()).rev() {
                    self.emit(&format!("  pop {}", arg_regs[i]));
                }

                self.emit(" mov rax, 0"); 
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
}
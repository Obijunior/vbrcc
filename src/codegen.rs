//! Stage 4: the code generator.
//!
//! [`Codegen::generate`] walks the typed AST and returns Intel-syntax x86-64 assembly.
//! It reads the `ty` field of each expression to scale pointer arithmetic and to decay
//! arrays, so it must run after [`crate::typeck`].
//!
//! # Rules
//!
//! - Every expression puts its result in `rax`.
//! - `rsp` does not move after the prologue. Intermediate values go into frame slots
//!   through `spill_rax`, never through `push`. See that method for the reason.
//! - Call arguments go into frame slots first. The generator loads `rcx`, `rdx`, `r8`,
//!   and `r9` immediately before the `call`, because any expression it evaluates in
//!   between overwrites `rcx`.
//! - Locals live at negative offsets from `rbp`. The `variables` map holds each offset.
//!   Slots are never reused, so a function reserves 8 bytes for each spill site.
//! - Every label carries a number, such as `loop_0_start`. Nested control flow cannot
//!   collide.
//!
//! # Values and addresses
//!
//! `gen_expr` puts the value of an expression in `rax`. `gen_lvalue_addr` puts the
//! address of an lvalue in `rax`. Use the second one for `&x`, for a store through
//! a pointer, and for an index.

use crate::ast::*;
use crate::diagnostic::{CompileError, Spanned};
use std::collections::HashMap;

/// Where a named variable lives, from [`Codegen::var_loc`].
enum VarLoc {
    /// A frame slot at this `rbp` offset (negative).
    Local(i64),
    /// A `.data` label, reached as `[rip + name]`.
    Global,
}

pub struct Codegen {
    output: String,
    data_section: String,
    string_count: usize,
    variables: HashMap<String, i64>, // name -> rbp offset
    globals: HashMap<String, Type>,  // name -> type, addressed as [rip + name]
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
            globals: HashMap::new(),
            stack_offset: 0,
            label_count: 0,
        }
    }

    fn emit(&mut self, line: &str) {
        self.output.push_str(line);
        self.output.push('\n');
    }

    /// Load `width` bytes from `addr` into rax, sign-extending narrow values.
    fn emit_load(&mut self, addr: &str, width: usize) {
        match width {
            1 => self.emit(&format!("  movsx rax, byte ptr {}", addr)),
            4 => self.emit(&format!("  movsxd rax, dword ptr {}", addr)),
            _ => self.emit(&format!("  mov rax, {}", addr)),
        }
    }

    /// Store the low `width` bytes of `src` (a 64-bit reg name) to `addr`.
    fn emit_store(&mut self, addr: &str, src: &str, width: usize) {
        match width {
            1 => self.emit(&format!("  mov byte ptr {}, {}", addr, src)),
            4 => self.emit(&format!("  mov dword ptr {}, {}", addr, src)),
            _ => self.emit(&format!("  mov {}, {}", addr, src)),
        }
    }

    /// Save `rax` into a new frame slot. Returns the offset of the slot from `rbp`.
    fn spill_rax(&mut self) -> i64 {
        self.stack_offset -= 8;
        let slot = self.stack_offset;
        self.emit(&format!("  mov [rbp - {}], rax", -slot));
        slot
    }

    /// Load a previously spilled value back into `reg`.
    fn reload(&mut self, reg: &str, slot: i64) {
        self.emit(&format!("  mov {}, [rbp - {}]", reg, -slot));
    }

    /// Collapse rax to 0 or 1
    fn normalize_bool(&mut self) {
        self.emit("  cmp rax, 0");
        self.emit("  setne al");
        self.emit("  movzx rax, al");
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

    /// Where a name lives. A local shadows a global of the same name, so
    /// `variables` is checked first.
    ///
    /// The assembler accepts a RIP-relative label only as a `lea` operand,
    /// not inside `mov`/`movsx`. So a global read or write goes through its
    /// address: `lea rax, [rip + name]` then load or store `[rax]`.
    fn var_loc(&self, name: &str) -> Option<VarLoc> {
        if let Some(&off) = self.variables.get(name) {
            Some(VarLoc::Local(off))
        } else if self.globals.contains_key(name) {
            Some(VarLoc::Global)
        } else {
            None
        }
    }

    /// Emit one global into the data section: a label plus a size-typed
    /// directive. `None` init is zero-filled.
    fn gen_global(&mut self, g: &GlobalVar) -> Result<(), CompileError> {
        self.emit_data("  .section .data");
        self.emit_data(&format!("{}:", g.name));

        let init = match &g.init {
            None => {
                self.emit_data(&format!("    .zero {}", g.ty.size()));
                return Ok(());
            }
            Some(e) => e,
        };

        match crate::constfold::eval_const(init)? {
            crate::constfold::ConstValue::Int(n) => {
                if g.ty == Type::Bool {
                    self.emit_data(&format!("    .byte {}", (n != 0) as i64));
                } else {
                    match g.ty.size() {
                        1 => self.emit_data(&format!("    .byte {n}")),
                        4 => self.emit_data(&format!("    .long {n}")),
                        _ => self.emit_data(&format!("    .quad {n}")),
                    }
                }
            }
            crate::constfold::ConstValue::Bytes(bytes) => {
                let escaped: String = bytes
                    .iter()
                    .map(|&c| match c {
                        0 => "\\0".to_string(),
                        b'"' => "\\\"".to_string(),
                        b'\\' => "\\\\".to_string(),
                        b'\n' => "\\n".to_string(),
                        b'\t' => "\\t".to_string(),
                        0x20..=0x7E => (c as char).to_string(),
                        _ => "\\0".to_string(),
                    })
                    .collect();
                self.emit_data(&format!("    .ascii \"{escaped}\""));
                let declared = g.ty.size();
                if declared > bytes.len() {
                    self.emit_data(&format!("    .zero {}", declared - bytes.len()));
                }
            }
        }
        Ok(())
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
        for g in &program.globals {
            self.globals.insert(g.name.clone(), g.ty.clone());
        }
        for g in &program.globals {
            self.gen_global(g)?;
        }

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
        for (i, (_ty, param)) in func.params.iter().enumerate() {
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

                self.gen_statement(init)?;
                self.emit(&format!("loop_{}_start:", id));

                self.gen_expr(cond)?;
                self.emit("  cmp rax, 0");
                self.emit(&format!("  je loop_{}_end", id));

                for stmt in body {
                    self.gen_statement(stmt)?;
                }
                self.gen_statement(update)?;
                self.emit(&format!("  jmp loop_{}_start", id));
                self.emit(&format!("loop_{}_end:", id));
            }
            Stmt::Expr(expr) => {
                self.gen_expr(expr)?;
            }
            Stmt::VarDecl { ty, name, init, .. } => {
                self.stack_offset -= ty.size() as i64;
                self.stack_offset = -Codegen::align_up(-self.stack_offset, ty.align() as i64);
                let offset: i64 = self.stack_offset;
                self.variables.insert(name.clone(), offset);
                if let Some(expr) = init {
                    if let Type::Struct { size, .. } = ty {
                        let size = *size;
                        self.gen_lvalue_addr(expr)?;
                        self.emit("  mov r11, rax");
                        self.emit(&format!("  lea r10, [rbp - {}]", -offset));
                        self.emit_struct_copy(size);
                    } else {
                        self.gen_expr(expr)?;
                        if *ty == Type::Bool {
                            self.normalize_bool();
                        }
                        let addr = format!("[rbp - {}]", -offset);
                        self.emit_store(&addr, "rax", ty.size());
                    }
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

    /// Put the address of an lvalue in rax. Compare `gen_expr`, which puts its value
    /// there.
    fn gen_lvalue_addr(&mut self, expr: &TypedExpr) -> Result<(), CompileError> {
        match &expr.node {
            Expr::Var(name) => {
                match self.var_loc(name).ok_or_else(|| {
                    CompileError::new(format!("undefined variable `{name}`"), expr.span)
                        .with_label("not found in this scope")
                })? {
                    VarLoc::Local(off) => self.emit(&format!("  lea rax, [rbp - {}]", -off)),
                    VarLoc::Global => self.emit(&format!("  lea rax, [rip + {name}]")),
                }
            }
            Expr::Deref(inner) => {
                // The address of `*p` is the value of `p`.
                self.gen_expr(inner)?;
            }
            Expr::Index(base, idx) => {
                let elem_size = base.ty.pointee().map(|t| t.size()).unwrap_or(8);
                self.gen_ptr_base(base)?;
                let base_slot = self.spill_rax();
                self.gen_expr(idx)?;
                self.emit(&format!("  imul rax, {}", elem_size));
                self.emit("  mov rcx, rax");
                self.reload("rax", base_slot);
                self.emit("  add rax, rcx");
            }
            Expr::Member(base, field) => {
                let offset = match &base.ty {
                    Type::Struct { fields, .. } => fields
                        .iter()
                        .find(|f| &f.name == field)
                        .map(|f| f.offset)
                        .expect("typeck verified the field exists"),
                    _ => unreachable!("typeck verified base is a struct"),
                };
                self.gen_lvalue_addr(base)?;
                if offset != 0 {
                    self.emit(&format!("  add rax, {offset}"));
                }
            }
            _ => {
                return Err(CompileError::new("expression is not an lvalue", expr.span)
                    .with_label("cannot take its address"));
            }
        }
        Ok(())
    }

    /// Copy `size` bytes from `[r11]` to `[r10]`, using `rax` as scratch.
    /// Unrolled at compile time; `r10` / `r11` / `rax` are caller-clobbered.
    fn emit_struct_copy(&mut self, size: usize) {
        let mut done = 0usize;
        while done < size {
            let chunk = size - done;
            let w = if chunk >= 8 { 8 } else if chunk >= 4 { 4 } else if chunk >= 2 { 2 } else { 1 };
            let (mov, reg) = match w {
                8 => ("mov", "rax"),
                4 => ("mov", "eax"),
                2 => ("mov", "ax"),
                _ => ("mov", "al"),
            };
            let ptr = match w { 8 => "qword ptr", 4 => "dword ptr", 2 => "word ptr", _ => "byte ptr" };
            self.emit(&format!("  {mov} {reg}, {ptr} [r11 + {done}]"));
            self.emit(&format!("  {mov} {ptr} [r10 + {done}], {reg}"));
            done += w;
        }
    }


    /// Yield the base pointer of an indexing target in rax: an array decays to its
    /// address; a pointer yields its value.
    fn gen_ptr_base(&mut self, base: &TypedExpr) -> Result<(), CompileError> {
        if matches!(base.ty, Type::Array(_, _)) {
            self.gen_lvalue_addr(base)
        } else {
            self.gen_expr(base)
        }
    }    

    fn gen_expr(&mut self, expr: &TypedExpr) -> Result<(), CompileError> {
        match &expr.node {
            Expr::IntLiteral(n) => {
                self.emit(&format!("  mov rax, {}", n));
            }

            Expr::StringLiteral(s) => {
                let label = self.add_string(s);
                self.emit(&format!("  lea rax, [rip + {}]", label));
            }

            Expr::FunctionCall {name, args} => {
                // Win64 passes the first four integer arguments in these registers.
                let arg_regs = ["rcx", "rdx", "r8", "r9"];
                if args.len() > arg_regs.len() {
                    return Err(CompileError::new(
                        format!("function calls with more than {} arguments are not supported", arg_regs.len()),
                        expr.span,
                    ));
                }

                // Evaluate every argument into a frame slot before any register load.
                let mut slots = Vec::with_capacity(args.len());
                for arg in args.iter() {
                    self.gen_expr(arg)?;
                    slots.push(self.spill_rax());
                }

                // Nothing runs between here and the call, so the registers are safe.
                for (i, slot) in slots.iter().enumerate() {
                    self.reload(arg_regs[i], *slot);
                    // A variadic callee reads its named arguments from shadow space.
                    self.emit(&format!("  mov [rsp + {}], {}", i * 8, arg_regs[i]));
                }

                self.emit(&format!("  call {}", name));
            }

            Expr::UnaryOp(op, inner) => {
                self.gen_expr(inner)?;
                match op {
                    UnaryOp::Negate => self.emit("  neg rax"),
                    UnaryOp::BitNot => self.emit("  not rax"),
                    UnaryOp::LogNot => {
                        self.emit("  cmp rax, 0");
                        self.emit("  mov rax, 0");
                        self.emit("  sete al");
                    }
                }
            }

            Expr::BinaryOp(op, left, right) => {
                // `&&` and `||` must not evaluate the right operand when the left
                // settles the result, so they branch instead of falling through.
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
                // The operation reads the left operand from rax and the right one
                // from rcx.
                self.gen_expr(left)?;
                let left_slot = self.spill_rax();
                self.gen_expr(right)?;
                self.emit("  mov rcx, rax");
                self.reload("rax", left_slot);

                match op {
                    BinaryOp::Add => {
                        if let Some(t) = left.ty.pointee() {
                            self.emit(&format!("  imul rcx, {}", t.size()));
                        } 
                        self.emit("  add rax, rcx")
                    }
                    BinaryOp::Sub => {
                        if let Some(t) = left.ty.pointee() {
                            self.emit(&format!("  imul rcx, {}", t.size()));
                        }
                        self.emit("  sub rax, rcx")
                    }
                    BinaryOp::Mul => self.emit("  imul rax, rcx"),
                    // `idiv` divides rdx:rax, so `cqo` must sign-extend rax into rdx
                    // first. The quotient lands in rax and the remainder in rdx.
                    BinaryOp::Div => {
                        self.emit("  cqo");
                        self.emit("  idiv rcx");
                    }
                    BinaryOp::Mod => {
                        self.emit("  cqo");
                        self.emit("  idiv rcx");
                        self.emit("  mov rax, rdx");
                    }
                    // Each comparison sets the low byte of rax, then widens it.
                    BinaryOp::Eq => {
                        self.emit("  cmp rax, rcx");
                        self.emit("  sete al");
                        self.emit("  movzx rax, al");
                    }
                    BinaryOp::Neq => {
                        self.emit("  cmp rax, rcx");
                        self.emit("  setne al");
                        self.emit("  movzx rax, al");
                    }
                    BinaryOp::Lt => {
                        self.emit("  cmp rax, rcx");
                        self.emit("  setl al");
                        self.emit("  movzx rax, al");
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
            Expr::Var(name) => {
                let loc = self.var_loc(name).ok_or_else(|| {
                    CompileError::new(format!("undefined variable `{name}`"), expr.span)
                        .with_label("not found in this scope")
                })?;
                let is_array = matches!(expr.ty, Type::Array(_, _));
                match loc {
                    VarLoc::Local(off) => {
                        let addr = format!("[rbp - {}]", -off);
                        if is_array {
                            // Array decays to a pointer to its first element.
                            self.emit(&format!("  lea rax, {addr}"));
                        } else {
                            self.emit_load(&addr, expr.ty.size());
                        }
                    }
                    VarLoc::Global => {
                        // The address first; the assembler has no
                        // `mov reg, [rip + label]`.
                        self.emit(&format!("  lea rax, [rip + {name}]"));
                        if !is_array {
                            self.emit_load("[rax]", expr.ty.size());
                        }
                    }
                }
            }

            Expr::AddressOf(inner) => {
                self.gen_lvalue_addr(inner)?;
            }

            Expr::Deref(inner) => {
                self.gen_expr(inner)?;         // rax = pointer
                self.emit_load("[rax]", expr.ty.size());
            }

            Expr::Index(_base, _idx) => {
                self.gen_lvalue_addr(expr)?;   // rax = element address
                self.emit_load("[rax]", expr.ty.size());
            }

            Expr::Cast(_ty, inner) => {
                // A cast does not change the representation yet. Evaluate the operand.
                self.gen_expr(inner)?;
            }

            Expr::Member(_base, _field) => {
                self.gen_lvalue_addr(expr)?;        // rax = field address
                let aggregate = matches!(expr.ty, Type::Struct { .. } | Type::Array(_, _));
                if !aggregate {
                    self.emit_load("[rax]", expr.ty.size());
                }
            }

            Expr::Assign(lval, value) => {
                if let Type::Struct { size, ..} = &lval.ty {
                    let size = *size;
                    self.gen_lvalue_addr(value)?; // rax = src addr
                    let src_slot = self.spill_rax();
                    self.gen_lvalue_addr(lval)?; // rax = dst addr
                    self.reload("r11", src_slot);
                    self.emit_struct_copy(size);
                    self.emit("  mov rax, r10")

                } else {
                    self.gen_expr(value)?;
                    if lval.ty == Type::Bool {
                        self.normalize_bool();
                    }
                    let value_slot = self.spill_rax();
                    self.gen_lvalue_addr(lval)?;
                    self.reload("rcx", value_slot);
                    self.emit_store("[rax]", "rcx", lval.ty.size());
                    // An assignment evaluates to the value it stored.
                    self.emit("  mov rax, rcx");
                }
            }

            Expr::PostIncDec(op, target) => {
                // `x++` evaluates to the value before the update, so the old value
                // goes into a slot and comes back last. The address needs a slot too,
                // because the load overwrites rax and the store needs the address.
                let width = target.ty.size();
                // `p++` steps by one element, the same as `p + 1`.
                let step = target.ty.pointee().map(|t| t.size()).unwrap_or(1);

                self.gen_lvalue_addr(target)?;
                let addr_slot = self.spill_rax();
                self.emit_load("[rax]", width);
                let old_slot = self.spill_rax();

                match op {
                    IncDec::Inc => self.emit(&format!("  add rax, {}", step)),
                    IncDec::Dec => self.emit(&format!("  sub rax, {}", step)),
                }
                self.emit("  mov rcx, rax");
                self.reload("rax", addr_slot);
                self.emit_store("[rax]", "rcx", width);
                self.reload("rax", old_slot);
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
        let mut program = parser.parse_program().unwrap();
        // Codegen relies on typeck-annotated `expr.ty` (pointer scaling, array decay).
        crate::typeck::check(&mut program).unwrap();
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
        assert!(asm.contains("mov dword ptr [rbp - 4], rax"));
        assert!(asm.contains("movsxd rax, dword ptr [rbp - 4]"));
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

    #[test]
    fn test_address_of_emits_lea() {
        let asm = compile("int main() { int x = 5; int *p = &x; return 0; }");
        assert!(asm.contains("lea rax, [rbp -"), "asm:\n{asm}");
    }

    #[test]
    fn test_deref_load() {
        let asm = compile("int main() { int x = 5; int *p = &x; return *p; }");
        assert!(asm.contains("movsxd rax, dword ptr [rax]"), "asm:\n{asm}");
    }

    #[test]
    fn test_pointer_arithmetic_scales_by_four() {
        let asm = compile("int main() { int a[3]; int *p = a; return *(p + 1); }");
        assert!(asm.contains("imul rcx, 4"), "asm:\n{asm}");
    }

    #[test]
    fn operands_spill_to_frame_slots_rather_than_pushing() {
        // `push rax` puts the operand where a nested call writes its shadow space, and
        // it leaves rsp misaligned at the call. `f(1) + g(2)` returned 4, not 3.
        let asm = compile(
            "int f(int x) { return x; } \
             int g(int x) { return x; } \
             int main() { return f(1) + g(2); }",
        );
        assert!(!asm.contains("push rax"), "operands must spill to frame slots:\n{asm}");
    }

    #[test]
    fn assignment_and_indexing_also_avoid_push() {
        let asm = compile("int main() { int a[3]; int i = 1; a[i] = 7; return a[i]; }");
        assert!(!asm.contains("push rax"), "asm:\n{asm}");
    }

    #[test]
    fn post_increment_steps_a_pointer_by_the_pointee_size() {
        // `p++` must scale like `p + 1`, not add a raw 1.
        let asm = compile("int main() { int a[3]; int *p = a; p++; return 0; }");
        assert!(asm.contains("add rax, 4"), "asm:\n{asm}");
    }

    #[test]
    fn post_increment_steps_a_scalar_by_one() {
        let asm = compile("int main() { int i = 0; i++; return i; }");
        assert!(asm.contains("add rax, 1"), "asm:\n{asm}");
    }

    #[test]
    fn test_store_through_pointer() {
        let asm = compile("int main() { int x = 0; int *p = &x; *p = 7; return x; }");
        assert!(asm.contains("mov dword ptr [rax], rcx"), "asm:\n{asm}");
    }

    #[test]
    fn a_scalar_global_is_emitted_into_data() {
        let asm = compile("int counter = 42; int main() { return counter; }");
        assert!(asm.contains(".section .data"), "asm:\n{asm}");
        assert!(asm.contains("counter:"), "asm:\n{asm}");
        assert!(asm.contains(".long 42"), "asm:\n{asm}");
    }

    #[test]
    fn a_global_is_read_through_rip() {
        let asm = compile("int counter = 42; int main() { return counter; }");
        assert!(asm.contains("[rip + counter]"), "asm:\n{asm}");
        assert!(!asm.contains("[rbp"), "counter must not be a frame slot:\n{asm}");
    }

    #[test]
    fn a_global_is_written_through_rip() {
        let asm = compile("int g; int main() { g = 7; return g; }");
        assert!(asm.contains("[rip + g]"), "asm:\n{asm}");
    }

    #[test]
    fn an_uninitialized_global_is_zero_filled() {
        let asm = compile("int g; int main() { return g; }");
        assert!(asm.contains("g:"), "asm:\n{asm}");
        assert!(asm.contains(".zero 4"), "asm:\n{asm}");
    }

    #[test]
    fn a_char_array_global_emits_ascii_bytes() {
        let asm = compile("char s[] = \"hi\"; int main() { return s[0]; }");
        assert!(asm.contains("s:"), "asm:\n{asm}");
        assert!(asm.contains(r#".ascii "hi\0""#), "asm:\n{asm}");
    }

    #[test]
    fn a_nonzero_bool_global_is_stored_as_one() {
        let asm = compile("_Bool b = 5; int main() { return b; }");
        assert!(asm.contains("b:"), "asm:\n{asm}");
        assert!(asm.contains(".byte 1"), "asm:\n{asm}");
    }

    #[test]
    fn a_local_still_uses_a_frame_slot() {
        let asm = compile("int main() { int x = 3; return x; }");
        assert!(asm.contains("[rbp -"), "asm:\n{asm}");
    }
}

use crate::register::Register64;

pub enum AsmLine {
    Instruction(Instruction),
    SectionChange(Section),
    DataBytes(Vec<u8>),
    Label(String),
    None,
}

#[derive(Debug)]
pub enum Instruction {
    Ret,
    Cqo,
    Syscall,
    MovRegImm64 { dst: Register64, imm: i64 },
    MovRegReg { dst: Register64, src: Register64 },
    MovMemDispReg { base: Register64, disp: i32, src: Register64 },
    MovRegMemDisp { dst: Register64, base: Register64, disp: i32 },
    AddRegReg { dst: Register64, src: Register64 },
    AddRegImm32 { dst: Register64, imm: i32 },
    SubRegReg { dst: Register64, src: Register64 },
    SubRegImm32 { dst: Register64, imm: i32 },
    ImulRegReg { dst: Register64, src: Register64 },
    ImulRegImm32 { dst: Register64, imm: i32 },
    AndRegReg { dst: Register64, src: Register64 },
    AndRegImm32 { dst: Register64, imm: i32 },
    CmpRegReg { dst: Register64, src: Register64 },
    CmpRegImm32 { dst: Register64, imm: i32 },
    PushReg { reg: Register64 },
    PopReg { reg: Register64 },
    NegReg { reg: Register64 },
    NotReg { reg: Register64 },
    LeaRegLabel { dst: Register64, label: String },
    CallLabel { label: String },
}

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum Section {
    Text,
    Data,
}

enum RegOrImm {
    Reg(Register64),
    Imm(i32),
}

fn parse_register64(s: &str) -> Option<Register64> {
    match s.trim().to_ascii_lowercase().as_str() {
        "rax" => Some(Register64::Rax), "rbx" => Some(Register64::Rbx),
        "rcx" => Some(Register64::Rcx), "rdx" => Some(Register64::Rdx),
        "rsi" => Some(Register64::Rsi), "rdi" => Some(Register64::Rdi),
        "rbp" => Some(Register64::Rbp), "rsp" => Some(Register64::Rsp),
        "r8" => Some(Register64::R8), "r9" => Some(Register64::R9),
        "r10" => Some(Register64::R10), "r11" => Some(Register64::R11),
        "r12" => Some(Register64::R12), "r13" => Some(Register64::R13),
        "r14" => Some(Register64::R14), "r15" => Some(Register64::R15),
        _ => None,
    }
}

fn split_instruction(line: &str) -> (&str, Vec<&str>) {
    let mut parts = line.trim().splitn(2, char::is_whitespace);
    let opcode = parts.next().unwrap_or("").trim();
    let rest = parts.next().unwrap_or("").trim();
    let operands = if rest.is_empty() {
        vec![]
    } else {
        rest.split(',').map(|s| s.trim()).collect()
    };
    (opcode, operands)    
}

fn parse_mem_operand(op: &str) -> Option<(Register64, i32)> {
    let s = op.trim();
    if !(s.starts_with('[') && s.ends_with(']')) {
        return None;
    }
    let inner = s[1..s.len() - 1].trim();
    if let Some((base_str, disp_str)) = inner.split_once('+') {
        let base = parse_register64(base_str.trim())?;
        let disp = disp_str.trim().parse::<i32>().ok()?;
        Some((base, disp))
    } else if let Some((base_str, disp_str)) = inner.split_once('-') {
        let base = parse_register64(base_str.trim())?;
        let disp = disp_str.trim().parse::<i32>().ok()?;
        Some((base, -disp))
    } else {
        let base = parse_register64(inner)?;
        Some((base, 0))
    }
}

fn parse_reg_regimm(operands: &[&str], raw: &str) -> Result<(Register64, RegOrImm), String> {
    if operands.len() != 2 {
        return Err(format!("[ ERROR ] :: expected 2 operands: {}", raw));
    }
    let dst = parse_register64(operands[0])
        .ok_or_else(|| format!("[ ERROR ] :: invalid dst register: {}", raw))?;
    if let Ok(imm64) = operands[1].parse::<i64>() {
        let imm = i32::try_from(imm64)
            .map_err(|_| format!("[ ERROR ] :: immediate out of 32-bit range: {}", raw))?;
        Ok((dst, RegOrImm::Imm(imm)))
    } else {
        let src = parse_register64(operands[1])
            .ok_or_else(|| format!("[ ERROR ] :: invalid src operand: {}", raw))?;
        Ok((dst, RegOrImm::Reg(src)))
    }
}

pub fn parse_intel_line(raw: &str) -> Result<AsmLine, String> {
    let mut line = raw.trim();
    if line.is_empty() || line.starts_with(';') { return Ok(AsmLine::None); }

    // Handle Section Directives: "section .text" or ".section .text"
    if line.starts_with("section") || line.starts_with(".section") {
        let parts: Vec<&str> = line.split_whitespace().collect();
        return match parts.get(1) {
            Some(&".text") => Ok(AsmLine::SectionChange(Section::Text)),
            Some(&".data") => Ok(AsmLine::SectionChange(Section::Data)),
            _ => Err(format!("Unknown section: {}", line)),
        };
    }

    // Handle Raw Data
    if line.starts_with("db") {
        let content = line.strip_prefix("db").unwrap().trim();
        let bytes = content.replace("\"", "").as_bytes().to_vec();
        return Ok(AsmLine::DataBytes(bytes));
    }

    // Handle .ascii "..." with basic escapes
    if line.starts_with(".ascii") {
        let content = line.strip_prefix(".ascii").unwrap().trim();
        if !(content.starts_with('"') && content.ends_with('"')) {
            return Err(format!("[ ERROR ] :: .ascii expects quoted string: {}", raw));
        }
        let inner = &content[1..content.len() - 1];
        let mut bytes = Vec::new();
        let mut chars = inner.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('n') => bytes.push(b'\n'),
                    Some('t') => bytes.push(b'\t'),
                    Some('"') => bytes.push(b'"'),
                    Some('\\') => bytes.push(b'\\'),
                    Some('0') => bytes.push(0),
                    Some(other) => {
                        return Err(format!("[ ERROR ] :: unknown escape \\{} in .ascii: {}", other, raw));
                    }
                    None => return Err(format!("[ ERROR ] :: unterminated escape in .ascii: {}", raw)),
                }
            } else {
                if c as u32 > 0x7F {
                    return Err(format!("[ ERROR ] :: non-ASCII in .ascii not supported: {}", raw));
                }
                bytes.push(c as u8);
            }
        }
        return Ok(AsmLine::DataBytes(bytes));
    }

    if let Some((head, _)) = line.split_once(';') {
        line = head.trim();
        if line.is_empty() {
            return Ok(AsmLine::None);
        }
    }

    // Labels
    if line.ends_with(':') {
        let name = line.trim_end_matches(':').trim().to_string();
        if name.is_empty() {
            return Err(format!("[ ERROR ] :: empty label: {}", raw));
        }
        return Ok(AsmLine::Label(name));
    }

    // Allow passthrough for directives for now.
    if line.starts_with(".intel_syntax") || line.starts_with(".globl") {
        return Ok(AsmLine::None);
    }
    if line.starts_with('.') { return Ok(AsmLine::None); }

    let (opcode, operands) = split_instruction(line);

    match opcode.to_ascii_lowercase().as_str() {
        "ret" => {
            if !operands.is_empty() {
                return Err(format!("[ ERROR ] :: ret should have no operands: {}", raw));
            }
            Ok(AsmLine::Instruction(Instruction::Ret))
        }
        "cqo" => {
            if !operands.is_empty() {
                return Err(format!("[ ERROR ] :: cqo should have no operands: {}", raw));
            }
            Ok(AsmLine::Instruction(Instruction::Cqo))
        }
        "syscall" => {
            Ok(AsmLine::Instruction(Instruction::Syscall))
        }
        "push" => {
            let reg = parse_register64(operands[0])
                .ok_or_else(|| format!("[ ERROR ] :: invalid register in push: {}", raw))?;
            Ok(AsmLine::Instruction(Instruction::PushReg { reg }))
        }
        "pop" => {
            let reg = parse_register64(operands[0])
                .ok_or_else(|| format!("[ ERROR ] :: invalid register in pop: {}", raw))?;
            Ok(AsmLine::Instruction(Instruction::PopReg { reg }))
        }
        "neg" => {
            let reg = parse_register64(operands[0])
                .ok_or_else(|| format!("[ ERROR ] :: invalid register in neg: {}", raw))?;
            Ok(AsmLine::Instruction(Instruction::NegReg { reg }))
        }
        "not" => {
            let reg = parse_register64(operands[0])
                .ok_or_else(|| format!("[ ERROR ] :: invalid register in not: {}", raw))?;
            Ok(AsmLine::Instruction(Instruction::NotReg { reg }))
        }
        "mov" => {
            if operands.len() != 2 {
                return Err(format!("[ ERROR ] :: mov expects 2 operands: {}", raw));
            }

            // mov [base +/- disp], reg
            if let Some((base, disp)) = parse_mem_operand(operands[0]) {
                let src = parse_register64(operands[1])
                    .ok_or_else(|| format!("[ ERROR ] :: invalid src register: {}", raw))?;
                return Ok(AsmLine::Instruction(Instruction::MovMemDispReg { base, disp, src }));
            }

            // mov reg, [base +/- disp]
            if let Some((base, disp)) = parse_mem_operand(operands[1]) {
                let dst = parse_register64(operands[0])
                    .ok_or_else(|| format!("[ ERROR ] :: invalid dst register: {}", raw))?;
                return Ok(AsmLine::Instruction(Instruction::MovRegMemDisp { dst, base, disp }));
            }

            let dst = parse_register64(operands[0])
                .ok_or_else(|| format!("[ ERROR ] :: invalid dst register: {}", raw))?;

            if let Ok(imm) = operands[1].parse::<i64>() {
                Ok(AsmLine::Instruction(Instruction::MovRegImm64 { dst, imm }))
            } else {
                let src = parse_register64(operands[1])
                    .ok_or_else(|| format!("[ ERROR ] :: invalid src register: {}", raw))?;
                Ok(AsmLine::Instruction(Instruction::MovRegReg { dst, src }))
            }
        }
        "add" | "sub" | "and" | "cmp" | "imul" => {
            let (dst, src) = parse_reg_regimm(&operands, raw)?;
            let instr = match (opcode.to_ascii_lowercase().as_str(), src) {
                ("add", RegOrImm::Reg(src)) => Instruction::AddRegReg { dst, src },
                ("add", RegOrImm::Imm(imm)) => Instruction::AddRegImm32 { dst, imm },
                ("sub", RegOrImm::Reg(src)) => Instruction::SubRegReg { dst, src },
                ("sub", RegOrImm::Imm(imm)) => Instruction::SubRegImm32 { dst, imm },
                ("and", RegOrImm::Reg(src)) => Instruction::AndRegReg { dst, src },
                ("and", RegOrImm::Imm(imm)) => Instruction::AndRegImm32 { dst, imm },
                ("cmp", RegOrImm::Reg(src)) => Instruction::CmpRegReg { dst, src },
                ("cmp", RegOrImm::Imm(imm)) => Instruction::CmpRegImm32 { dst, imm },
                ("imul", RegOrImm::Reg(src)) => Instruction::ImulRegReg { dst, src },
                ("imul", RegOrImm::Imm(imm)) => Instruction::ImulRegImm32 { dst, imm },
                _ => unreachable!(),
            };
            Ok(AsmLine::Instruction(instr))
        }
        "lea" => {
            if operands.len() != 2 {
                return Err(format!("[ ERROR ] :: lea expects 2 operands: {}", raw));
            }
            let dst = parse_register64(operands[0])
                .ok_or_else(|| format!("[ ERROR ] :: invalid dst register: {}", raw))?;
            let src = operands[1].trim();
            if !(src.starts_with('[') && src.ends_with(']')) {
                return Err(format!("[ ERROR ] :: lea expects [label] operand: {}", raw));
            }
            let inner = src[1..src.len() - 1].trim();
            let label = if inner.starts_with("rip +") {
                inner["rip +".len()..].trim().to_string()
            } else if inner.starts_with("rip+") {
                inner["rip+".len()..].trim().to_string()
            } else {
                inner.to_string()
            };
            if label.is_empty() {
                return Err(format!("[ ERROR ] :: lea label is empty: {}", raw));
            }
            Ok(AsmLine::Instruction(Instruction::LeaRegLabel { dst, label }))
        }
        "call" => {
            if operands.len() != 1 {
                return Err(format!("[ ERROR ] :: call expects 1 operand: {}", raw));
            }
            let label = operands[0].trim().to_string();
            if label.is_empty() {
                return Err(format!("[ ERROR ] :: call target is empty: {}", raw));
            }
            Ok(AsmLine::Instruction(Instruction::CallLabel { label }))
        }
        _ => Err(format!("[ ERROR ] :: unsupported opcode: {}", raw)),
    }
}

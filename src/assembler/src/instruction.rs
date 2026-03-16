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
    Syscall,
    MovRegImm64 { dst: Register64, imm: i64 },
    MovRegReg { dst: Register64, src: Register64 },
    MovMemDispReg { base: Register64, disp: i32, src: Register64 },
    MovRegMemDisp { dst: Register64, base: Register64, disp: i32 },
    AddRegReg { dst: Register64, src: Register64 },
    AddRegImm32 { dst: Register64, imm: i32 },
    SubRegReg { dst: Register64, src: Register64 },
    SubRegImm32 { dst: Register64, imm: i32 },
    AndRegReg { dst: Register64, src: Register64 },
    AndRegImm32 { dst: Register64, imm: i32 },
    ImulRegReg { dst: Register64, src: Register64 },
    PushReg { reg: Register64 },
    PopReg { reg: Register64 },
    LeaRegLabel { dst: Register64, label: String },
    CallLabel { label: String },
}

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum Section {
    Text,
    Data,
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
        "add" | "sub" | "and" => {
            let dst = parse_register64(operands[0])
                .ok_or_else(|| format!("[ ERROR ] :: invalid dst register: {}", raw))?;

            if let Ok(imm) = operands[1].parse::<i64>() {
                if opcode.eq_ignore_ascii_case("and") {
                    let imm32 = i32::try_from(imm)
                        .map_err(|_| format!("[ ERROR ] :: and immediate out of 32-bit range: {}", raw))?;
                    Ok(AsmLine::Instruction(Instruction::AndRegImm32 { dst, imm: imm32 }))
                } else if opcode.eq_ignore_ascii_case("add") {
                    let imm32 = i32::try_from(imm)
                        .map_err(|_| format!("[ ERROR ] :: add immediate out of 32-bit range: {}", raw))?;
                    Ok(AsmLine::Instruction(Instruction::AddRegImm32 { dst, imm: imm32 }))
                } else if opcode.eq_ignore_ascii_case("sub") {
                    let imm32 = i32::try_from(imm)
                        .map_err(|_| format!("[ ERROR ] :: sub immediate out of 32-bit range: {}", raw))?;
                    Ok(AsmLine::Instruction(Instruction::SubRegImm32 { dst, imm: imm32 }))
                } else {
                    Err(format!("[ ERROR ] :: only add/sub/and support immediates: {}", raw))
                }
            } else {
                let src = parse_register64(operands[1])
                    .ok_or_else(|| format!("[ ERROR ] :: invalid src register: {}", raw))?;

                let instr = match opcode.to_ascii_lowercase().as_str() {
                    "add" => Instruction::AddRegReg { dst, src },
                    "sub" => Instruction::SubRegReg { dst, src },
                    "and" => Instruction::AndRegReg { dst, src },
                    _ => unreachable!(),
                };
                Ok(AsmLine::Instruction(instr))
            }
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

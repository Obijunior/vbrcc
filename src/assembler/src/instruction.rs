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
    AddRegReg { dst: Register64, src: Register64 },
    SubRegReg { dst: Register64, src: Register64 },
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

pub fn parse_intel_line(raw: &str) -> Result<AsmLine, String> {
    let mut line = raw.trim();
    if line.is_empty() || line.starts_with(';') { return Ok(AsmLine::None); }

    // Handle Section Directives
    if line.starts_with("section") {
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
        "mov" | "add" | "sub" | "and" => {
            let dst = parse_register64(operands[0])
                .ok_or_else(|| format!("[ ERROR ] :: invalid dst register: {}", raw))?;
            
            if let Ok(imm) = operands[1].parse::<i64>() {
                if opcode.eq_ignore_ascii_case("mov") {
                    Ok(AsmLine::Instruction(Instruction::MovRegImm64 { dst, imm })) 
                } else if opcode.eq_ignore_ascii_case("and") {
                    let imm32 = i32::try_from(imm)
                        .map_err(|_| format!("[ ERROR ] :: and immediate out of 32-bit range: {}", raw))?;
                    Ok(AsmLine::Instruction(Instruction::AndRegImm32 { dst, imm: imm32 }))
                } else {
                    Err(format!("[ ERROR ] :: only mov supports immediates: {}", raw))
                }
            } else {
                let src = parse_register64(operands[1])
                    .ok_or_else(|| format!("[ ERROR ] :: invalid src register: {}", raw))?;
                
                let instr = match opcode.to_ascii_lowercase().as_str() {
                    "mov" => Instruction::MovRegReg { dst, src },
                    "add" => Instruction::AddRegReg { dst, src },
                    "sub" => Instruction::SubRegReg { dst, src },
                    "and" => Instruction::AndRegReg { dst, src },
                    "imul" => Instruction::ImulRegReg { dst, src },
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
            let label = src[1..src.len() - 1].trim().to_string();
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

use std::env;
use std::path::Path;
use std::process;

#[derive(Debug, Clone, Copy)]
enum Register64 {
    Rax, Rbx, Rcx, Rdx, Rsi, Rdi, Rbp, Rsp,
    R8, R9, R10, R11, R12, R13, R14, R15,
}

impl Register64 {
    fn id(self) -> u8 {
        match self {
            Self::Rax => 0, Self::Rcx => 1, Self::Rdx => 2, Self::Rbx => 3,
            Self::Rsp => 4, Self::Rbp => 5, Self::Rsi => 6, Self::Rdi => 7,
            Self::R8 => 8, Self::R9 => 9, Self::R10 => 10, Self::R11 => 11,
            Self::R12 => 12, Self::R13 => 13, Self::R14 => 14, Self::R15 => 15,
        }
    }
    fn low3(self) -> u8 { self.id() & 0b111 }
    fn ext(self) -> bool { self.id() >= 8 }
}

fn rex(w: bool, r: bool, x: bool, b: bool) -> u8 {
    0x40 | ((w as u8) << 3) | ((r as u8) << 2) | ((x as u8) << 1) | (b as u8)
}

fn modrm(mod_bits: u8, reg: u8, rm: u8) -> u8 {
    ((mod_bits & 0b11) << 6) | ((reg & 0b111) << 3) | (rm & 0b111)
}

#[derive(Debug)]
enum Instruction {
    Ret,
    MovRegImm64 { dst: Register64, imm: i64 },
    MovRegReg { dst: Register64, src: Register64 },
    AddRegReg { dst: Register64, src: Register64 },
    SubRegReg { dst: Register64, src: Register64 },
    ImulRegReg { dst: Register64, src: Register64 },
    PushReg { reg: Register64 },
    PopReg { reg: Register64 },
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

fn parse_intel_line(raw: &str) -> Result<Option<Instruction>, String> {
    let mut line = raw.trim();

    // strip comments and empty lines
    if line.is_empty() || line.starts_with(';') { return Ok(None); }

    if let Some((head, _)) = line.split_once(';') {
        line = head.trim();
        if line.is_empty() {
            return Ok(None);
        }
    }

    // Allow passthrough for labels/directives for now.
    if line.ends_with(':') || line.starts_with('.') { return Ok(None); }

    let (opcode, operands) = split_instruction(line);

    match opcode.to_ascii_lowercase().as_str() {
        "ret" => {
            if !operands.is_empty() {
                return Err(format!("[ ERROR ] :: [assembler] ret should have no operands: {}", raw));
            }
            Ok(Some(Instruction::Ret))
        }
        "push" => {
            if operands.len() != 1 {
                return Err(format!("[ ERROR ] :: [assembler] push should have exactly 1 operand: {}", raw));
            }
            let reg = parse_register64(operands[0]).ok_or_else(|| format!("[ ERROR ] :: [assembler] invalid register in push: {}", raw))?;
            Ok(Some(Instruction::PushReg { reg }))  
        }
        "pop" => {
            if operands.len() != 1 {
                return Err(format!("[ ERROR ] :: [assembler] pop should have exactly 1 operand: {}", raw));
            }
            let reg = parse_register64(operands[0]).ok_or_else(|| format!("[ ERROR ] :: [assembler] invalid register in pop: {}", raw))?;
            Ok(Some(Instruction::PopReg { reg }))  
        }
        "mov" | "add" | "sub" => {
            if operands.len() != 2 {
                return Err(format!("[ ERROR ] :: [assembler] {} should have exactly 2 operands: {}", opcode, raw));
            }
            let dst = parse_register64(operands[0]).ok_or_else(|| format!("[ ERROR ] :: [assembler] invalid dst register in {}: {}", opcode, raw))?;
            if let Ok(imm) = operands[1].parse::<i64>() {
                if opcode.eq_ignore_ascii_case("mov") {
                    Ok(Some(Instruction::MovRegImm64 { dst, imm }))
                } else {
                    Err(format!("[ ERROR ] :: [assembler] only mov supports immediate operands: {}", raw))
                }
            } else {
                let src = parse_register64(operands[1]).ok_or_else(|| format!("[ ERROR ] :: [assembler] invalid src register in {}: {}", opcode, raw))?;
                match opcode.to_ascii_lowercase().as_str() {
                    "mov" => Ok(Some(Instruction::MovRegReg { dst, src })),
                    "add" => Ok(Some(Instruction::AddRegReg { dst, src })),
                    "sub" => Ok(Some(Instruction::SubRegReg { dst, src })),
                    "imul" => Ok(Some(Instruction::ImulRegReg { dst, src })),
                    _ => Err(format!("[ ERROR ] :: [assembler] unsupported opcode: {}", raw)),
                }
            }
        }
        _ => Err(format!("[ ERROR ] :: [assembler] unsupported opcode: {}", raw)),
    }
}

fn encode(instruction: &Instruction) -> Vec<u8> {
    match instruction {
        Instruction::Ret => vec![0xC3],
    
        Instruction::MovRegImm64 { dst, imm } => {
            // REX.W (0x48) + Opcode (0xB8 + reg_id) + 8-byte immediate
            let mut out = vec![rex(true, false, false, dst.ext()), 0xB8 + dst.low3()];
            out.extend_from_slice(&imm.to_le_bytes());
            out
        }

        Instruction::MovRegReg { dst, src } => {
            // Opcode 0x89 is MOV r/m64, r64
            let r = rex(true, src.ext(), false, dst.ext());
            let m = modrm(0b11, src.low3(), dst.low3()); // 0b11 means register-to-register
            vec![r, 0x89, m]
        }
        
        Instruction::AddRegReg { dst, src } => {
            // Opcode 0x01 is ADD r/m64, r64
            let r = rex(true, src.ext(), false, dst.ext());
            let m = modrm(0b11, src.low3(), dst.low3());
            vec![r, 0x01, m]
        }

        Instruction::SubRegReg { dst, src } => {
            // Opcode 0x29 is SUB r/m64, r64
            let r = rex(true, src.ext(), false, dst.ext());
            let m = modrm(0b11, src.low3(), dst.low3());
            vec![r, 0x29, m]
        }

        Instruction::ImulRegReg { dst, src } => {
            // Opcode 0x0F AF is IMUL r64, r/m64
            let r = rex(true, dst.ext(), false, src.ext());
            let m = modrm(0b11, dst.low3(), src.low3());
            vec![r, 0x0F, 0xAF, m]
        }
        
        Instruction::PushReg { reg } => {
            // PUSH is special: 0x50 + reg_id. 
            // If reg >= R8, it needs a REX prefix (0x41)
            if reg.ext() {
                vec![0x41, 0x50 + reg.low3()]
            } else {
                vec![0x50 + reg.low3()]
            }
        }

        Instruction::PopReg { reg } => {
            // POP is special: 0x58 + reg_id. 
            // If reg >= R8, it needs a REX prefix (0x41)
            if reg.ext() {
                vec![0x41, 0x58 + reg.low3()]
            } else {
                vec![0x58 + reg.low3()]
            }
        }
    }
}

fn assemble_to_bytes(input_text: &str) -> Result<Vec<u8>, String> {
    let mut machine_code = Vec::new();
    for (line_no, line) in input_text.lines().enumerate() {
        if let Some(instr) = parse_intel_line(line)
            .map_err(|e| format!("line {}: {}", line_no + 1, e))?
        {
            let encoded_bytes = encode(&instr);
            machine_code.extend(encoded_bytes);
        }
    }
    Ok(machine_code)
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input.s> <output.o>", args[0]);
        process::exit(1);
    }

    let input_path = Path::new(&args[1]);
    let output_path = Path::new(&args[2]);

    // Stage 1: read source code
    let source = std::fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("[ ERROR ] :: [assembler] failed to read {:?}: {}", input_path, e);
        process::exit(1);
    });

    // Stage 2: Assemble the source into machine code bytes
    match assemble_to_bytes(&source) {
        Ok(machine_code) => {
            // Stage 3: Write the raw bytes to the output file
            if let Err(e) = std::fs::write(output_path, machine_code) {
                eprintln!("[ ERROR ] :: Failed to write to {:?}: {}", output_path, e);
                process::exit(1);
            }
            println!("[ SUCCESS ] :: Assembled to {:?}", output_path);
        }
        Err(e) => {
            eprintln!("[ ERROR ] :: Assembler error: {}", e);
            process::exit(1);
        }
    }
}


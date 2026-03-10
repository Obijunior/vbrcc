// use crate::register::Register64;
use crate::instruction::{Instruction, Section};
use std::collections::HashMap;

fn rex(w: bool, r: bool, x: bool, b: bool) -> u8 {
    0x40 | ((w as u8) << 3) | ((r as u8) << 2) | ((x as u8) << 1) | (b as u8)
}

fn modrm(mod_bits: u8, reg: u8, rm: u8) -> u8 {
    ((mod_bits & 0b11) << 6) | ((reg & 0b111) << 3) | (rm & 0b111)
}

pub fn encoded_len(instruction: &Instruction) -> usize {
    match instruction {
        Instruction::Ret => 1,
        Instruction::Syscall => 2,
        Instruction::MovRegImm64 { .. } => 10,
        Instruction::MovRegReg { .. } => 3,
        Instruction::AddRegReg { .. } => 3,
        Instruction::SubRegReg { .. } => 3,
        Instruction::AndRegReg { .. } => 3,
        Instruction::AndRegImm32 { .. } => 7,
        Instruction::ImulRegReg { .. } => 4,
        Instruction::PushReg { reg } => if reg.ext() { 2 } else { 1 },
        Instruction::PopReg { reg } => if reg.ext() { 2 } else { 1 },
        Instruction::LeaRegLabel { .. } => 7,
        Instruction::CallLabel { .. } => 5,
    }
}

pub fn encode(instruction: &Instruction) -> Vec<u8> {
    match instruction {
        Instruction::Ret => vec![0xC3],
        Instruction::Syscall => vec![0x0F, 0x05],
    
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

        Instruction::AndRegReg { dst, src } => {
            // Opcode 0x21 is AND r/m64, r64
            let r = rex(true, src.ext(), false, dst.ext());
            let m = modrm(0b11, src.low3(), dst.low3());
            vec![r, 0x21, m]
        }

        Instruction::AndRegImm32 { dst, imm } => {
            // Opcode 0x81 /4 is AND r/m64, imm32 (sign-extended)
            let r = rex(true, false, false, dst.ext());
            let m = modrm(0b11, 0b100, dst.low3()); // /4 in reg field
            let mut out = vec![r, 0x81, m];
            out.extend_from_slice(&imm.to_le_bytes());
            out
        }

        Instruction::ImulRegReg { dst, src } => {
            // Opcode 0x0F AF is IMUL r64, r/m64
            let r = rex(true, dst.ext(), false, src.ext());
            let m = modrm(0b11, dst.low3(), src.low3());
            vec![r, 0x0F, 0xAF, m]
        }
        
        Instruction::PushReg { reg } => {
            if reg.ext() {
                vec![0x41, 0x50 + reg.low3()]
            } else {
                vec![0x50 + reg.low3()]
            }
        }

        Instruction::PopReg { reg } => {
            if reg.ext() {
                vec![0x41, 0x58 + reg.low3()]
            } else {
                vec![0x58 + reg.low3()]
            }
        }

        Instruction::LeaRegLabel { .. } | Instruction::CallLabel { .. } => {
            // Label-based encodings require relocation information.
            Vec::new()
        }
    }
}

pub fn encode_with_labels(
    instruction: &Instruction,
    labels: &HashMap<String, (Section, usize)>,
    instr_rva: u32,
    text_rva: u32,
    data_rva: u32,
) -> Result<Vec<u8>, String> {
    match instruction {
        Instruction::LeaRegLabel { dst, label } => {
            let (section, offset) = labels
                .get(label)
                .ok_or_else(|| format!("[ ERROR ] :: unknown label: {}", label))?;
            let target_rva = match section {
                Section::Text => text_rva + (*offset as u32),
                Section::Data => data_rva + (*offset as u32),
            };
            let next_rva = instr_rva + encoded_len(instruction) as u32;
            let disp = (target_rva as i64) - (next_rva as i64);
            let disp32 = i32::try_from(disp)
                .map_err(|_| format!("[ ERROR ] :: lea target out of range: {}", label))?;
            let r = rex(true, dst.ext(), false, false);
            let m = modrm(0b00, dst.low3(), 0b101); // RIP-relative
            let mut out = vec![r, 0x8D, m];
            out.extend_from_slice(&disp32.to_le_bytes());
            Ok(out)
        }
        Instruction::CallLabel { label } => {
            let (section, offset) = labels
                .get(label)
                .ok_or_else(|| format!("[ ERROR ] :: unknown label: {}", label))?;
            if *section != Section::Text {
                return Err(format!("[ ERROR ] :: call target must be in .text: {}", label));
            }
            let target_rva = text_rva + (*offset as u32);
            let next_rva = instr_rva + encoded_len(instruction) as u32;
            let disp = (target_rva as i64) - (next_rva as i64);
            let disp32 = i32::try_from(disp)
                .map_err(|_| format!("[ ERROR ] :: call target out of range: {}", label))?;
            let mut out = vec![0xE8];
            out.extend_from_slice(&disp32.to_le_bytes());
            Ok(out)
        }
        _ => Ok(encode(instruction)),
    }
}

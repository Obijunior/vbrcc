// use crate::register::Register64;
use crate::instruction::{Instruction, Section};
use std::collections::{HashMap, HashSet};

fn rex(w: bool, r: bool, x: bool, b: bool) -> u8 {
    0x40 | ((w as u8) << 3) | ((r as u8) << 2) | ((x as u8) << 1) | (b as u8)
}

fn modrm(mod_bits: u8, reg: u8, rm: u8) -> u8 {
    ((mod_bits & 0b11) << 6) | ((reg & 0b111) << 3) | (rm & 0b111)
}

fn mem_disp_len(base_low3: u8, disp: i32) -> usize {
    let disp_len = if disp == 0 && base_low3 != 0b101 {
        0
    } else if disp >= -128 && disp <= 127 {
        1
    } else {
        4
    };
    let sib_len = if base_low3 == 0b100 { 1 } else { 0 };
    1 + sib_len + disp_len // modrm + sib? + disp
}

fn encode_mem_disp(reg_field: u8, base_low3: u8, disp: i32) -> Vec<u8> {
    let (mod_bits, disp_bytes) = if disp == 0 && base_low3 != 0b101 {
        (0b00, Vec::new())
    } else if disp >= -128 && disp <= 127 {
        (0b01, vec![(disp as i8) as u8])
    } else {
        (0b10, disp.to_le_bytes().to_vec())
    };

    let rm = if base_low3 == 0b100 { 0b100 } else { base_low3 };
    let mut out = Vec::new();
    out.push(modrm(mod_bits, reg_field, rm));
    if rm == 0b100 {
        let sib = (0b00 << 6) | (0b100 << 3) | base_low3;
        out.push(sib);
    }
    out.extend_from_slice(&disp_bytes);
    out
}

pub fn encoded_len(instruction: &Instruction) -> usize {
    match instruction {
        Instruction::Ret => 1,
        Instruction::Cqo => 2,
        Instruction::Syscall => 2,
        Instruction::AddRegReg { .. } => 3,
        Instruction::AddRegImm32 { .. } => 7,
        Instruction::SubRegReg { .. } => 3,
        Instruction::SubRegImm32 { .. } => 7,
        Instruction::CmpRegReg { .. } => 3,
        Instruction::CmpRegImm32 { .. } => 7,
        Instruction::AndRegReg { .. } => 3,
        Instruction::AndRegImm32 { .. } => 7,
        Instruction::ImulRegReg { .. } => 4,
        Instruction::ImulRegImm32 { .. } => 7,
        Instruction::PushReg { reg } => if reg.ext() { 2 } else { 1 },
        Instruction::PopReg { reg } => if reg.ext() { 2 } else { 1 },
        Instruction::NegReg { .. } => 3,
        Instruction::NotReg { .. } => 3,
        Instruction::IdivReg { .. } => 3,
        Instruction::LeaRegLabel { .. } => 7,
        Instruction::CallLabel { .. } => 5,

        Instruction::SeteReg8 { .. } => 3,
        Instruction::SetneReg8 { .. } => 3,
        Instruction::SetlReg8 { .. } => 3,
        Instruction::SetgReg8 { .. } => 3,
        Instruction::SetleReg8 { .. } => 3,
        Instruction::SetgeReg8 { .. } => 3,

        Instruction::JeLabel { .. } => 6,
        Instruction::JneLabel { .. } => 6,
        Instruction::JlLabel { .. } => 6,
        Instruction::JgLabel { .. } => 6,
        Instruction::JleLabel { .. } => 6,
        Instruction::JgeLabel { .. } => 6,
        Instruction::JmpLabel { .. } => 5,
        
        Instruction::MovRegImm64 { .. } => 10,
        Instruction::MovRegReg { .. } => 3,
        Instruction::MovzxReg64Reg8 { .. } => 4,
        Instruction::MovMemDispReg { base, disp, .. } => {
            2 + mem_disp_len(base.low3(), *disp)
        }
        Instruction::MovRegMemDisp { base, disp, .. } => {
            2 + mem_disp_len(base.low3(), *disp)
        }
    }
}

pub fn encoded_len_with_labels(instruction: &Instruction, label_set: &HashSet<String>) -> usize {
    match instruction {
        Instruction::CallLabel { label } => {
            if label_set.contains(label) { 5 } else { 6 }
        }
        Instruction::JeLabel { .. } | Instruction::JneLabel { .. }
        | Instruction::JlLabel { .. } | Instruction::JgLabel { .. }
        | Instruction::JleLabel { .. } | Instruction::JgeLabel { .. } => {
            6
        }
        Instruction::JmpLabel { .. } => {
            5
        }
        _ => encoded_len(instruction),
    }
}

pub fn encode(instruction: &Instruction) -> Vec<u8> {
    match instruction {
        Instruction::Ret => vec![0xC3],
        Instruction::Cqo => vec![0x48, 0x99],
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

        Instruction::MovMemDispReg { base, disp, src } => {
            // Opcode 0x89 is MOV r/m64, r64
            let r = rex(true, src.ext(), false, base.ext());
            let mut out = vec![r, 0x89];
            out.extend_from_slice(&encode_mem_disp(src.low3(), base.low3(), *disp));
            out
        }

        Instruction::MovRegMemDisp { dst, base, disp } => {
            // Opcode 0x8B is MOV r64, r/m64
            let r = rex(true, dst.ext(), false, base.ext());
            let mut out = vec![r, 0x8B];
            out.extend_from_slice(&encode_mem_disp(dst.low3(), base.low3(), *disp));
            out
        }

        Instruction::MovzxReg64Reg8 { dst, src } => {
            // REX.W + 0F B6 /r is MOVZX r64, r/m8
            let r = rex(true, dst.ext(), false, src.ext());
            let m = modrm(0b11, dst.low3(), src.low3());
            vec![r, 0x0F, 0xB6, m]
        }
        
        Instruction::AddRegReg { dst, src } => {
            // Opcode 0x01 is ADD r/m64, r64
            let r = rex(true, src.ext(), false, dst.ext());
            let m = modrm(0b11, src.low3(), dst.low3());
            vec![r, 0x01, m]
        }

        Instruction::AddRegImm32 { dst, imm } => {
            // Opcode 0x81 /0 is ADD r/m64, imm32 (sign-extended)
            let r = rex(true, false, false, dst.ext());
            let m = modrm(0b11, 0b000, dst.low3());
            let mut out = vec![r, 0x81, m];
            out.extend_from_slice(&imm.to_le_bytes());
            out
        }

        Instruction::SubRegReg { dst, src } => {
            // Opcode 0x29 is SUB r/m64, r64
            let r = rex(true, src.ext(), false, dst.ext());
            let m = modrm(0b11, src.low3(), dst.low3());
            vec![r, 0x29, m]
        }

        Instruction::SubRegImm32 { dst, imm } => {
            // Opcode 0x81 /5 is SUB r/m64, imm32 (sign-extended)
            let r = rex(true, false, false, dst.ext());
            let m = modrm(0b11, 0b101, dst.low3());
            let mut out = vec![r, 0x81, m];
            out.extend_from_slice(&imm.to_le_bytes());
            out
        }

        Instruction::CmpRegReg { dst, src } => {
            let r = rex(true, src.ext(), false, dst.ext());
            let m = modrm(0b11, src.low3(), dst.low3());
            vec![r, 0x39, m]
        }

        Instruction::CmpRegImm32 { dst, imm } => {
            let r = rex(true, false, false, dst.ext());
            let m = modrm(0b11, 0b111, dst.low3());
            let mut out = vec![r, 0x81, m];
            out.extend_from_slice(&imm.to_le_bytes());
            out
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

        Instruction::ImulRegImm32 { dst, imm } => {
            let r = rex(true, false, false, dst.ext());
            let m = modrm(0b11, dst.low3(), dst.low3()); // Opcode 0x69 is IMUL r64, r/m64, imm32
            let mut out = vec![r, 0x69, m];
            out.extend_from_slice(&imm.to_le_bytes());
            out
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

        Instruction::NegReg { reg } => {
            let r = rex(true, false, false, reg.ext());
            let m = modrm(0b11, 0b011, reg.low3()); // /3 in reg field
            vec![r, 0xF7, m]
        }

        Instruction::NotReg { reg } => {
            let r = rex(true, false, false, reg.ext());
            let m = modrm(0b11, 0b010, reg.low3()); // /2 in reg field
            vec![r, 0xF7, m]
        }

        Instruction::IdivReg { reg } => {
            let r = rex(true, false, false, reg.ext());
            let m = modrm(0b11, 0b111, reg.low3()); // /7 in reg field
            vec![r, 0xF7, m]
        }

        Instruction::SeteReg8 { reg } => {
            let m = modrm(0b11, 0b000, reg.low3()); // /0 in reg field
            vec![0x0F, 0x94, m]
        }

        Instruction::SetneReg8 { reg } => {
            let m = modrm(0b11, 0b000, reg.low3()); // /0 in reg field
            vec![0x0F, 0x95, m]
        }

        Instruction::SetlReg8 { reg } => {
            let m = modrm(0b11, 0b000, reg.low3()); // /0 in reg field
            vec![0x0F, 0x9C, m]
        }

        Instruction::SetgReg8 { reg } => {
            let m = modrm(0b11, 0b000, reg.low3()); // /0 in reg field
            vec![0x0F, 0x9F, m]
        }

        Instruction::SetleReg8 { reg } => {
            let m = modrm(0b11, 0b000, reg.low3()); // /0 in reg field
            vec![0x0F, 0x9E, m]
        }

        Instruction::SetgeReg8 { reg } => {
            let m = modrm(0b11, 0b000, reg.low3()); // /0 in reg field
            vec![0x0F, 0x9D, m]
        }

        Instruction::LeaRegLabel { .. } | Instruction::CallLabel { .. } 
        | Instruction::JeLabel { .. } | Instruction::JneLabel { .. } 
        | Instruction::JlLabel { .. } | Instruction::JgLabel { .. }
        | Instruction::JleLabel { .. } | Instruction::JgeLabel { .. }
        | Instruction::JmpLabel { .. } => {
            // Label-based encodings require relocation information.
            Vec::new()
        }
    }
}

pub fn encode_with_labels(
    instruction: &Instruction,
    labels: &HashMap<String, (Section, usize)>,
    externs: &HashMap<String, u32>,
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
            if let Some((section, offset)) = labels.get(label) {
                if *section != Section::Text {
                    return Err(format!("[ ERROR ] :: call target must be in .text: {}", label));
                }
                let target_rva = text_rva + (*offset as u32);
                let next_rva = instr_rva + 5;
                let disp = (target_rva as i64) - (next_rva as i64);
                let disp32 = i32::try_from(disp)
                    .map_err(|_| format!("[ ERROR ] :: call target out of range: {}", label))?;
                let mut out = vec![0xE8];
                out.extend_from_slice(&disp32.to_le_bytes());
                Ok(out)
            } else if let Some(&iat_rva) = externs.get(label) {
                // call qword ptr [rip + disp32] -> FF /2
                let next_rva = instr_rva + 6;
                let disp = (iat_rva as i64) - (next_rva as i64);
                let disp32 = i32::try_from(disp)
                    .map_err(|_| format!("[ ERROR ] :: call target out of range: {}", label))?;
                let mut out = vec![0xFF, 0x15];
                out.extend_from_slice(&disp32.to_le_bytes());
                Ok(out)
            } else {
                Err(format!("[ ERROR ] :: unknown label: {}", label))
            }
        }
        Instruction::JeLabel { label } | Instruction::JneLabel { label }
        | Instruction::JlLabel { label } | Instruction::JgLabel { label }
        | Instruction::JleLabel { label } | Instruction::JgeLabel { label } => {
            let opcode_byte = match instruction {
                Instruction::JeLabel { .. } => 0x84,
                Instruction::JneLabel { .. } => 0x85,
                Instruction::JlLabel { .. } => 0x8C,
                Instruction::JgLabel { .. } => 0x8F,
                Instruction::JleLabel { .. } => 0x8E,
                Instruction::JgeLabel { .. } => 0x8D,
                _ => unreachable!(),
            };
            let (section, offset) = labels
                .get(label)
                .ok_or_else(|| format!("[ ERROR ] :: unknown label: {}", label))?;
            if *section != Section::Text {
                return Err(format!("[ ERROR ] :: jump target must be in .text: {}", label));
            }
            let target_rva = text_rva + (*offset as u32);
            let next_rva = instr_rva + 6; // 2 opcode bytes + 4 rel32 bytes
            let disp = (target_rva as i64) - (next_rva as i64);
            let disp32 = i32::try_from(disp)
                .map_err(|_| format!("[ ERROR ] :: jump target out of range: {}", label))?;
            let mut out = vec![0x0F, opcode_byte];
            out.extend_from_slice(&disp32.to_le_bytes());
            Ok(out)
        }
        Instruction::JmpLabel { label } => {
            let (section, offset) = labels
                .get(label)
                .ok_or_else(|| format!("[ ERROR ] :: unknown label: {}", label))?;
            if *section != Section::Text {
                return Err(format!("[ ERROR ] :: jump target must be in .text: {}", label));
            }
            let target_rva = text_rva + (*offset as u32);
            let next_rva = instr_rva + 5;
            let disp = (target_rva as i64) - (next_rva as i64);
            let disp32 = i32::try_from(disp)
                .map_err(|_| format!("[ ERROR ] :: jump target out of range: {}", label))?;
            let mut out = vec![0xE9];
            out.extend_from_slice(&disp32.to_le_bytes());
            Ok(out)
        }
        _ => Ok(encode(instruction)),
    }
}
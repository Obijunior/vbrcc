// use crate::register::Register64;
use crate::instruction::Instruction;

fn rex(w: bool, r: bool, x: bool, b: bool) -> u8 {
    0x40 | ((w as u8) << 3) | ((r as u8) << 2) | ((x as u8) << 1) | (b as u8)
}

fn modrm(mod_bits: u8, reg: u8, rm: u8) -> u8 {
    ((mod_bits & 0b11) << 6) | ((reg & 0b111) << 3) | (rm & 0b111)
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
    }
}

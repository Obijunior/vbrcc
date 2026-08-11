//! The x86-64 registers and their encoding bits.
//!
//! Register numbers on x86-64 are neither alphabetical nor intuitive. The first eight
//! registers keep their 8086 numbers: `rax` is 0, `rcx` is 1, `rdx` is 2, `rbx` is 3,
//! `rsp` is 4, `rbp` is 5, `rsi` is 6, and `rdi` is 7. Then `r8` to `r15` take 8 to 15.
//! The `id` method returns exactly those numbers.
//!
//! An instruction field holds only three bits, so each register splits in two. `low3`
//! gives the bits for the ModR/M field or the opcode field. `ext` reports the fourth
//! bit, which the caller puts into a REX prefix. Callers in [`super::encoder`] need
//! both parts.
//!
//! [`Register8`] covers only `al`, `bl`, `cl`, and `dl`. The code generator uses these
//! for a `set<cc>` result, and then widens the result with `movzx`.

#[derive(Debug, Clone, Copy)]
pub enum Register64 {
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
    pub fn low3(self) -> u8 { self.id() & 0b111 }
    pub fn ext(self) -> bool { self.id() >= 8 }
}

#[derive(Debug, Clone, Copy)]
pub enum Register8 {
    Al, Bl, Cl, Dl,
}

impl Register8 {
    fn id(self) -> u8 {
        match self {
            Self::Al => 0, Self::Cl => 1, Self::Dl => 2, Self::Bl => 3,
        }
    }
    pub fn low3(self) -> u8 { self.id() & 0b111 }
    pub fn ext(self) -> bool { false }
}

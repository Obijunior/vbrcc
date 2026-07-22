//! x86-64 register definitions and their encoding bits.
//!
//! Register numbering on x86-64 follows neither alphabetical nor intuitive order. The classic
//! eight registers keep their historical 8086 numbering (`rax`=0, `rcx`=1, `rdx`=2,
//! `rbx`=3, `rsp`=4, `rbp`=5, `rsi`=6, `rdi`=7), with `r8`–`r15` following as 8–15.
//! The `id` mapping here encodes exactly that.
//!
//! Because instruction fields hold only three bits, each register splits in two: `low3`
//! gives the bits that go into the ModR/M or opcode field, and `ext` reports whether the
//! fourth bit is set, which the caller must fold into a REX prefix. Callers in
//! [`super::encoder`] always need both halves.
//!
//! [`Register8`] covers only `al`, `bl`, `cl` and `dl`, the sub-registers the code
//! generator uses for `set<cc>` results before widening them with `movzx`.

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

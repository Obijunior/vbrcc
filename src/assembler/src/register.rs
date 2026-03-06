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
//! Relocations, symbols, and the output of an assembly pass.
//!
//! These types are the interface between [`super::encoder`] and the two container
//! writers, [`super::pe`] and [`super::coff`].
//!
//! A [`Relocation`] marks a place in the encoded bytes that the encoder could not
//! finish, such as a call to a symbol with no known address. It records the target
//! symbol and how to apply the final value.
//!
//! A [`Symbol`] with `section: None` is external. A linker or an import table must
//! resolve it. Any other symbol is defined at `offset` inside the named section.
//!
//! [`AssembleResult`] is what a full pass returns: the `.text` and `.data` bytes, plus
//! the relocation and symbol tables that the container writer needs.

use super::instruction::Section;

#[derive(Debug)]
pub struct Relocation {
    pub offset: u32,         // byte offset within the section where the patch goes
    pub symbol_name: String, // name of the target symbol
    pub rel_type: RelocationType,
}

#[derive(Debug, Copy, Clone)]
pub enum RelocationType {
    Rel32 = 0x0004,     // IMAGE_REL_AMD64_REL32 (0x0004) -- RIP-relative 32-bit
}

pub struct AssembleResult {
    pub text_bytes: Vec<u8>,
    pub data_bytes: Vec<u8>,
    pub text_relocs: Vec<Relocation>,
    pub symbols: Vec<Symbol>,   // defined + external symbols
}

pub struct Symbol {
    pub name: String,
    pub section: Option<Section>,  // None = external/undefined
    pub offset: u32,               // offset within section (0 for externals)
}

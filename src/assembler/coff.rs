//! Writing a relocatable COFF object file.
//!
//! This is the `--lld-link` output path. Where [`super::pe`] produces a finished
//! executable, this module produces a `.obj` with unresolved references left for an
//! external linker to fill in, which is what makes linking against the real C runtime
//! possible.
//!
//! A COFF object here consists of a header, section headers and data for `.text` and
//! `.data`, a relocation table, and a symbol table with an attached string table.
//!
//! # Symbols and relocations
//!
//! Every symbol is either *defined* (it lives at a known offset in one of this object's
//! sections, emitted as `IMAGE_SYM_CLASS_STATIC` or `IMAGE_SYM_CLASS_EXTERNAL`) or
//! *undefined*: a reference such as `printf` that the linker must resolve against
//! another object or import library. Undefined symbols carry section number 0.
//!
//! References are emitted as `IMAGE_REL_AMD64_REL32` relocations, meaning RIP-relative
//! 32-bit displacements. The value written into the instruction stream is a placeholder;
//! the linker computes the final displacement once it knows where everything landed.
//!
//! # String table
//!
//! COFF stores symbol names inline only when they fit in eight bytes. Longer names go
//! into a trailing string table, and the symbol record instead holds a zero word
//! followed by an offset into it. Both encodings are produced here depending on name
//! length.

use std::collections::HashMap;

use super::relocation::AssembleResult;
use super::instruction::Section;

// === Helper Functions ===

// 2 bytes
fn push_u16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

// 4 bytes
fn push_u32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

// COFF object generator
pub fn create_coff_obj(source: &AssembleResult) -> Vec<u8> {
    const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
    const IMAGE_SYM_CLASS_EXTERNAL: u8 = 2;
    const IMAGE_SYM_CLASS_STATIC: u8 = 3;
    // const IMAGE_SYM_UNDEFINED: i16 = 0;
    // const IMAGE_REL_AMD64_REL32: u16 = 0x0004;  <- defined in RelocationType
    const COFF_HEADER_SIZE: usize = 20;
    const SECTION_HEADER_SIZE: usize = 40;
    const SYMBOL_SIZE: usize = 18;
    const RELOCATION_SIZE: usize = 10;

    let text_bytes = &source.text_bytes;
    let data_bytes = &source.data_bytes;
    let symbols = &source.symbols;

    let r = source.text_relocs.len();
    let num_sections = 2;                 // always .text + .data

    // let file_header = 0;
    // let section_headers = 20;
    let text_raw_ptr = COFF_HEADER_SIZE+(SECTION_HEADER_SIZE*num_sections);
    let text_reloc_ptr = text_raw_ptr + text_bytes.len();
    let data_raw_ptr = text_reloc_ptr + (RELOCATION_SIZE*r);
    let symtab_ptr = data_raw_ptr + data_bytes.len();

    let mut coff = Vec::new();
    let mut strings: Vec<u8> = Vec::new();
    let sym_index: HashMap<&str, u32> = symbols.iter().enumerate()
        .map(|(i, s)| (s.name.as_str(), i as u32))
        .collect();

    // --- 1. COFF header ---
    push_u16(&mut coff, IMAGE_FILE_MACHINE_AMD64);       // Machine
    push_u16(&mut coff, num_sections as u16);            // NumberOfSections
    push_u32(&mut coff, 0);                              // TimeDateStamp
    push_u32(&mut coff, symtab_ptr as u32);              // PointerToSymbolTable
    push_u32(&mut coff, symbols.len() as u32);    // NumberOfSymbols
    push_u16(&mut coff, 0);                              // SizeOfOptionalHeader
    push_u16(&mut coff, 0);                              // Characteristics

    // --- 2. Section headers ---

    // --- 2a. .text
    coff.extend_from_slice(b".text\0\0\0");                // Name
    push_u32(&mut coff, 0);                         // VirtualSize
    push_u32(&mut coff, 0);                         // VirtualAddress
    push_u32(&mut coff, text_bytes.len() as u32);   // SizeOfRawData
    push_u32(&mut coff, text_raw_ptr as u32);       // PointerToRawData
    push_u32(&mut coff, if r == 0 { 0 } else { text_reloc_ptr as u32 });   // PointerToRelocations
    push_u32(&mut coff, 0);                         // PointerToLineNumbers
    push_u16(&mut coff, r as u16);                  // NumberOfRelocations
    push_u16(&mut coff, 0);                         // NumberOfLineNumbers
    push_u32(&mut coff, 0x60500020);                // Characteristics


    // --- 2b. .data
    coff.extend_from_slice(b".data\0\0\0");                // Name
    push_u32(&mut coff, 0);                         // VirtualSize
    push_u32(&mut coff, 0);                         // VirtualAddress
    push_u32(&mut coff, data_bytes.len() as u32);   // SizeOfRawData
    push_u32(&mut coff, if data_bytes.is_empty() { 0 } else { data_raw_ptr as u32 });  // PointerToRawData
    push_u32(&mut coff, 0);                         // PointerToRelocations
    push_u32(&mut coff, 0);                         // PointerToLineNumbers
    push_u16(&mut coff, 0);                         // NumberOfRelocations
    push_u16(&mut coff, 0);                         // NumberOfLineNumbers
    push_u32(&mut coff, 0xC0500040);                // Characteristics

    // --- 3a. .text raw data
    coff.extend_from_slice(text_bytes);
    debug_assert_eq!(coff.len(), text_reloc_ptr);

    // --- 3b. .text relocations - 10 bytes each
    for reloc in &source.text_relocs {
        let idx = *sym_index.get(reloc.symbol_name.as_str())
            .unwrap_or_else(|| panic!("relocation references undefined symbol: {}", reloc.symbol_name));
        push_u32(&mut coff, reloc.offset);                  // VirtualAddress
        push_u32(&mut coff, idx);                           // SymbolTableIndex
        push_u16(&mut coff, reloc.rel_type as u16);         // Type
    }
    debug_assert_eq!(coff.len(), data_raw_ptr);

    // --- 3c. .data raw data
    coff.extend_from_slice(data_bytes);
    debug_assert_eq!(coff.len(), symtab_ptr);

    // --- 4. symbol table + string table
    for sym in symbols {
        // Name fields (8 bytes): inline if short, string-table ref if long
        if sym.name.len() <= 8 {
            let mut name_field = [0u8; 8];
            name_field[..sym.name.len()].copy_from_slice(sym.name.as_bytes());
            coff.extend_from_slice(&name_field);
        } else {
            let offset = 4 + strings.len() as u32;          // +4 for the size header
            strings.extend_from_slice(sym.name.as_bytes());
            strings.push(0);                                     // null-terminate
            push_u32(&mut coff, 0);                       // zero marker
            push_u32(&mut coff, offset);                  // offset into string table
        }

        let (section_number, storage_class): (u16, u8) = match sym.section {
            Some(Section::Text) => (1, IMAGE_SYM_CLASS_EXTERNAL),  // global function (main)
            Some(Section::Data) => (2, IMAGE_SYM_CLASS_STATIC),    // local literal (str_0)
            None                => (0, IMAGE_SYM_CLASS_EXTERNAL),  // undefined external (printf)
        };

        // --- rest of the 18-byte record ---
        push_u32(&mut coff, sym.offset);             // Value
        push_u16(&mut coff, section_number);         // SectionNumber
        push_u16(&mut coff, 0);                      // Type (0x20 for funcs, optional)
        coff.push(storage_class);                           // StorageClass
        coff.push(0);                                       // NumberOfAuxSymbols
    }

    // --- String table: size-prefixed, appended after all symbols ---
    push_u32(&mut coff, 4 + strings.len() as u32);          // total size incl. these 4 bytes
    coff.extend_from_slice(&strings);

    debug_assert_eq!(coff.len(), symtab_ptr + SYMBOL_SIZE * symbols.len() + 4 + strings.len());

    coff
}

#[cfg(test)]
mod tests {
    use super::create_coff_obj;
    use crate::assembler::relocation::{AssembleResult, Symbol, Relocation, RelocationType};
    use crate::assembler::instruction::Section;

    #[test]
    fn test_coff_header_magic() {
        let result = AssembleResult {
            text_bytes: vec![0xC3], // ret
            data_bytes: vec![],
            text_relocs: vec![],
            symbols: vec![Symbol { name: "main".into(), section: Some(Section::Text), offset: 0 }],
        };
        let obj = create_coff_obj(&result);
        // COFF machine type at offset 0
        assert_eq!(u16::from_le_bytes([obj[0], obj[1]]), 0x8664);
    }

    #[test]
    fn test_coff_relocs_and_symbols() {
        let result = AssembleResult {
            text_bytes: vec![0; 16],
            data_bytes: b"hello\0".to_vec(),
            text_relocs: vec![
                Relocation { offset: 3, symbol_name: "str_0".into(), rel_type: RelocationType::Rel32 },
                Relocation { offset: 9, symbol_name: "printf".into(), rel_type: RelocationType::Rel32 },
            ],
            symbols: vec![
                Symbol { name: "main".into(),   section: Some(Section::Text), offset: 0 },
                Symbol { name: "str_0".into(),  section: Some(Section::Data), offset: 0 },
                Symbol { name: "printf".into(), section: None,                offset: 0 },
            ],
        };
        let obj = create_coff_obj(&result);
        // NumberOfSymbols (header offset 12) == 3
        assert_eq!(u32::from_le_bytes(obj[12..16].try_into().unwrap()), 3);
        // .text NumberOfRelocations (in .text hdr, offset 20+32=52) == 2
        assert_eq!(u16::from_le_bytes(obj[52..54].try_into().unwrap()), 2);
        // First reloc entry sits at text_reloc_ptr = 100 + 16 = 116:
        //   VirtualAddress == 3, SymbolTableIndex == 1 (str_0)
        assert_eq!(u32::from_le_bytes(obj[116..120].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(obj[120..124].try_into().unwrap()), 1);
    }

    #[test]
    fn test_coff_long_symbol_name_uses_string_table() {
        let result = AssembleResult {
            text_bytes: vec![0xC3],
            data_bytes: vec![],
            text_relocs: vec![],
            symbols: vec![Symbol { name: "a_very_long_function_name".into(),
                                section: Some(Section::Text), offset: 0 }],
        };
        let obj = create_coff_obj(&result);
        // Name field's first 4 bytes are the zero marker; next 4 are the string-table offset (==4)
        let name_field_start = 100 + 1; // symtab_ptr: 100 hdrs + 1 byte text + 0 data
        assert_eq!(&obj[name_field_start..name_field_start+4], &[0,0,0,0]);
        assert_eq!(u32::from_le_bytes(obj[name_field_start+4..name_field_start+8].try_into().unwrap()), 4);
    }
}

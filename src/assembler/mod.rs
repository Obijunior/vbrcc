use std::collections::{HashMap, HashSet};

pub mod pe;
pub mod instruction;
pub mod register;
pub mod encoder;
pub mod relocation;
pub mod coff;

use self::instruction::{Instruction, Section, AsmLine};
use self::relocation::{Relocation, AssembleResult, Symbol};

fn align(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn write_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u16(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(buf: &mut [u8], offset: usize, value: u64) {
    buf[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn build_import_section(externs: &[String], idata_rva: u32) -> (Vec<u8>, HashMap<String, u32>) {
    if externs.is_empty() {
        return (Vec::new(), HashMap::new());
    }

    let mut names: Vec<String> = externs.iter().cloned().collect();
    names.sort();
    names.dedup();

    let dll_name = "ucrtbase.dll";
    let n = names.len();

    let desc_offset = 0usize;
    let int_offset = 40usize;
    let int_size = 8usize * (n + 1);
    let iat_offset = int_offset + int_size;
    let iat_size = 8usize * (n + 1);

    let mut hint_name_offsets: Vec<usize> = Vec::with_capacity(n);
    let mut cursor = iat_offset + iat_size;

    for name in &names {
        if cursor % 2 != 0 { cursor += 1; }
        hint_name_offsets.push(cursor);
        cursor += 2 + name.as_bytes().len() + 1;
    }

    let dll_name_offset = cursor;
    let total_size = dll_name_offset + dll_name.as_bytes().len() + 1;

    let mut buf = vec![0u8; total_size];

    let int_rva = idata_rva + int_offset as u32;
    let iat_rva = idata_rva + iat_offset as u32;
    let dll_name_rva = idata_rva + dll_name_offset as u32;

    // IMAGE_IMPORT_DESCRIPTOR
    write_u32(&mut buf, desc_offset + 0, int_rva);       // OriginalFirstThunk (INT)
    write_u32(&mut buf, desc_offset + 4, 0);             // TimeDateStamp
    write_u32(&mut buf, desc_offset + 8, 0);             // ForwarderChain
    write_u32(&mut buf, desc_offset + 12, dll_name_rva); // Name
    write_u32(&mut buf, desc_offset + 16, iat_rva);      // FirstThunk (IAT)

    let mut iat_map: HashMap<String, u32> = HashMap::new();

    for (i, name) in names.iter().enumerate() {
        let hint_name_rva = idata_rva + hint_name_offsets[i] as u32;
        let int_entry_off = int_offset + (i * 8);
        let iat_entry_off = iat_offset + (i * 8);

        write_u64(&mut buf, int_entry_off, hint_name_rva as u64);
        write_u64(&mut buf, iat_entry_off, hint_name_rva as u64);

        let hn_off = hint_name_offsets[i];
        let name_bytes = name.as_bytes();
        write_u16(&mut buf, hn_off, 0); // Hint
        buf[hn_off + 2 .. hn_off + 2 + name_bytes.len()].copy_from_slice(name_bytes);
        buf[hn_off + 2 + name_bytes.len()] = 0;

        iat_map.insert(name.clone(), iat_rva + (i as u32 * 8));
    }

    let dll_bytes = dll_name.as_bytes();
    buf[dll_name_offset .. dll_name_offset + dll_bytes.len()].copy_from_slice(dll_bytes);
    buf[dll_name_offset + dll_bytes.len()] = 0;

    (buf, iat_map)
}

// pass 1
fn parse_lines(source: &str) -> Result<(Vec<(Section, AsmLine)>, HashSet<String>, HashSet<String>), String> {
    let mut lines: Vec<(Section, AsmLine)> = Vec::new();
    let mut label_names: HashSet<String> = HashSet::new();
    let mut globals: HashSet<String> = HashSet::new();
    let mut current_section = Section::Text;

    for (line_no, line) in source.lines().enumerate() {
        match instruction::parse_intel_line(line)
            .map_err(|e| format!("L{}: {}", line_no + 1, e))?
        {
            AsmLine::SectionChange(new_section) => {
                current_section = new_section;
            }
            AsmLine::Label(name) => {
                if !label_names.insert(name.clone()) {
                    return Err(format!(
                        "L{}: [ ERROR ] :: duplicate label: {}",
                        line_no + 1,
                        name
                    ));
                }
                lines.push((current_section, AsmLine::Label(name)));
            }
            AsmLine::DataBytes(bytes) => {
                lines.push((current_section, AsmLine::DataBytes(bytes)));
            }
            AsmLine::Instruction(instr) => {
                lines.push((current_section, AsmLine::Instruction(instr)));
            }
            AsmLine::Globl(name) => {
                globals.insert(name);
            }
            AsmLine::None => {}
        }
    }
    Ok((lines, label_names, globals))
}

// pass 2
fn compute_layout(
    lines: &[(Section, AsmLine)],
    len_fn: impl Fn(&Instruction) -> usize,
) -> (HashMap<String, (Section, usize)>, usize, usize) {
    let mut labels: HashMap<String, (Section, usize)> = HashMap::new();
    let mut text_offset: usize = 0;
    let mut data_offset: usize = 0;

    for (section, line) in lines {
        match line {
            AsmLine::Label(name) => {
                let offset = if *section == Section::Text {
                    text_offset
                } else {
                    data_offset
                };
                labels.insert(name.clone(), (*section, offset));
            }
            AsmLine::DataBytes(bytes) => {
                data_offset += bytes.len();
            }
            AsmLine::Instruction(instr) => {
                let size = len_fn(instr);
                if *section == Section::Text {
                    text_offset += size;
                } else {
                    data_offset += size;
                }
            }
            _ => {}
        }
    }

    (labels, text_offset, data_offset)
}

pub fn assemble(source: &str) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>), String> {
    let (lines, label_names, _globals) = parse_lines(source)?;
    let (labels, text_offset, data_offset) = compute_layout(&lines, |i| encoder::encoded_len_with_labels(i, &label_names));


    let mut externs: Vec<String> = Vec::new();
    for (_section, line) in &lines {
        if let AsmLine::Instruction(Instruction::CallLabel { label }) = line {
            if !label_names.contains(label) {
                externs.push(label.clone());
            }
        }
    }

    let text_rva: u32 = 0x1000;
    let data_rva: u32 = (0x1000 + align(text_offset, 0x1000)) as u32;
    let idata_rva: u32 = (data_rva as usize + align(data_offset, 0x1000)) as u32;

    let (idata_section, extern_map) = build_import_section(&externs, idata_rva);

    let mut text_section = Vec::new();
    let mut data_section = Vec::new();
    let mut text_cursor: usize = 0;
    let mut data_cursor: usize = 0;

    // Pass 3: encode with resolved labels
    for (section, line) in lines {
        match line {
            AsmLine::DataBytes(bytes) => {
                let len = bytes.len();
                data_section.extend(bytes);
                data_cursor += len;
            }
            AsmLine::Instruction(instr) => {
                let instr_rva = if section == Section::Text {
                    text_rva + text_cursor as u32
                } else {
                    data_rva + data_cursor as u32
                };
                let bytes = encoder::encode_with_labels(
                    &instr,
                    &labels,
                    &extern_map,
                    instr_rva,
                    text_rva,
                    data_rva,
                )?;
                if section == Section::Text {
                    text_section.extend(&bytes);
                    text_cursor += bytes.len();
                } else {
                    data_section.extend(&bytes);
                    data_cursor += bytes.len();
                }
            }
            _ => {}
        }
    }

    Ok((text_section, data_section, idata_section))
}

pub fn assemble_to_obj(source: &str) -> Result<AssembleResult, String> {
    let (lines, label_names, _globals) = parse_lines(source)?;
    let (labels, _text_size, _data_size) = compute_layout(&lines, encoder::encoded_len);

    let mut externs: Vec<String> = Vec::new();
    for (_section, line) in &lines {
        if let AsmLine::Instruction(Instruction::CallLabel { label }) = line {
            if !label_names.contains(label) && !externs.contains(label) {
                externs.push(label.clone());
            }
        }
    }

    let mut text_bytes  = Vec::new();
    let mut data_bytes  = Vec::new();
    let mut text_relocs: Vec<Relocation> = Vec::new();
    let mut text_cursor: usize = 0;

    for (section, line) in &lines {
        match line {
            AsmLine::DataBytes(bytes) => {
                data_bytes.extend_from_slice(bytes);
            }
            AsmLine::Instruction(instr) => {
                let (bytes, relocs) =
                    encoder::encode_for_obj(instr, &labels, *section, text_cursor as u32)?;
                text_bytes.extend_from_slice(&bytes);
                text_relocs.extend(relocs);
                text_cursor += bytes.len();
            }
            _ => {}
        }
    }

    let mut symbols: Vec<Symbol> = Vec::new();
    for (name, (section, offset)) in &labels {
        symbols.push(Symbol {
            name: name.clone(),
            section: Some(*section),
            offset: *offset as u32,
        });
    }
    for name in &externs {
        symbols.push(Symbol { name: name.clone(), section: None, offset: 0 });
    }

    Ok(AssembleResult { text_bytes, data_bytes, text_relocs, symbols })
}

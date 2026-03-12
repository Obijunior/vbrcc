use std::env;
use std::path::Path;
use std::process;

mod pe;
mod instruction;
mod register;
mod encoder;

use crate::instruction::{AsmLine, Section};
use std::collections::{HashMap, HashSet};

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

    let dll_name = "msvcrt.dll";
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

fn assemble(source: &str) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>), String> {
    let mut lines: Vec<(Section, AsmLine)> = Vec::new();
    let mut label_names: HashSet<String> = HashSet::new();
    let mut current_section = Section::Text;

    // Pass 1: parse and collect label names (no offsets)
    for (line_no, line) in source.lines().enumerate() {
        match instruction::parse_intel_line(line)
            .map_err(|e| format!("L{}: {}", line_no + 1, e))?
        {
            AsmLine::SectionChange(new_section) => {
                current_section = new_section;
            }
            AsmLine::Label(name) => {
                if !label_names.insert(name.clone()) {
                    return Err(format!("L{}: [ ERROR ] :: duplicate label: {}", line_no + 1, name));
                }
                lines.push((current_section, AsmLine::Label(name)));
            }
            AsmLine::DataBytes(bytes) => {
                lines.push((current_section.clone(), AsmLine::DataBytes(bytes)));
            }
            AsmLine::Instruction(instr) => {
                lines.push((current_section.clone(), AsmLine::Instruction(instr)));
            }
            AsmLine::None => {}
        }
    }

    // Pass 2: compute label offsets and section sizes
    let mut labels: HashMap<String, (Section, usize)> = HashMap::new();
    let mut text_offset: usize = 0;
    let mut data_offset: usize = 0;
    for (section, line) in &lines {
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
                let size = encoder::encoded_len_with_labels(instr, &label_names);
                if *section == Section::Text {
                    text_offset += size;
                } else {
                    data_offset += size;
                }
            }
            _ => {}
        }
    }

    let mut externs: Vec<String> = Vec::new();
    for (_section, line) in &lines {
        if let AsmLine::Instruction(crate::instruction::Instruction::CallLabel { label }) = line {
            if !label_names.contains(label) {
                externs.push(label.clone());
            }
        }
    }

    let text_size = text_offset;
    let text_rva: u32 = 0x1000;
    let data_rva: u32 = (0x1000 + align(text_size, 0x1000)) as u32;
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

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input.s> <output.exe>", args[0]);
        process::exit(1);
    }

    let input_path = Path::new(&args[1]);
    let output_path = Path::new(&args[2]);

    // 1. Read the assembly source file
    let source = std::fs::read_to_string(input_path).unwrap_or_else(|e| {
        eprintln!("[ ERROR ] :: Failed to read {:?}: {}", input_path, e);
        process::exit(1);
    });

    // 2. Assemble the source into machine code and data buckets
    // We use a match here instead of '?' because main() returns ()
    match assemble(&source) {
        Ok((text_section, data_section, idata_section)) => {
            
            // 3. Wrap the sections into a Windows Portable Executable (PE)
            let final_exe = pe::create_pe_wrapper(&text_section, &data_section, &idata_section);
            
            // 4. Write the final binary to disk
            if let Err(e) = std::fs::write(output_path, final_exe) {
                eprintln!("[ ERROR ] :: Failed to write to {:?}: {}", output_path, e);
                process::exit(1);
            }

            println!("[ SUCCESS ] :: Created Windows Executable: {:?}", output_path);
            println!("  - .text size: {} bytes", text_section.len());
            println!("  - .data size: {} bytes", data_section.len());
            println!("  - .idata size: {} bytes", idata_section.len());
        }
        Err(e) => {
            eprintln!("[ ERROR ] :: Assembler error: {}", e);
            process::exit(1);
        }
    }
}

use std::env;
use std::path::Path;
use std::process;

mod pe;
mod instruction;
mod register;
mod encoder;

use crate::instruction::{AsmLine, Section};
use std::collections::HashMap;

fn align(value: usize, align: usize) -> usize {
    (value + align - 1) & !(align - 1)
}

fn assemble(source: &str) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut lines: Vec<(Section, AsmLine)> = Vec::new();
    let mut labels: HashMap<String, (Section, usize)> = HashMap::new();
    let mut current_section = Section::Text;
    let mut text_offset: usize = 0;
    let mut data_offset: usize = 0;

    // Pass 1: parse and collect labels + sizes
    for (line_no, line) in source.lines().enumerate() {
        match instruction::parse_intel_line(line)
            .map_err(|e| format!("L{}: {}", line_no + 1, e))?
        {
            AsmLine::SectionChange(new_section) => {
                current_section = new_section;
            }
            AsmLine::Label(name) => {
                let entry = labels.entry(name.clone());
                if let std::collections::hash_map::Entry::Vacant(v) = entry {
                    let offset = if current_section == Section::Text {
                        text_offset
                    } else {
                        data_offset
                    };
                    v.insert((current_section.clone(), offset));
                } else {
                    return Err(format!("L{}: [ ERROR ] :: duplicate label: {}", line_no + 1, name));
                }
            }
            AsmLine::DataBytes(bytes) => {
                data_offset += bytes.len();
                lines.push((current_section.clone(), AsmLine::DataBytes(bytes)));
            }
            AsmLine::Instruction(instr) => {
                let size = encoder::encoded_len(&instr);
                if current_section == Section::Text {
                    text_offset += size;
                } else {
                    data_offset += size;
                }
                lines.push((current_section.clone(), AsmLine::Instruction(instr)));
            }
            AsmLine::None => {}
        }
    }

    let text_size = text_offset;
    let text_rva: u32 = 0x1000;
    let data_rva: u32 = (0x1000 + align(text_size, 0x1000)) as u32;

    let mut text_section = Vec::new();
    let mut data_section = Vec::new();
    let mut text_cursor: usize = 0;
    let mut data_cursor: usize = 0;

    // Pass 2: encode with resolved labels
    for (section, line) in lines {
        match line {
            AsmLine::DataBytes(bytes) => {
                data_section.extend(bytes);
                data_cursor += bytes.len();
            }
            AsmLine::Instruction(instr) => {
                let instr_rva = if section == Section::Text {
                    text_rva + text_cursor as u32
                } else {
                    data_rva + data_cursor as u32
                };
                let bytes = encoder::encode_with_labels(&instr, &labels, instr_rva, text_rva, data_rva)?;
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

    Ok((text_section, data_section))
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
        Ok((text_section, data_section)) => {
            
            // 3. Wrap the sections into a Windows Portable Executable (PE)
            let final_exe = pe::create_pe_wrapper(&text_section, &data_section);
            
            // 4. Write the final binary to disk
            if let Err(e) = std::fs::write(output_path, final_exe) {
                eprintln!("[ ERROR ] :: Failed to write to {:?}: {}", output_path, e);
                process::exit(1);
            }

            println!("[ SUCCESS ] :: Created Windows Executable: {:?}", output_path);
            println!("  - .text size: {} bytes", text_section.len());
            println!("  - .data size: {} bytes", data_section.len());
        }
        Err(e) => {
            eprintln!("[ ERROR ] :: Assembler error: {}", e);
            process::exit(1);
        }
    }
}

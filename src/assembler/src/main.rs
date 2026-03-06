use std::env;
use std::path::Path;
use std::process;

mod pe;
mod instruction;
mod register;
mod encoder;

use crate::instruction::{AsmLine, Section};

fn assemble(source: &str) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut text_section = Vec::new();
    let mut data_section = Vec::new();
    let mut current_section = Section::Text;

    for (line_no, line) in source.lines().enumerate() {
        match instruction::parse_intel_line(line).map_err(|e| format!("L{}: {}", line_no + 1, e))? {
            AsmLine::SectionChange(new_section) => {
                current_section = new_section;
            }
            AsmLine::DataBytes(bytes) => {
                data_section.extend(bytes);
            }
            AsmLine::Instruction(instr) => {
                let bytes = encoder::encode(&instr);
                if current_section == Section::Text {
                    text_section.extend(bytes);
                } else {
                    data_section.extend(bytes);
                }
            }
            AsmLine::None => {
                // Skip comments and empty lines
            }
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
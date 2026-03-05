use std::env;
use std::path::Path;
use std::process;

#[derive(Debug)]
enum Register64 {
    Rax,
}

#[derive(Debug)]
enum Instruction {
    MovImm64 { dst: Register64, imm: i64 },
    Ret,
}

fn parse_intel_line(raw: &str) -> Result<Option<Instruction>, String> {
    let line = raw.trim();
    if line.is_empty() || line.starts_with(';') {
        return Ok(None);
    }

    // Allow passthrough for labels/directives for now.
    if line.ends_with(':') || line.starts_with('.') {
        return Ok(None);
    }

    if line.eq_ignore_ascii_case("ret") {
        return Ok(Some(Instruction::Ret));
    }

    // Small starter subset: mov rax, <imm64>
    if let Some(rest) = line.strip_prefix("mov ") {
        let mut parts = rest.split(',').map(str::trim);
        let dst = parts
            .next()
            .ok_or_else(|| format!("invalid mov syntax: {}", raw))?;
        let src = parts
            .next()
            .ok_or_else(|| format!("invalid mov syntax: {}", raw))?;
        if parts.next().is_some() {
            return Err(format!("invalid mov syntax: {}", raw));
        }

        let dst = match dst.to_ascii_lowercase().as_str() {
            "rax" => Register64::Rax,
            _ => return Err(format!("unsupported destination register: {}", dst)),
        };
        let imm = src
            .parse::<i64>()
            .map_err(|_| format!("unsupported immediate value: {}", src))?;
        return Ok(Some(Instruction::MovImm64 { dst, imm }));
    }

    Ok(None)
}

fn encode(instr: &Instruction) -> Vec<u8> {
    match instr {
        Instruction::Ret => vec![0xC3],
        Instruction::MovImm64 {
            dst: Register64::Rax,
            imm,
        } => {
            let mut out = vec![0x48, 0xB8];
            out.extend_from_slice(&imm.to_le_bytes());
            out
        }
    }
}

fn validate_supported_intel_subset(input_text: &str) -> Result<(), String> {
    for (line_no, line) in input_text.lines().enumerate() {
        if let Some(instr) = parse_intel_line(line)
            .map_err(|e| format!("line {}: {}", line_no + 1, e))?
        {
            let _encoded_bytes = encode(&instr);
        }
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <input.s> <output.o>", args[0]);
        process::exit(1);
    }

    let input_s = Path::new(&args[1]);

    let source = std::fs::read_to_string(input_s).unwrap_or_else(|e| {
        eprintln!("[ ERROR ] :: [assembler] failed to read {:?}: {}", input_s, e);
        process::exit(1);
    });

    if let Err(e) = validate_supported_intel_subset(&source) {
        eprintln!("[ ERROR ] :: [assembler] parse/encode template error: {}", e);
        process::exit(1);
    }
}


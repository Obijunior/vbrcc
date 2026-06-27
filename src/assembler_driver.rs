use std::path::Path;
use std::process::Command;

pub enum LinkerMode {
    CustomPe,   // default: custom assembler emits full PE
    Gcc,        // --gcc: gcc assembles + links
    LldLink,    // --lld-link: custom assembler emits .obj, lld-link links
}

pub fn assemble_and_link(asm_path: &Path, bin_path: &Path, linker: LinkerMode) -> Result<(), String> {
    match linker {
        LinkerMode::CustomPe => {
            run_assembler(asm_path, bin_path)?;
        }
        LinkerMode::Gcc => {
            assemble_and_link_with_gcc(asm_path, bin_path)?;
        }
        LinkerMode::LldLink => {
            print!("Not yet implemented: LldLink mode");
        }
    }
    Ok(())
}

fn run_assembler(asm_path: &Path, out_path: &Path) -> Result<(), String> {
    let status = Command::new("cargo")
        .args([
            "run",
            "--manifest-path",
            "src/assembler/Cargo.toml",
            "--",
            asm_path
                .to_str()
                .ok_or_else(|| "asm path is not valid UTF-8".to_string())?,
            out_path
                .to_str()
                .ok_or_else(|| "output path is not valid UTF-8".to_string())?,
        ])
        .status()
        .map_err(|e| format!("Failed to run assembler: {}", e))?;

    if !status.success() {
        return Err(format!("assembler failed with status: {}", status));
    }
    Ok(())
}

fn assemble_and_link_with_gcc(asm_path: &Path, bin_path: &Path) -> Result<(), String> {
    let status = Command::new("gcc")
        .args([
            asm_path
                .to_str()
                .ok_or_else(|| "asm path is not valid UTF-8".to_string())?,
            "-lmsvcrt", // link against msvcrt for Windows (provides printf, etc.)
            "-o",
            bin_path
                .to_str()
                .ok_or_else(|| "bin path is not valid UTF-8".to_string())?,
        ])
        .status()
        .map_err(|e| format!("Failed to run gcc: {}", e))?;

    if !status.success() {
        return Err(format!("gcc failed with status: {}", status));
    }
    Ok(())
}

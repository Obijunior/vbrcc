use std::path::Path;
use std::process::Command;

pub fn assemble_and_link(asm_path: &Path, bin_path: &Path, use_gcc: bool) -> Result<(), String> {
    if use_gcc {
        assemble_and_link_with_gcc(asm_path, bin_path)?;
    } else {
        let obj_path = bin_path.with_extension("o");
        run_assembler(asm_path, &obj_path)?;
        link_object(&obj_path, bin_path)?;
    }
    Ok(())
}

fn run_assembler(asm_path: &Path, obj_path: &Path) -> Result<(), String> {
    let status = Command::new("cargo")
        .args([
            "run",
            "--manifest-path",
            "src/assembler/Cargo.toml",
            "--",
            asm_path
                .to_str()
                .ok_or_else(|| "asm path is not valid UTF-8".to_string())?,
            obj_path
                .to_str()
                .ok_or_else(|| "obj path is not valid UTF-8".to_string())?,
        ])
        .status()
        .map_err(|e| format!("Failed to run assembler: {}", e))?;

    if !status.success() {
        return Err(format!("assembler failed with status: {}", status));
    }
    Ok(())
}

fn link_object(obj_path: &Path, bin_path: &Path) -> Result<(), String> {
    let status = Command::new("gcc")
        .args([
            obj_path
                .to_str()
                .ok_or_else(|| "obj path is not valid UTF-8".to_string())?,
            "-o",
            bin_path
                .to_str()
                .ok_or_else(|| "bin path is not valid UTF-8".to_string())?,
            "-mconsole", // Link as a console application (no GUI subsystem)
            "-nostartfiles", // Don't link the standard startup files (we provide our own entry point)
        ])
        .status()
        .map_err(|e| format!("Failed to link with gcc: {}", e))?;

    if !status.success() {
        return Err(format!("gcc link failed with status: {}", status));
    }
    Ok(())
}

fn assemble_and_link_with_gcc(asm_path: &Path, bin_path: &Path) -> Result<(), String> {
    let status = Command::new("gcc")
        .args([
            asm_path
                .to_str()
                .ok_or_else(|| "asm path is not valid UTF-8".to_string())?,
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

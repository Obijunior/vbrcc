use std::path::Path;
use std::process::Command;

fn find_windows_sdk_lib() -> Result<String, String> {
    let kits_root = r"C:\Program Files (x86)\Windows Kits\10\Lib";
    let mut versions: Vec<String> = std::fs::read_dir(kits_root)
        .map_err(|_| "Windows SDK not found".to_string())?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    versions.sort();
    versions.last()
        .map(|v| format!("{kits_root}\\{v}"))
        .ok_or_else(|| "No Windows SDK versions found".to_string())
}

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
            assemble_and_link_with_lld(asm_path, bin_path)?;
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

fn assemble_and_link_with_lld(asm_path: &Path, bin_path: &Path) -> Result<(), String> {
    let obj_path = asm_path.with_extension("obj");

    // Step 1: custom assembler → .obj
    let status = Command::new("cargo")
        .args(["run", "--manifest-path", "src/assembler/Cargo.toml", "--",
               asm_path.to_str().unwrap(),
               obj_path.to_str().unwrap(),
               "--coff"])
        .status()
        .map_err(|e| format!("Failed to run assembler: {}", e))?;
    if !status.success() {
        return Err("assembler (COFF mode) failed".into());
    }

    // Step 2: if assembler wrote a .def (has external symbols), generate import lib
    let def_path = obj_path.with_extension("def");
    let implib_path = obj_path.with_extension("lib");
    let has_externals = def_path.exists();

    if has_externals {
        let status = Command::new("llvm-dlltool")
            .args([
                "-m", "i386:x86-64",
                "-d", def_path.to_str().unwrap(),
                "-l", implib_path.to_str().unwrap(),
                "-D", "msvcrt.dll",
            ])
            .status()
            .map_err(|e| format!("Failed to run llvm-dlltool: {}", e))?;
        if !status.success() {
            return Err("llvm-dlltool failed".into());
        }
    }

    // Step 3: lld-link → .exe
    let sdk = find_windows_sdk_lib()?;
    let mut lld_args = vec![
        format!("/entry:main"),
        "/subsystem:console".to_string(),
        format!("/out:{}", bin_path.to_str().unwrap()),
        obj_path.to_str().unwrap().to_string(),
        "kernel32.lib".to_string(),
        format!("/libpath:{}\\ucrt\\x64", sdk),
        format!("/libpath:{}\\um\\x64", sdk),
    ];
    if has_externals {
        lld_args.push(implib_path.to_str().unwrap().to_string());
    }
    let status = Command::new("lld-link")
        .args(&lld_args)
        .status()
        .map_err(|e| format!("Failed to run lld-link: {}", e))?;
    if !status.success() {
        return Err("lld-link failed".into());
    }

    Ok(())
}

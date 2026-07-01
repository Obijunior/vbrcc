use std::path::Path;
use std::process::Command;
use crate::assembler;

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
            assemble_to_pe(asm_path, bin_path)?;
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

fn assemble_to_pe(asm_path: &Path, out_path: &Path) -> Result<(), String> {
    let source = std::fs::read_to_string(asm_path)
        .map_err(|e| format!("Failed to read {:?}: {}", asm_path, e))?;

    let (text, data, idata) = assembler::assemble(&source)?;
    let pe = assembler::pe::create_pe_wrapper(&text, &data, &idata);

    std::fs::write(out_path, pe)
        .map_err(|e| format!("Failed to write {:?}: {}", out_path, e))?;

    println!("[ SUCCESS ] :: Created Windows Executable: {:?}", out_path);
    println!("  - .text size: {} bytes", text.len());
    println!("  - .data size: {} bytes", data.len());
    println!("  - .idata size: {} bytes", idata.len());
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

    // Step 1: custom assembler -> COFF .obj
    let source = std::fs::read_to_string(asm_path)
        .map_err(|e| format!("Failed to read {:?}: {}", asm_path, e))?;

    let result = assembler::assemble_to_obj(&source)
        .map_err(|e| format!("assembler (COFF mode) failed: {}", e))?;

    let obj_bytes = assembler::coff::create_coff_obj(&result);
    std::fs::write(&obj_path, obj_bytes)
        .map_err(|e| format!("Failed to write .obj: {}", e))?;

    // Step 1b: write .def if there are external symbols
    let externals: Vec<&str> = result.symbols.iter()
        .filter(|s| s.section.is_none())
        .map(|s| s.name.as_str())
        .collect();
    let has_externals = !externals.is_empty();

    let def_path = obj_path.with_extension("def");
    let implib_path = obj_path.with_extension("lib");

    if has_externals {
        let mut def = String::from("LIBRARY msvcrt.dll\nEXPORTS\n");
        for name in &externals {
            def.push_str(&format!("    {}\n", name));
        }
        std::fs::write(&def_path, &def)
            .map_err(|e| format!("Failed to write .def: {}", e))?;
    }

    println!("[ SUCCESS ] :: Created COFF object: {:?}", obj_path);

    // Step 2: if assembler wrote a .def (has external symbols), generate import lib
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

    // Step 3: lld-link -> .exe
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

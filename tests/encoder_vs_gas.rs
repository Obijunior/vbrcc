//! Checks the encoder against GNU `as`, one instruction at a time.
//!
//! The test builds every instruction form for all 16 registers and several
//! displacements. It assembles the list with `as`, reads the bytes back with
//! `objdump`, and compares each instruction with `encoder::encode`. It also checks
//! that `encoded_len` agrees with the bytes, because pass one lays out labels from it.
//!
//! The test skips when `as` or `objdump` is not on the `PATH`.
//!
//! Label forms (`jmp`, `j<cc>`, `call`, `lea [rip + x]`) are not here. Their bytes
//! depend on the layout. Unit tests in `encoder.rs` cover them.

use std::process::Command;

use vbrcc::assembler::encoder::{encode, encoded_len};
use vbrcc::assembler::instruction::{parse_intel_line, AsmLine};

const R64: [&str; 16] = [
    "rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi",
    "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15",
];
const R32: [&str; 16] = [
    "eax", "ecx", "edx", "ebx", "esp", "ebp", "esi", "edi",
    "r8d", "r9d", "r10d", "r11d", "r12d", "r13d", "r14d", "r15d",
];
const R8: [&str; 16] = [
    "al", "cl", "dl", "bl", "spl", "bpl", "sil", "dil",
    "r8b", "r9b", "r10b", "r11b", "r12b", "r13b", "r14b", "r15b",
];
/// The only 8-bit registers the assembler accepts.
const R8_LOW: [&str; 4] = ["al", "cl", "dl", "bl"];
/// 0, then disp8 both ways, then disp32 both ways.
const DISPS: [i32; 5] = [0, 8, -8, 300, -300];
/// Outside the imm8 range, so `as` also picks the imm32 form.
const IMMS: [i32; 2] = [300, -300];

fn mem(base: &str, disp: i32) -> String {
    match disp {
        0 => format!("[{base}]"),
        d if d < 0 => format!("[{base} - {}]", -d),
        d => format!("[{base} + {d}]"),
    }
}

/// Every case as (our syntax, GNU syntax). They differ only for the 10-byte
/// `mov r64, imm64`, which GNU spells `movabs`.
fn cases() -> Vec<(String, String)> {
    let mut v = Vec::new();

    for op in ["mov", "add", "sub", "and", "or", "xor", "cmp", "imul"] {
        for d in R64 {
            for s in R64 {
                same(&mut v, format!("{op} {d}, {s}"));
            }
        }
    }
    for op in ["add", "sub", "and", "or", "xor", "cmp", "imul"] {
        for d in R64 {
            // For `rax`, `as` picks the short accumulator form (`48 05 id`). Ours
            // (`48 81 /0 id`) is also valid, so the bytes cannot match. `imul` has
            // no accumulator form.
            if d == "rax" && op != "imul" {
                continue;
            }
            for i in IMMS {
                same(&mut v, format!("{op} {d}, {i}"));
            }
        }
    }
    for op in ["neg", "not", "idiv", "push", "pop"] {
        for r in R64 {
            same(&mut v, format!("{op} {r}"));
        }
    }
    for op in ["shl", "sar"] {
        for r in R64 {
            same(&mut v, format!("{op} {r}, cl"));
        }
    }
    for cc in ["sete", "setne", "setl", "setle", "setg", "setge"] {
        for r in R8_LOW {
            same(&mut v, format!("{cc} {r}"));
        }
    }
    for d in R64 {
        for s in R8_LOW {
            same(&mut v, format!("movzx {d}, {s}"));
        }
    }
    for b in R64 {
        for disp in DISPS {
            let m = mem(b, disp);
            for (i, r) in R64.iter().enumerate() {
                same(&mut v, format!("mov {m}, {r}"));
                same(&mut v, format!("mov {r}, {m}"));
                same(&mut v, format!("lea {r}, {m}"));
                same(&mut v, format!("movsx {r}, byte ptr {m}"));
                same(&mut v, format!("movsx {r}, word ptr {m}"));
                same(&mut v, format!("movsxd {r}, dword ptr {m}"));
                same(&mut v, format!("mov dword ptr {m}, {}", R32[i]));
                same(&mut v, format!("mov byte ptr {m}, {}", R8[i]));
            }
        }
    }
    for r in R64 {
        v.push((format!("mov {r}, 1234567890123"), format!("movabs {r}, 1234567890123")));
    }
    for s in ["ret", "cqo", "syscall"] {
        v.push((s.to_string(), s.to_string()));
    }
    v
}

fn same(v: &mut Vec<(String, String)>, s: String) {
    v.push((s.clone(), s));
}

/// Assemble `lines` with GNU `as`. Returns each instruction's bytes, in order.
fn gas_bytes(lines: &[&str]) -> Option<Vec<Vec<u8>>> {
    let tools_ok = Command::new("as").arg("--version").output().is_ok()
        && Command::new("objdump").arg("--version").output().is_ok();
    if !tools_ok {
        eprintln!("skipping: GNU as or objdump not on PATH");
        return None;
    }
    let dir = std::env::temp_dir();
    let src = dir.join("vbrcc_encoder_vs_gas.s");
    let obj = dir.join("vbrcc_encoder_vs_gas.o");
    std::fs::write(&src, format!(".intel_syntax noprefix\n{}\n", lines.join("\n"))).unwrap();

    let out = Command::new("as").arg(&src).arg("-o").arg(&obj).output().unwrap();
    assert!(out.status.success(), "as failed:\n{}", String::from_utf8_lossy(&out.stderr));
    let dump = Command::new("objdump")
        .args(["-d", "-M", "intel", "--insn-width=16"])
        .arg(&obj)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&dump.stdout);

    // An instruction line is "  addr:\tbytes\tmnemonic". `as` pads the end with `nop`.
    let bytes: Vec<Vec<u8>> = text
        .lines()
        .filter_map(|l| {
            let mut cols = l.splitn(3, '\t');
            let addr = cols.next()?;
            if !addr.trim_end().ends_with(':') {
                return None;
            }
            let hex = cols.next()?;
            Some(hex.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect())
        })
        .take(lines.len())
        .collect();
    assert_eq!(bytes.len(), lines.len(), "objdump returned too few instructions");
    Some(bytes)
}

#[test]
fn every_instruction_form_matches_gnu_as() {
    let cases = cases();
    let gas_lines: Vec<&str> = cases.iter().map(|(_, g)| g.as_str()).collect();
    let Some(expected) = gas_bytes(&gas_lines) else { return };

    let mut failures = Vec::new();
    for ((ours, gas), want) in cases.iter().zip(expected) {
        let instr = match parse_intel_line(ours) {
            Ok(AsmLine::Instruction(i)) => i,
            other => {
                failures.push(format!("{ours}: does not parse ({other:?})"));
                continue;
            }
        };
        let got = encode(&instr);
        if got != want {
            failures.push(format!("{ours}: {} (as: {} for `{gas}`)", hex(&got), hex(&want)));
        } else if encoded_len(&instr) != got.len() {
            failures.push(format!("{ours}: encoded_len {} but {} bytes", encoded_len(&instr), got.len()));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} forms differ from GNU as:\n{}",
        failures.len(),
        cases.len(),
        failures.iter().take(40).cloned().collect::<Vec<_>>().join("\n")
    );
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

// Windows PE (Portable Executable) wrapper for raw machine code.
// This emits a minimal but valid PE32+ executable with optional .idata.

fn align_up(value: u32, align: u32) -> u32 {
    (value + align - 1) & !(align - 1)
}

fn write_u16(buf: &mut [u8], offset: usize, value: u16) {
    buf[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn write_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn write_u64(buf: &mut [u8], offset: usize, value: u64) {
    buf[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[derive(Clone, Copy)]
struct SectionDef<'a> {
    name: &'a [u8; 8],
    data: &'a [u8],
    characteristics: u32,
}

pub fn create_pe_wrapper(text_code: &[u8], data_content: &[u8], idata_content: &[u8]) -> Vec<u8> {
    const IMAGE_BASE: u64 = 0x400000;
    const SECTION_ALIGNMENT: u32 = 0x1000;
    const FILE_ALIGNMENT: u32 = 0x200;

    let mut sections: Vec<SectionDef<'_>> = Vec::new();
    sections.push(SectionDef {
        name: b".text\0\0\0",
        data: text_code,
        characteristics: 0x60000020, // Code, Execute, Read
    });
    sections.push(SectionDef {
        name: b".data\0\0\0",
        data: data_content,
        characteristics: 0xC0000040, // Data, Read, Write
    });
    if !idata_content.is_empty() {
        sections.push(SectionDef {
            name: b".idata\0\0",
            data: idata_content,
            characteristics: 0xC0000040, // Data, Read, Write
        });
    }

    let num_sections = sections.len() as u16;
    let dos_header_size = 0x80u32;
    let pe_sig_size = 4u32;
    let coff_header_size = 20u32;
    let optional_header_size = 0xF0u32;
    let section_table_size = 40u32 * num_sections as u32;

    let headers_unaligned = dos_header_size + pe_sig_size + coff_header_size + optional_header_size + section_table_size;
    let size_of_headers = align_up(headers_unaligned, FILE_ALIGNMENT);

    // Compute RVAs and raw pointers
    let mut section_rva = SECTION_ALIGNMENT;
    let mut section_raw = size_of_headers;

    let mut section_layout: Vec<(u32, u32, u32, u32)> = Vec::new();
    for s in &sections {
        let virtual_size = s.data.len() as u32;
        let raw_size = align_up(virtual_size, FILE_ALIGNMENT);
        section_layout.push((section_rva, section_raw, virtual_size, raw_size));
        section_rva += align_up(virtual_size, SECTION_ALIGNMENT);
        section_raw += raw_size;
    }

    let size_of_code = align_up(text_code.len() as u32, FILE_ALIGNMENT);
    let size_of_init_data = align_up(data_content.len() as u32, FILE_ALIGNMENT)
        + if !idata_content.is_empty() { align_up(idata_content.len() as u32, FILE_ALIGNMENT) } else { 0 };

    let size_of_image = align_up(section_rva, SECTION_ALIGNMENT);

    let base_of_code = SECTION_ALIGNMENT;
    let address_of_entry_point = SECTION_ALIGNMENT;

    let mut pe = Vec::new();

    // --- 1. DOS HEADER ---
    pe.extend_from_slice(b"MZ");
    pe.resize(60, 0);
    pe.extend_from_slice(&(dos_header_size as u32).to_le_bytes()); // e_lfanew
    pe.resize(dos_header_size as usize, 0);

    // --- 2. PE SIGNATURE ---
    pe.extend_from_slice(b"PE\0\0");

    // --- 3. COFF FILE HEADER ---
    pe.extend_from_slice(&0x8664u16.to_le_bytes()); // Machine: x64
    pe.extend_from_slice(&num_sections.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes()); // TimeDateStamp
    pe.extend_from_slice(&0u32.to_le_bytes()); // PtrToSymbolTable
    pe.extend_from_slice(&0u32.to_le_bytes()); // NumSymbols
    pe.extend_from_slice(&(optional_header_size as u16).to_le_bytes());
    pe.extend_from_slice(&0x0022u16.to_le_bytes()); // Characteristics (EXECUTABLE_IMAGE | LARGE_ADDRESS_AWARE)

    // --- 4. OPTIONAL HEADER (PE32+) ---
    let opt_start = pe.len();
    pe.extend_from_slice(&0x020bu16.to_le_bytes()); // Magic PE32+
    pe.push(0); // MajorLinkerVersion
    pe.push(0); // MinorLinkerVersion
    pe.extend_from_slice(&size_of_code.to_le_bytes());
    pe.extend_from_slice(&size_of_init_data.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes()); // SizeOfUninitializedData
    pe.extend_from_slice(&address_of_entry_point.to_le_bytes());
    pe.extend_from_slice(&base_of_code.to_le_bytes());
    pe.extend_from_slice(&IMAGE_BASE.to_le_bytes());
    pe.extend_from_slice(&SECTION_ALIGNMENT.to_le_bytes());
    pe.extend_from_slice(&FILE_ALIGNMENT.to_le_bytes());
    pe.extend_from_slice(&6u16.to_le_bytes()); // MajorOSVersion
    pe.extend_from_slice(&0u16.to_le_bytes()); // MinorOSVersion
    pe.extend_from_slice(&0u16.to_le_bytes()); // MajorImageVersion
    pe.extend_from_slice(&0u16.to_le_bytes()); // MinorImageVersion
    pe.extend_from_slice(&6u16.to_le_bytes()); // MajorSubsystemVersion
    pe.extend_from_slice(&0u16.to_le_bytes()); // MinorSubsystemVersion
    pe.extend_from_slice(&0u32.to_le_bytes()); // Win32VersionValue
    pe.extend_from_slice(&size_of_image.to_le_bytes());
    pe.extend_from_slice(&size_of_headers.to_le_bytes());
    pe.extend_from_slice(&0u32.to_le_bytes()); // CheckSum
    pe.extend_from_slice(&3u16.to_le_bytes()); // Subsystem: Console
    pe.extend_from_slice(&(0x0140u16).to_le_bytes()); // DllCharacteristics: DYNAMIC_BASE | NX_COMPAT | TERMINAL_SERVER_AWARE
    pe.extend_from_slice(&0x0010_0000u64.to_le_bytes()); // SizeOfStackReserve (1MB)
    pe.extend_from_slice(&0x0000_1000u64.to_le_bytes()); // SizeOfStackCommit (4KB)
    pe.extend_from_slice(&0x0010_0000u64.to_le_bytes()); // SizeOfHeapReserve (1MB)
    pe.extend_from_slice(&0x0000_1000u64.to_le_bytes()); // SizeOfHeapCommit (4KB)
    pe.extend_from_slice(&0u32.to_le_bytes()); // LoaderFlags
    pe.extend_from_slice(&16u32.to_le_bytes()); // NumberOfRvaAndSizes
    let data_dir_offset = pe.len();
    pe.resize(pe.len() + 16 * 8, 0);

    // Pad optional header to 0xF0
    let opt_len = pe.len() - opt_start;
    if opt_len < optional_header_size as usize {
        pe.resize(opt_start + optional_header_size as usize, 0);
    }

    // --- 5. SECTION TABLE ---
    for (i, s) in sections.iter().enumerate() {
        let (rva, raw_ptr, virtual_size, raw_size) = section_layout[i];
        pe.extend_from_slice(s.name);
        pe.extend_from_slice(&virtual_size.to_le_bytes());
        pe.extend_from_slice(&rva.to_le_bytes());
        pe.extend_from_slice(&raw_size.to_le_bytes());
        pe.extend_from_slice(&raw_ptr.to_le_bytes());
        pe.extend_from_slice(&0u32.to_le_bytes()); // PtrToRelocations
        pe.extend_from_slice(&0u32.to_le_bytes()); // PtrToLinenumbers
        pe.extend_from_slice(&0u16.to_le_bytes()); // NumRelocations
        pe.extend_from_slice(&0u16.to_le_bytes()); // NumLinenumbers
        pe.extend_from_slice(&s.characteristics.to_le_bytes());
    }

    // Patch Import Directory entry if .idata exists
    if let Some(idata_index) = sections.iter().position(|s| s.name == b".idata\0\0") {
        let (idata_rva, _raw_ptr, idata_vs, _raw_size) = section_layout[idata_index];
        let import_dir_offset = data_dir_offset + (1 * 8); // IMAGE_DIRECTORY_ENTRY_IMPORT
        write_u32(&mut pe, import_dir_offset, idata_rva);
        write_u32(&mut pe, import_dir_offset + 4, idata_vs);
    }

    // --- 6. WRITE SECTION DATA ---
    let header_len = pe.len() as u32;
    if header_len < size_of_headers {
        pe.resize(size_of_headers as usize, 0);
    }

    for (i, s) in sections.iter().enumerate() {
        let (_rva, raw_ptr, _virtual_size, raw_size) = section_layout[i];
        let end = raw_ptr + raw_size;
        if pe.len() < end as usize {
            pe.resize(end as usize, 0);
        }
        let data_end = raw_ptr as usize + s.data.len();
        pe[raw_ptr as usize..data_end].copy_from_slice(s.data);
    }

    // Ensure e_lfanew points to PE signature
    write_u32(&mut pe, 0x3C, dos_header_size);
    let _ = write_u16 as fn(&mut [u8], usize, u16);
    let _ = write_u64 as fn(&mut [u8], usize, u64);

    pe
}

// Windows PE (Portable Executable) wrapper for raw machine code.
// This is a minimal implementation to allow the assembler to produce .exe files directly.

fn align_up(value: u32, align: u32) -> u32 {
    (value + align - 1) & !(align - 1)
}

fn write_u32(buf: &mut [u8], offset: usize, value: u32) {
    buf[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub fn create_pe_wrapper(text_code: &[u8], data_content: &[u8], idata_content: &[u8]) -> Vec<u8> {
    let mut pe = Vec::new();

    // --- 1. DOS HEADER ---
    pe.extend_from_slice(b"MZ"); 
    pe.resize(60, 0);            
    pe.extend_from_slice(&0x80u32.to_le_bytes()); // Offset to PE Header
    pe.resize(128, 0);           

    // --- 2. PE SIGNATURE ---
    pe.extend_from_slice(b"PE\0\0");

    // --- 3. COFF FILE HEADER ---
    pe.extend_from_slice(&0x8664u16.to_le_bytes()); // Machine: x64
    pe.extend_from_slice(&3u16.to_le_bytes());      // Sections: .text, .data, .idata
    pe.extend_from_slice(&0u32.to_le_bytes());      
    pe.extend_from_slice(&0u32.to_le_bytes());      
    pe.extend_from_slice(&0u32.to_le_bytes());      
    pe.extend_from_slice(&0xf0u16.to_le_bytes());   // SizeOfOptionalHeader
    pe.extend_from_slice(&0x0022u16.to_le_bytes()); // Characteristics

    // --- 4. OPTIONAL HEADER (PE32+) ---
    pe.extend_from_slice(&0x020bu16.to_le_bytes()); 
    pe.extend_from_slice(&[0, 0]);                  
    pe.extend_from_slice(&align_up(text_code.len() as u32, 0x1000).to_le_bytes()); 
    pe.extend_from_slice(&(data_content.len() as u32 + idata_content.len() as u32).to_le_bytes()); 
    pe.extend_from_slice(&0u32.to_le_bytes());      
    pe.extend_from_slice(&0x1000u32.to_le_bytes()); // EntryPoint (Start of .text)
    pe.extend_from_slice(&0x1000u32.to_le_bytes()); 

    pe.extend_from_slice(&0x400000u64.to_le_bytes()); // ImageBase
    pe.extend_from_slice(&0x1000u32.to_le_bytes());   // SectionAlignment (4KB)
    pe.extend_from_slice(&0x200u32.to_le_bytes());    // FileAlignment (512B)
    pe.extend_from_slice(&[6, 0, 0, 0, 0, 0, 0, 0, 6, 0, 0, 0]); // Versions
    pe.extend_from_slice(&0u32.to_le_bytes());        

    // Image Size: Headers (0x1000) + .text + .data + .idata (aligned)
    let text_aligned = align_up(text_code.len() as u32, 0x1000);
    let data_aligned = align_up(data_content.len() as u32, 0x1000);
    let idata_aligned = align_up(idata_content.len() as u32, 0x1000);
    let size_of_image = 0x1000 + text_aligned + data_aligned + idata_aligned;
    
    pe.extend_from_slice(&size_of_image.to_le_bytes());
    pe.extend_from_slice(&0x400u32.to_le_bytes());    // SizeOfHeaders
    pe.extend_from_slice(&0u32.to_le_bytes());        
    pe.extend_from_slice(&3u16.to_le_bytes());        // Subsystem: Console
    pe.extend_from_slice(&0x8140u16.to_le_bytes());   
    pe.resize(pe.len() + 32, 0);                      // Stack/Heap sizes
    pe.extend_from_slice(&0u32.to_le_bytes());        
    pe.extend_from_slice(&16u32.to_le_bytes());       // DataDirectories
    let data_dir_offset = pe.len();
    pe.resize(pe.len() + (16 * 8), 0);

    // --- 5. SECTION TABLE ---
    // .text
    pe.extend_from_slice(b".text\0\0\0");
    pe.extend_from_slice(&(text_code.len() as u32).to_le_bytes());
    pe.extend_from_slice(&0x1000u32.to_le_bytes());   // RVA
    pe.extend_from_slice(&align_up(text_code.len() as u32, 0x200).to_le_bytes());
    pe.extend_from_slice(&0x400u32.to_le_bytes());    // PointerToRawData
    pe.resize(pe.len() + 12, 0);
    pe.extend_from_slice(&0x60000020u32.to_le_bytes()); // Code, Execute, Read

    // .data
    pe.extend_from_slice(b".data\0\0\0");
    pe.extend_from_slice(&(data_content.len() as u32).to_le_bytes());
    pe.extend_from_slice(&(0x1000 + text_aligned).to_le_bytes()); // RVA
    pe.extend_from_slice(&align_up(data_content.len() as u32, 0x200).to_le_bytes());
    let text_raw_size = align_up(text_code.len() as u32, 0x200);
    let data_raw_ptr = 0x400 + text_raw_size;
    pe.extend_from_slice(&data_raw_ptr.to_le_bytes()); // PointerToRawData
    pe.resize(pe.len() + 12, 0);
    pe.extend_from_slice(&0xC0000040u32.to_le_bytes()); // Data, Read, Write

    // .idata
    pe.extend_from_slice(b".idata\0\0");
    pe.extend_from_slice(&(idata_content.len() as u32).to_le_bytes());
    pe.extend_from_slice(&(0x1000 + text_aligned + data_aligned).to_le_bytes()); // RVA
    pe.extend_from_slice(&align_up(idata_content.len() as u32, 0x200).to_le_bytes());
    let data_raw_size = align_up(data_content.len() as u32, 0x200);
    let idata_raw_ptr = data_raw_ptr + data_raw_size;
    pe.extend_from_slice(&idata_raw_ptr.to_le_bytes()); // PointerToRawData
    pe.resize(pe.len() + 12, 0);
    pe.extend_from_slice(&0xC0000040u32.to_le_bytes()); // Data, Read, Write

    // --- 6. WRITE DATA ---
    pe.resize(0x400, 0); // Align to first section
    pe.extend_from_slice(text_code);
    pe.resize(data_raw_ptr as usize, 0); // Align to second section
    pe.extend_from_slice(data_content);
    pe.resize(idata_raw_ptr as usize, 0); // Align to third section
    pe.extend_from_slice(idata_content);

    // Patch Import Directory entry if .idata exists
    if !idata_content.is_empty() {
        let idata_rva = 0x1000 + text_aligned + data_aligned;
        let import_dir_offset = data_dir_offset + (1 * 8); // IMAGE_DIRECTORY_ENTRY_IMPORT
        write_u32(&mut pe, import_dir_offset, idata_rva);
        write_u32(&mut pe, import_dir_offset + 4, idata_content.len() as u32);
    }

    pe
}

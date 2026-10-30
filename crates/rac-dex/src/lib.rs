use byteorder::{LittleEndian, WriteBytesExt};
use sha1::{Digest, Sha1};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DexError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Generates a valid minimal DEX file (format 035).
pub fn generate_empty_dex() -> Result<Vec<u8>, DexError> {
    // A minimal valid DEX has:
    // Header (112 bytes = 0x70)
    // Map list (4 bytes size + items)
    let header_size = 0x70u32;
    let map_off = 0x70u32;

    // Map list items:
    // 1. HEADER_ITEM (type 0x0000, size 1, offset 0)
    // 2. MAP_LIST_ITEM (type 0x1000, size 1, offset map_off)
    let map_items_count = 2u32;
    let map_size_bytes = 4 + map_items_count * 12;
    let total_file_size = header_size + map_size_bytes;

    let mut buf = vec![0u8; total_file_size as usize];

    // Magic: "dex\n035\0"
    buf[0..8].copy_from_slice(b"dex\n035\0");

    // Header values:
    let mut cursor = std::io::Cursor::new(&mut buf[32..]);
    cursor.write_u32::<LittleEndian>(total_file_size)?; // file_size
    cursor.write_u32::<LittleEndian>(header_size)?;     // header_size
    cursor.write_u32::<LittleEndian>(0x12345678)?;     // endian_tag (LITTLE_ENDIAN)
    cursor.write_u32::<LittleEndian>(0)?;              // link_size
    cursor.write_u32::<LittleEndian>(0)?;              // link_off
    cursor.write_u32::<LittleEndian>(map_off)?;        // map_off
    cursor.write_u32::<LittleEndian>(0)?;              // string_ids_size
    cursor.write_u32::<LittleEndian>(0)?;              // string_ids_off
    cursor.write_u32::<LittleEndian>(0)?;              // type_ids_size
    cursor.write_u32::<LittleEndian>(0)?;              // type_ids_off
    cursor.write_u32::<LittleEndian>(0)?;              // proto_ids_size
    cursor.write_u32::<LittleEndian>(0)?;              // proto_ids_off
    cursor.write_u32::<LittleEndian>(0)?;              // field_ids_size
    cursor.write_u32::<LittleEndian>(0)?;              // field_ids_off
    cursor.write_u32::<LittleEndian>(0)?;              // method_ids_size
    cursor.write_u32::<LittleEndian>(0)?;              // method_ids_off
    cursor.write_u32::<LittleEndian>(0)?;              // class_defs_size
    cursor.write_u32::<LittleEndian>(0)?;              // class_defs_off
    cursor.write_u32::<LittleEndian>(map_size_bytes)?; // data_size
    cursor.write_u32::<LittleEndian>(map_off)?;        // data_off

    // Write Map list at map_off
    let mut map_cursor = std::io::Cursor::new(&mut buf[map_off as usize..]);
    map_cursor.write_u32::<LittleEndian>(map_items_count)?;

    // Item 1: TYPE_HEADER_ITEM = 0x0000
    map_cursor.write_u16::<LittleEndian>(0x0000)?;
    map_cursor.write_u16::<LittleEndian>(0)?; // unused
    map_cursor.write_u32::<LittleEndian>(1)?; // size = 1
    map_cursor.write_u32::<LittleEndian>(0)?; // offset = 0

    // Item 2: TYPE_MAP_LIST = 0x1000
    map_cursor.write_u16::<LittleEndian>(0x1000)?;
    map_cursor.write_u16::<LittleEndian>(0)?; // unused
    map_cursor.write_u32::<LittleEndian>(1)?; // size = 1
    map_cursor.write_u32::<LittleEndian>(map_off)?; // offset = map_off

    // Compute SHA-1 signature over bytes 32..end
    let mut sha1_hasher = Sha1::new();
    sha1_hasher.update(&buf[32..]);
    let sig = sha1_hasher.finalize();
    buf[12..32].copy_from_slice(&sig);

    // Compute Adler32 checksum over bytes 12..end
    let adler = adler2::adler32_slice(&buf[12..]);
    buf[8..12].copy_from_slice(&adler.to_le_bytes());

    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dex_structure() {
        let dex = generate_empty_dex().unwrap();
        assert_eq!(&dex[0..8], b"dex\n035\0");
        assert!(dex.len() >= 112);
    }
}

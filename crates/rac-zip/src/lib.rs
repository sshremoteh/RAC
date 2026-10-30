use std::io::{self, Seek, Write};
use flate2::write::DeflateEncoder;
use flate2::Compression;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ZipError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Invalid ZIP format: {0}")]
    Format(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompressionMethod {
    Stored = 0,
    Deflated = 8,
}

pub struct ZipEntryMeta {
    pub name: String,
    pub compression_method: CompressionMethod,
    pub crc32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub local_header_offset: u64,
}

pub struct ZipWriter<W: Write + Seek> {
    writer: W,
    entries: Vec<ZipEntryMeta>,
}

impl<W: Write + Seek> ZipWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            entries: Vec::new(),
        }
    }

    /// Add a file entry to the ZIP archive.
    /// If `alignment` is Some(a) where a > 1 and `method == Stored`, the uncompressed file
    /// data will be aligned to an offset divisible by `a` (standard Android 4-byte or 16KB alignment).
    pub fn add_file(
        &mut self,
        name: &str,
        data: &[u8],
        method: CompressionMethod,
        alignment: Option<u32>,
    ) -> Result<(), ZipError> {
        let uncompressed_size = data.len() as u32;
        let crc = crc32fast::hash(data);

        let (compressed_data, final_method) = match method {
            CompressionMethod::Stored => (None, CompressionMethod::Stored),
            CompressionMethod::Deflated => {
                let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
                encoder.write_all(data)?;
                let compressed = encoder.finish()?;
                // If compressed data is not smaller, fall back to Stored
                if compressed.len() < data.len() {
                    (Some(compressed), CompressionMethod::Deflated)
                } else {
                    (None, CompressionMethod::Stored)
                }
            }
        };

        let file_bytes = match &compressed_data {
            Some(cd) => cd.as_slice(),
            None => data,
        };
        let compressed_size = file_bytes.len() as u32;

        let local_header_offset = self.writer.stream_position()?;
        let name_bytes = name.as_bytes();
        let name_len = name_bytes.len() as u16;

        let mut extra_field = Vec::new();
        if final_method == CompressionMethod::Stored {
            let align = alignment.unwrap_or(4);
            if align > 1 {
                let base_data_offset = local_header_offset + 30 + name_len as u64;
                let rem = (base_data_offset % (align as u64)) as u32;
                if rem != 0 {
                    let pad_len = align - rem;
                    extra_field.resize(pad_len as usize, 0u8);
                }
            }
        }

        let extra_len = extra_field.len() as u16;

        // Local file header:
        // 0..4: 0x04034b50
        // 4..6: version needed to extract (20 = 2.0)
        // 6..8: general purpose bit flag (0x0800 = UTF-8 filename)
        // 8..10: compression method
        // 10..12: last mod file time
        // 12..14: last mod file date
        // 14..18: crc-32
        // 18..22: compressed size
        // 22..26: uncompressed size
        // 26..28: file name length
        // 28..30: extra field length
        self.writer.write_all(&0x04034b50u32.to_le_bytes())?;
        self.writer.write_all(&20u16.to_le_bytes())?;
        self.writer.write_all(&0x0800u16.to_le_bytes())?; // UTF-8 filename flag
        self.writer.write_all(&(final_method as u16).to_le_bytes())?;
        self.writer.write_all(&0u16.to_le_bytes())?; // time
        self.writer.write_all(&0u16.to_le_bytes())?; // date
        self.writer.write_all(&crc.to_le_bytes())?;
        self.writer.write_all(&compressed_size.to_le_bytes())?;
        self.writer.write_all(&uncompressed_size.to_le_bytes())?;
        self.writer.write_all(&name_len.to_le_bytes())?;
        self.writer.write_all(&extra_len.to_le_bytes())?;
        self.writer.write_all(name_bytes)?;
        if !extra_field.is_empty() {
            self.writer.write_all(&extra_field)?;
        }

        // Verify alignment if requested or defaulted
        let actual_data_offset = self.writer.stream_position()?;
        if final_method == CompressionMethod::Stored {
            let align = alignment.unwrap_or(4);
            if align > 1 {
                debug_assert_eq!(
                    actual_data_offset % (align as u64),
                    0,
                    "Data offset must be aligned to {}",
                    align
                );
            }
        }

        // Write file payload
        self.writer.write_all(file_bytes)?;

        self.entries.push(ZipEntryMeta {
            name: name.to_string(),
            compression_method: final_method,
            crc32: crc,
            compressed_size,
            uncompressed_size,
            local_header_offset,
        });

        Ok(())
    }

    /// Write Central Directory and End of Central Directory record, returning total written size.
    pub fn finish(&mut self) -> Result<u64, ZipError> {
        let cd_offset = self.writer.stream_position()?;
        let num_entries = self.entries.len() as u16;

        for entry in &self.entries {
            let name_bytes = entry.name.as_bytes();
            let name_len = name_bytes.len() as u16;

            // Central directory file header:
            // 0..4: 0x02014b50
            // 4..6: version made by (Unix / 2.0 = 0x0314)
            // 6..8: version needed to extract (20)
            // 8..10: general purpose bit flag (0x0800 = UTF-8)
            // 10..12: compression method
            // 12..14: last mod file time
            // 14..16: last mod file date
            // 16..20: crc-32
            // 20..24: compressed size
            // 24..28: uncompressed size
            // 28..30: file name length
            // 30..32: extra field length (0 in central directory)
            // 32..34: file comment length (0)
            // 34..36: disk number start (0)
            // 36..38: internal file attributes (0)
            // 38..42: external file attributes (regular file: 0x81A40000)
            // 42..46: relative offset of local header
            self.writer.write_all(&0x02014b50u32.to_le_bytes())?;
            self.writer.write_all(&0x0314u16.to_le_bytes())?;
            self.writer.write_all(&20u16.to_le_bytes())?;
            self.writer.write_all(&0x0800u16.to_le_bytes())?;
            self.writer.write_all(&(entry.compression_method as u16).to_le_bytes())?;
            self.writer.write_all(&0u16.to_le_bytes())?;
            self.writer.write_all(&0u16.to_le_bytes())?;
            self.writer.write_all(&entry.crc32.to_le_bytes())?;
            self.writer.write_all(&entry.compressed_size.to_le_bytes())?;
            self.writer.write_all(&entry.uncompressed_size.to_le_bytes())?;
            self.writer.write_all(&name_len.to_le_bytes())?;
            self.writer.write_all(&0u16.to_le_bytes())?; // extra len in CD
            self.writer.write_all(&0u16.to_le_bytes())?; // comment len
            self.writer.write_all(&0u16.to_le_bytes())?; // disk #
            self.writer.write_all(&0u16.to_le_bytes())?; // internal attr
            self.writer.write_all(&0x81a40000u32.to_le_bytes())?; // external attr
            self.writer.write_all(&(entry.local_header_offset as u32).to_le_bytes())?;
            self.writer.write_all(name_bytes)?;
        }

        let cd_end_offset = self.writer.stream_position()?;
        let cd_size = (cd_end_offset - cd_offset) as u32;

        // End of central directory record (EOCD):
        // 0..4: 0x06054b50
        // 4..6: number of this disk (0)
        // 6..8: disk where central directory starts (0)
        // 8..10: total entries in central dir on this disk
        // 10..12: total entries in central dir
        // 12..16: size of central directory
        // 16..20: offset of start of central directory
        // 20..22: comment length (0)
        self.writer.write_all(&0x06054b50u32.to_le_bytes())?;
        self.writer.write_all(&0u16.to_le_bytes())?;
        self.writer.write_all(&0u16.to_le_bytes())?;
        self.writer.write_all(&num_entries.to_le_bytes())?;
        self.writer.write_all(&num_entries.to_le_bytes())?;
        self.writer.write_all(&cd_size.to_le_bytes())?;
        self.writer.write_all(&(cd_offset as u32).to_le_bytes())?;
        self.writer.write_all(&0u16.to_le_bytes())?;

        let total_size = self.writer.stream_position()?;
        self.writer.flush()?;
        Ok(total_size)
    }

    pub fn into_inner(self) -> W {
        self.writer
    }
}

/// Helper to parse EOCD and Central Directory offset from an existing ZIP.
pub struct ZipSections {
    pub cd_offset: u64,
    pub cd_size: u64,
    pub eocd_offset: u64,
}

pub fn find_zip_sections(data: &[u8]) -> Result<ZipSections, ZipError> {
    if data.len() < 22 {
        return Err(ZipError::Format("File too short for ZIP".to_string()));
    }
    // Search for EOCD signature 0x06054b50 from the end
    let max_search = std::cmp::min(data.len(), 65536 + 22);
    let search_start = data.len() - max_search;

    let mut eocd_offset = None;
    for i in (search_start..=(data.len() - 22)).rev() {
        if &data[i..i + 4] == &[0x50, 0x4b, 0x05, 0x06] {
            eocd_offset = Some(i as u64);
            break;
        }
    }

    let eocd_offset = eocd_offset.ok_or_else(|| ZipError::Format("EOCD signature not found".into()))?;
    let eocd_bytes = &data[eocd_offset as usize..];
    let cd_size = u32::from_le_bytes(eocd_bytes[12..16].try_into().unwrap()) as u64;
    let cd_offset = u32::from_le_bytes(eocd_bytes[16..20].try_into().unwrap()) as u64;

    Ok(ZipSections {
        cd_offset,
        cd_size,
        eocd_offset,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_zip_alignment() {
        let mut buffer = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(&mut buffer);

        zip.add_file("uncompressed.txt", b"Hello, uncompressed world!", CompressionMethod::Stored, Some(4)).unwrap();
        zip.add_file("compressed.txt", b"Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.", CompressionMethod::Deflated, None).unwrap();
        zip.add_file("lib/arm64-v8a/libtest.so", b"ELF binary mock bytes", CompressionMethod::Stored, Some(4)).unwrap();

        zip.finish().unwrap();

        let bytes = buffer.into_inner();
        let sections = find_zip_sections(&bytes).unwrap();
        assert!(sections.cd_offset > 0);
        assert!(sections.eocd_offset > sections.cd_offset);
    }
}

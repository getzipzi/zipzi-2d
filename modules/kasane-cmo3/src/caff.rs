//! CAFF framing: big-endian fields, width-dependent XOR and ZIP-wrapped XML.
use crate::Cmo3Error;
use std::io::{Cursor, Write};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

pub(crate) fn encode(
    images: Vec<(String, Vec<u8>)>,
    xml: &[u8],
    key: i32,
) -> Result<Vec<u8>, Cmo3Error> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(
        "contents",
        SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(1)),
    )?;
    zip.write_all(xml)?;
    let zipped = zip.finish()?.into_inner();
    let mut entries: Vec<_> = images.into_iter().map(|(p, b)| (p, "", 16u8, b)).collect();
    entries.push(("main.xml".into(), "main_xml", 33, zipped));
    let mut table = Writer::new(key);
    table.u32(u32::try_from(entries.len()).map_err(|_| Cmo3Error::TooLarge)?);
    let mut patches = Vec::new();
    for (path, tag, compression, bytes) in &entries {
        table.string(path)?;
        table.string(tag)?;
        patches.push(table.0.len());
        table.u64(0);
        table.u32(u32::try_from(bytes.len()).map_err(|_| Cmo3Error::TooLarge)?);
        table.byte(1);
        table.byte(*compression);
        table.0.extend([0; 8]);
    }
    let mut offset = 54usize
        .checked_add(table.0.len())
        .ok_or(Cmo3Error::TooLarge)?;
    for ((_, _, _, bytes), patch) in entries.iter().zip(patches) {
        // Cubism uses the low 32 bits for offsets, even though the slot is 64 bits.
        let start = u32::try_from(offset).map_err(|_| Cmo3Error::TooLarge)?;
        let mask = ((key as i64) << 32) | key as i64;
        table.0[patch..patch + 8].copy_from_slice(&((start as u64) ^ mask as u64).to_be_bytes());
        offset = offset.checked_add(bytes.len()).ok_or(Cmo3Error::TooLarge)?;
    }
    u32::try_from(offset).map_err(|_| Cmo3Error::TooLarge)?;
    let mut out = Vec::with_capacity(offset + 2);
    out.extend(b"CAFF\0\0\0----\0\0\0");
    out.extend(key.to_be_bytes());
    out.extend([0; 8]);
    out.extend([127, 127]);
    out.extend([0; 26]);
    out.extend(table.0);
    for (_, _, _, bytes) in entries {
        out.extend(bytes.into_iter().map(|b| b ^ key as u8));
    }
    out.extend(b"bc");
    Ok(out)
}
struct Writer(Vec<u8>, i32);
impl Writer {
    fn new(key: i32) -> Self {
        Self(Vec::new(), key)
    }
    fn byte(&mut self, v: u8) {
        self.0.push(v ^ self.1 as u8);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend((v ^ self.1 as u32).to_be_bytes());
    }
    fn u64(&mut self, v: u64) {
        let mask = ((self.1 as i64) << 32) | self.1 as i64;
        self.0.extend((v ^ mask as u64).to_be_bytes());
    }
    fn string(&mut self, v: &str) -> Result<(), Cmo3Error> {
        let len = v.len();
        if len >= 1 << 28 {
            return Err(Cmo3Error::TooLarge);
        }
        let mut shift = 21;
        while shift > 0 && len >> shift == 0 {
            shift -= 7;
        }
        loop {
            self.byte(((len >> shift) & 127) as u8 | if shift > 0 { 128 } else { 0 });
            if shift == 0 {
                break;
            }
            shift -= 7;
        }
        for b in v.bytes() {
            self.byte(b);
        }
        Ok(())
    }
}

use super::{Result, invalid};
use crate::Cmo3Error;
use std::{collections::HashMap, io::Read};

const MAX_BYTES: usize = 512 * 1024 * 1024;
/// Read CAFF without extracting paths. Cubism writes a streaming ZIP, which need
/// not have a central directory; use its local-file record rather than ZipArchive.
#[cfg(any(test, feature = "test-support"))]
pub fn read(bytes: &[u8]) -> Result<HashMap<String, Vec<u8>>> {
    read_limited(bytes, MAX_BYTES)
}
pub fn read_limited(bytes: &[u8], limit: usize) -> Result<HashMap<String, Vec<u8>>> {
    let limit = limit.min(MAX_BYTES);
    if bytes.len() < 60
        || bytes.len() > MAX_BYTES
        || &bytes[..4] != b"CAFF"
        || !bytes.ends_with(b"bc")
    {
        return Err(invalid("Invalid or oversized CAFF archive"));
    }
    let key = i32::from_be_bytes(bytes[14..18].try_into().unwrap());
    let mut reader = Reader { bytes, at: 54, key };
    let count = reader.u32()? as usize;
    if count > 16384 {
        return Err(invalid("Too many CAFF resources"));
    }
    let mut table = Vec::new();
    for _ in 0..count {
        let path = reader.string()?;
        let _tag = reader.string()?;
        // Real Cubism archives use only the low 32 bits of this slot.
        let offset = (reader.u64()? as u32) as usize;
        let size = reader.u32()? as usize;
        let flags = reader.byte()?;
        // Cubism's native saver emits clear entries (flag 0) with XOR key 0.
        // Our writer and obfuscated native projects use flag 1.
        if flags != 1 && !(flags == 0 && key == 0) {
            return Err(invalid("Unsupported CAFF entry flags"));
        }
        let compression = reader.byte()?;
        reader.take(8)?;
        table.push((path, offset, size, compression));
    }
    let table_end = reader.at;
    let mut total = 0usize;
    let mut out = HashMap::new();
    let mut ranges = Vec::new();
    for (path, start, size, compression) in table {
        let end = start.checked_add(size).ok_or(Cmo3Error::TooLarge)?;
        if start < table_end
            || end > bytes.len() - 2
            || ranges.iter().any(|&(a, b)| start < b && end > a)
        {
            return Err(invalid("Invalid or overlapping CAFF resource range"));
        }
        ranges.push((start, end));
        if size > limit.saturating_sub(total) {
            return Err(invalid("CAFF stored resource budget exceeded"));
        }
        let stored: Vec<_> = bytes[start..end].iter().map(|v| v ^ key as u8).collect();
        let data = match compression {
            16 => stored,
            33 => unzip(&stored, limit - total)?,
            _ => {
                return Err(invalid(format!(
                    "Unsupported CAFF compression: {compression}"
                )));
            }
        };
        total = total.checked_add(data.len()).ok_or(Cmo3Error::TooLarge)?;
        if total > limit {
            return Err(invalid("CAFF decoded resource budget exceeded"));
        }
        if out.insert(path, data).is_some() {
            return Err(invalid("Duplicate CAFF resource"));
        }
    }
    Ok(out)
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
    key: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_clear_entries_work_and_unknown_flags_still_fail() {
        let xml = b"<native-save/>";
        let mut archive = crate::caff::encode(Vec::new(), xml, 0).unwrap();
        let mut table = Reader {
            bytes: &archive,
            at: 54,
            key: 0,
        };
        assert_eq!(table.u32().unwrap(), 1);
        assert_eq!(table.string().unwrap(), "main.xml");
        assert_eq!(table.string().unwrap(), "main_xml");
        table.u64().unwrap();
        table.u32().unwrap();
        let flag = table.at;
        archive[flag] = 0;
        assert_eq!(read(&archive).unwrap()["main.xml"], xml);
        archive[flag] = 2;
        assert!(read(&archive).is_err());
    }
}
impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self.at.checked_add(n).ok_or(Cmo3Error::TooLarge)?;
        let b = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| invalid("Truncated CAFF table"))?;
        self.at = end;
        Ok(b)
    }
    fn byte(&mut self) -> Result<u8> {
        let key = self.key as u8;
        Ok(self.take(1)?[0] ^ key)
    }
    fn u32(&mut self) -> Result<u32> {
        let key = self.key as u32;
        Ok(u32::from_be_bytes(self.take(4)?.try_into().unwrap()) ^ key)
    }
    fn u64(&mut self) -> Result<u64> {
        let mask = (((self.key as i64) << 32) | self.key as i64) as u64;
        Ok(u64::from_be_bytes(self.take(8)?.try_into().unwrap()) ^ mask)
    }
    fn string(&mut self) -> Result<String> {
        let mut n = 0usize;
        for index in 0..4 {
            let b = self.byte()?;
            n = (n << 7) | (b & 127) as usize;
            if b & 128 == 0 {
                if n > 1048576 {
                    return Err(invalid("CAFF string too large"));
                }
                let key = self.key as u8;
                return String::from_utf8(self.take(n)?.iter().map(|v| v ^ key).collect())
                    .map_err(|e| invalid(e.to_string()));
            }
            if index == 3 {
                return Err(invalid("Invalid CAFF string length"));
            }
        }
        unreachable!()
    }
}

fn unzip(bytes: &[u8], budget: usize) -> Result<Vec<u8>> {
    let u16_at = |i| {
        bytes
            .get(i..i + 2)
            .map(|v| u16::from_le_bytes(v.try_into().unwrap()))
            .ok_or_else(|| invalid("Truncated ZIP header"))
    };
    let u32_at = |i| {
        bytes
            .get(i..i + 4)
            .map(|v| u32::from_le_bytes(v.try_into().unwrap()))
            .ok_or_else(|| invalid("Truncated ZIP record"))
    };
    if u32_at(0)? != 0x04034b50 || u16_at(8)? != 8 || u16_at(6)? & 1 != 0 {
        return Err(invalid("Unsupported Cubism ZIP payload"));
    }
    let name_len = u16_at(26)? as usize;
    let extra_len = u16_at(28)? as usize;
    if bytes.get(30..30 + name_len) != Some(b"contents".as_slice()) {
        return Err(invalid("Unexpected Cubism ZIP filename"));
    }
    let start = 30 + name_len + extra_len;
    let compressed = bytes
        .get(start..)
        .ok_or_else(|| invalid("Truncated ZIP contents"))?;
    let mut decoder = flate2::read::DeflateDecoder::new(compressed);
    let mut out = Vec::new();
    decoder
        .by_ref()
        .take((budget + 1) as u64)
        .read_to_end(&mut out)?;
    if out.len() > budget {
        return Err(invalid("Cubism XML decoded budget exceeded"));
    }
    let consumed = decoder.total_in() as usize;
    let (crc, size) = if u16_at(6)? & 8 != 0 {
        let mut at = start + consumed;
        if u32_at(at)? == 0x08074b50 {
            at += 4;
        }
        if u32_at(at + 4)? as usize != consumed {
            return Err(invalid("ZIP compressed size mismatch"));
        }
        (u32_at(at)?, u32_at(at + 8)?)
    } else {
        if u32_at(18)? as usize != consumed {
            return Err(invalid("ZIP compressed size mismatch"));
        }
        (u32_at(14)?, u32_at(22)?)
    };
    if size as usize != out.len() || crc32fast::hash(&out) != crc {
        return Err(invalid("ZIP integrity check failed"));
    }
    Ok(out)
}

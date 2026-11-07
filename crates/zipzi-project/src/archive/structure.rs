//! Admission before zip::ZipArchive allocates its index. All offsets are absolute;
//! self-extracting prefixes, split archives and unaccounted data are unsupported.
use std::collections::BTreeSet;
use std::io::{Read, Seek, SeekFrom};

use super::{asset_hash, check, invalid, ArchiveError, ArchiveIoLimits, Result};

fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes(b[i..i + 2].try_into().unwrap())
}
fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().unwrap())
}
fn end(a: u64, b: u64, limit: u64) -> Result<u64> {
    a.checked_add(b)
        .filter(|n| *n <= limit)
        .ok_or_else(|| invalid("ZIP region outside archive"))
}
fn read<const N: usize>(input: &mut (impl Read + Seek), offset: u64) -> Result<[u8; N]> {
    input.seek(SeekFrom::Start(offset))?;
    let mut b = [0; N];
    input.read_exact(&mut b)?;
    Ok(b)
}
fn extra_zip64(extra: &[u8]) -> Result<Option<&[u8]>> {
    let mut rest = extra;
    let mut zip64 = None;
    while !rest.is_empty() {
        if rest.len() < 4 {
            return Err(invalid("Truncated ZIP extra field"));
        }
        let tag = u16_at(rest, 0);
        let size = usize::from(u16_at(rest, 2));
        if size > rest.len() - 4 {
            return Err(invalid("Truncated ZIP extra field"));
        }
        // These fields can override the validated filename or enable encryption.
        if matches!(tag, 0x7075 | 0x9901) {
            return Err(invalid("Unsupported ZIP extra field"));
        }
        if tag == 1 {
            if zip64.is_some() {
                return Err(invalid("Duplicate ZIP64 extra field"));
            }
            zip64 = Some(&rest[4..4 + size]);
        }
        rest = &rest[4 + size..];
    }
    Ok(zip64)
}
fn take64(extra: &mut &[u8]) -> Result<u64> {
    if extra.len() < 8 {
        return Err(invalid("Missing ZIP64 value"));
    }
    let v = u64_at(extra, 0);
    *extra = &extra[8..];
    Ok(v)
}
struct Entry {
    name: Vec<u8>,
    flags: u16,
    method: u16,
    crc: u32,
    needed: u16,
    modified: u32,
    compressed: u64,
    expanded: u64,
    offset: u64,
}

pub(super) fn validate(
    input: &mut (impl Read + Seek),
    length: u64,
    limits: ArchiveIoLimits,
    keep_going: &mut impl FnMut() -> bool,
) -> Result<BTreeSet<String>> {
    validate_budgeted(input, length, limits, keep_going, None).map(|(names, _)| names)
}

pub(super) fn validate_budgeted(
    input: &mut (impl Read + Seek),
    length: u64,
    limits: ArchiveIoLimits,
    keep_going: &mut impl FnMut() -> bool,
    memory: Option<&crate::TransferReservation>,
) -> Result<(BTreeSet<String>, usize)> {
    if length > limits.archive_bytes {
        return Err(ArchiveError::Limit("archive bytes"));
    }
    if length < 22 {
        return Err(invalid("Not a ZIP archive"));
    }
    let tail_len = length.min(22 + u16::MAX as u64) as usize;
    input.seek(SeekFrom::Start(length - tail_len as u64))?;
    let mut tail = vec![0; tail_len];
    input.read_exact(&mut tail)?;
    let eocd = (0..=tail_len - 22)
        .rev()
        .find(|i| {
            tail[*i..*i + 4] == *b"PK\x05\x06"
                && *i + 22 + usize::from(u16_at(&tail, *i + 20)) == tail_len
        })
        .ok_or_else(|| invalid("Missing or truncated ZIP end record"))?;
    let e = &tail[eocd..];
    if u16_at(e, 4) != 0 || u16_at(e, 6) != 0 || u16_at(e, 8) != u16_at(e, 10) {
        return Err(invalid("Split ZIP archives are unsupported"));
    }
    let eocd_offset = length - tail_len as u64 + eocd as u64;
    let mut count = u64::from(u16_at(e, 10));
    let mut directory_size = u64::from(u32_at(e, 12));
    let mut directory = u64::from(u32_at(e, 16));
    let mut end_records = eocd_offset;
    let sentinel = count == 0xffff || directory_size == 0xffff_ffff || directory == 0xffff_ffff;
    let has_locator = eocd_offset >= 20 && read::<4>(input, eocd_offset - 20)? == *b"PK\x06\x07";
    if sentinel || has_locator {
        if !has_locator {
            return Err(invalid("Missing ZIP64 locator"));
        }
        let locator = read::<20>(input, eocd_offset - 20)?;
        if u32_at(&locator, 4) != 0 || u32_at(&locator, 16) != 1 {
            return Err(invalid("Split ZIP64 archive"));
        }
        let offset = u64_at(&locator, 8);
        end(offset, 56, eocd_offset - 20)?;
        let z = read::<56>(input, offset)?;
        let record_size = u64_at(&z, 4);
        if z[..4] != *b"PK\x06\x06"
            || !(44..=65536).contains(&record_size)
            || end(offset, 12 + record_size, eocd_offset - 20)? != eocd_offset - 20
            || u32_at(&z, 16) != 0
            || u32_at(&z, 20) != 0
            || u64_at(&z, 24) != u64_at(&z, 32)
        {
            return Err(invalid("Invalid ZIP64 end record"));
        }
        let (zc, zs, zo) = (u64_at(&z, 32), u64_at(&z, 40), u64_at(&z, 48));
        if (count != 0xffff && count != zc)
            || (directory_size != 0xffff_ffff && directory_size != zs)
            || (directory != 0xffff_ffff && directory != zo)
        {
            return Err(invalid("Inconsistent ZIP64 end records"));
        }
        (count, directory_size, directory, end_records) = (zc, zs, zo, offset);
    }
    if count > limits.entries as u64 {
        return Err(ArchiveError::Limit("entries"));
    }
    if directory_size > limits.directory_bytes {
        return Err(ArchiveError::Limit("central directory bytes"));
    }
    if end(directory, directory_size, end_records)? != end_records || count > directory_size / 46 {
        return Err(invalid("Invalid central directory region"));
    }
    // Both indexes, path sets and streaming buffers are admitted before allocation.
    let workspace = directory_size
        .checked_mul(8)
        .and_then(|n| count.checked_mul(2048)?.checked_add(n))
        .and_then(|n| n.checked_add(2 << 20))
        .and_then(|n| usize::try_from(n).ok())
        .ok_or(ArchiveError::Limit("index workspace overflow"))?;
    if let Some(memory) = memory {
        memory.ensure_at_least(workspace)?;
    }
    let mut entries = Vec::with_capacity(count as usize);
    let mut names = BTreeSet::new();
    let mut cursor = directory;
    let mut expanded_total = 0u64;
    for _ in 0..count {
        check(keep_going)?;
        end(cursor, 46, end_records)?;
        let h = read::<46>(input, cursor)?;
        if h[..4] != *b"PK\x01\x02" {
            return Err(invalid("Invalid central header"));
        }
        let flags = u16_at(&h, 8);
        let method = u16_at(&h, 10);
        if u16_at(&h, 6) > 45 {
            return Err(invalid("Unsupported ZIP extraction version"));
        }
        if flags & !0x080e != 0 || !matches!(method, 0 | 8) || (method == 0 && flags & 6 != 0) {
            return Err(invalid("Encrypted or unsupported ZIP entry"));
        }
        let name_len = usize::from(u16_at(&h, 28));
        let extra_len = usize::from(u16_at(&h, 30));
        let total = (46 + name_len + extra_len + usize::from(u16_at(&h, 32))) as u64;
        cursor = end(cursor, total, end_records)?;
        // Protocol paths have a small fixed maximum; reject before allocating.
        if name_len == 0 || name_len > 75 {
            return Err(invalid("Invalid ZIP path"));
        }
        let mut name = vec![0; name_len];
        input.read_exact(&mut name)?;
        let name_text =
            std::str::from_utf8(&name).map_err(|_| invalid("Invalid ZIP path encoding"))?;
        if !matches!(
            name_text,
            "format.json" | "project.json" | "assets/" | "preview.png"
        ) && asset_hash(name_text).is_none()
        {
            return Err(ArchiveError::Document {
                code: "UNSUPPORTED_PROJECT_ENTRY".into(),
                message: name_text.into(),
            });
        }
        if !names.insert(name_text.to_owned()) {
            return Err(invalid("Duplicate ZIP path"));
        }
        let mode = u32_at(&h, 38) >> 16;
        let kind = mode & 0o170000;
        if !matches!(kind, 0 | 0o100000 | 0o040000)
            || (kind == 0o040000 || u32_at(&h, 38) & 0x10 != 0) != (name_text == "assets/")
        {
            return Err(invalid("Symlink or special ZIP entry"));
        }
        let mut extra = vec![0; extra_len];
        input.read_exact(&mut extra)?;
        let mut z = extra_zip64(&extra)?.unwrap_or_default();
        let mut expanded = u64::from(u32_at(&h, 24));
        let mut compressed = u64::from(u32_at(&h, 20));
        let mut offset = u64::from(u32_at(&h, 42));
        if expanded == 0xffff_ffff {
            expanded = take64(&mut z)?;
        }
        if compressed == 0xffff_ffff {
            compressed = take64(&mut z)?;
        }
        if offset == 0xffff_ffff {
            offset = take64(&mut z)?;
        }
        let disk = u16_at(&h, 34);
        if disk != 0 && (disk != 0xffff || z.len() < 4 || u32_at(z, 0) != 0) {
            return Err(invalid("Split ZIP entry"));
        }
        if (name_text == "assets/" && expanded != 0) || (method == 0 && expanded != compressed) {
            return Err(invalid("Invalid ZIP size"));
        }
        super::add(&mut expanded_total, expanded, limits.expanded_bytes)?;
        let json_limit = match name_text {
            "format.json" => limits.format_bytes,
            "project.json" => limits.document_bytes,
            "preview.png" => zipzi_project_preview::MAX_PREVIEW_BYTES as u64,
            _ => u64::MAX,
        };
        if expanded > json_limit {
            return Err(ArchiveError::Limit("JSON bytes"));
        }
        entries.push(Entry {
            name,
            flags,
            method,
            crc: u32_at(&h, 16),
            needed: u16_at(&h, 6),
            modified: u32_at(&h, 12),
            compressed,
            expanded,
            offset,
        });
    }
    if cursor != end_records {
        return Err(invalid("Central directory count/size mismatch"));
    }
    if !names.contains("format.json") || !names.contains("project.json") {
        return Err(invalid("Missing project manifest"));
    }
    entries.sort_unstable_by_key(|e| e.offset);
    let mut previous_end = 0;
    for (index, entry) in entries.iter().enumerate() {
        check(keep_going)?;
        if entry.offset != previous_end {
            return Err(invalid("Overlapping entries or unaccounted ZIP data"));
        }
        end(entry.offset, 30, directory)?;
        let h = read::<30>(input, entry.offset)?;
        if h[..4] != *b"PK\x03\x04"
            || u16_at(&h, 6) != entry.flags
            || u16_at(&h, 8) != entry.method
            || u16_at(&h, 4) != entry.needed
            || u32_at(&h, 10) != entry.modified
        {
            return Err(invalid("Local/central ZIP header mismatch"));
        }
        let nl = usize::from(u16_at(&h, 26));
        let el = usize::from(u16_at(&h, 28));
        let data = end(entry.offset, (30 + nl + el) as u64, directory)?;
        if nl != entry.name.len() {
            return Err(invalid("Local/central ZIP name mismatch"));
        }
        let mut name = vec![0; nl];
        input.read_exact(&mut name)?;
        if name != entry.name {
            return Err(invalid("Local/central ZIP name mismatch"));
        }
        let mut extra = vec![0; el];
        input.read_exact(&mut extra)?;
        let mut z = extra_zip64(&extra)?.unwrap_or_default();
        let mut expanded = u64::from(u32_at(&h, 22));
        let mut compressed = u64::from(u32_at(&h, 18));
        let local_zip64 = !z.is_empty() || expanded == 0xffff_ffff || compressed == 0xffff_ffff;
        if expanded == 0xffff_ffff {
            expanded = take64(&mut z)?;
        }
        if compressed == 0xffff_ffff {
            compressed = take64(&mut z)?;
        }
        let descriptor = entry.flags & 8 != 0;
        let crc = u32_at(&h, 14);
        if (!descriptor
            && (crc != entry.crc || expanded != entry.expanded || compressed != entry.compressed))
            || (descriptor
                && ((crc != 0 && crc != entry.crc)
                    || (expanded != 0 && expanded != entry.expanded)
                    || (compressed != 0 && compressed != entry.compressed)))
        {
            return Err(invalid("Local/central ZIP size or CRC mismatch"));
        }
        let data_end = end(data, entry.compressed, directory)?;
        let next = entries.get(index + 1).map_or(directory, |e| e.offset);
        previous_end = if descriptor {
            let width = if local_zip64 { 20 } else { 12 };
            let signed = next.checked_sub(data_end) == Some(width + 4);
            if next.checked_sub(data_end) != Some(width + if signed { 4 } else { 0 }) {
                return Err(invalid("Invalid data descriptor boundary"));
            }
            let mut bytes = vec![0; (width + if signed { 4 } else { 0 }) as usize];
            input.seek(SeekFrom::Start(data_end))?;
            input.read_exact(&mut bytes)?;
            let b = if signed {
                if bytes[..4] != *b"PK\x07\x08" {
                    return Err(invalid("Invalid descriptor signature"));
                }
                &bytes[4..]
            } else {
                &bytes[..]
            };
            let (compressed, expanded) = if local_zip64 {
                (u64_at(b, 4), u64_at(b, 12))
            } else {
                (u64::from(u32_at(b, 4)), u64::from(u32_at(b, 8)))
            };
            if u32_at(b, 0) != entry.crc
                || compressed != entry.compressed
                || expanded != entry.expanded
            {
                return Err(invalid("ZIP descriptor mismatch"));
            }
            next
        } else {
            data_end
        };
    }
    if previous_end != directory {
        return Err(invalid("Unaccounted ZIP data before central directory"));
    }
    input.rewind()?;
    Ok((names, workspace))
}

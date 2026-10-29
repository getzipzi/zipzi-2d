//! Check mask metadata which ag-psd 0.3.0 silently drops: its additional-info
//! alias dispatch misses the standard `vmsk` key, and its mask model omits the
//! obsolete invert flag. Never bake only part of a combined mask by accident.
//! This walks bounded layer records, never searching names or pixel payloads
//! for signatures. Remove the checks when the reader preserves this metadata.

use crate::ImportError;

fn invalid() -> ImportError {
    ImportError::new("INVALID_PSD", "Invalid PSD layer metadata")
}

fn take<'a>(bytes: &mut &'a [u8], count: usize) -> Result<&'a [u8], ImportError> {
    let (value, rest) = bytes.split_at_checked(count).ok_or_else(invalid)?;
    *bytes = rest;
    Ok(value)
}

fn u32_len(bytes: &mut &[u8]) -> Result<usize, ImportError> {
    Ok(u32::from_be_bytes(take(bytes, 4)?.try_into().unwrap()) as usize)
}

fn section<'a>(bytes: &mut &'a [u8]) -> Result<&'a [u8], ImportError> {
    let count = u32_len(bytes)?;
    take(bytes, count)
}

pub(super) fn check(bytes: &[u8]) -> Result<(), ImportError> {
    // The caller has already validated the version-1, 8-bit RGB header.
    let mut file = bytes.get(26..).ok_or_else(invalid)?;
    section(&mut file)?; // Color mode data.
    section(&mut file)?; // Image resources.
    let mut layers_and_masks = section(&mut file)?;
    if layers_and_masks.is_empty() {
        return Ok(());
    }
    let mut layers = section(&mut layers_and_masks)?;
    if layers.is_empty() {
        return Ok(());
    }
    let count = i16::from_be_bytes(take(&mut layers, 2)?.try_into().unwrap()).unsigned_abs();
    for _ in 0..count {
        take(&mut layers, 16)?; // Layer bounds.
        let channels = u16::from_be_bytes(take(&mut layers, 2)?.try_into().unwrap());
        take(&mut layers, usize::from(channels) * 6 + 12)?; // Channels and blending fields.
        let mut extra = section(&mut layers)?;
        let mask = section(&mut extra)?;
        let inverted = mask
            .get(17)
            .is_some_and(|flags| flags & 4 != 0 && flags & 2 == 0);
        section(&mut extra)?; // Blending ranges.
        let name_len = usize::from(take(&mut extra, 1)?[0]);
        let name = take(&mut extra, name_len)?;
        take(&mut extra, (4 - (name_len + 1) % 4) % 4)?;

        let mut vector_mask = false;
        let mut unicode_name = None;
        while !extra.is_empty() {
            // Some writers align additional-info records to four bytes, others
            // to two. Only skip padding between records, never within payloads.
            while extra.first() == Some(&0) {
                take(&mut extra, 1)?;
            }
            if extra.is_empty() {
                break;
            }
            let signature = take(&mut extra, 4)?;
            if signature != b"8BIM" && signature != b"8B64" {
                return Err(invalid());
            }
            let key = take(&mut extra, 4)?;
            if signature == b"8B64" && u32_len(&mut extra)? != 0 {
                return Err(invalid());
            }
            let payload = section(&mut extra)?;
            take(&mut extra, payload.len() % 2)?;
            vector_mask |= matches!(key, b"vmsk" | b"vsms");
            if key == b"luni" {
                unicode_name = Some(payload);
            }
        }
        if vector_mask || inverted {
            let mut name = String::from_utf8_lossy(name).into_owned();
            if let Some(mut payload) = unicode_name {
                let count = u32_len(&mut payload)?;
                let chars = take(&mut payload, count.checked_mul(2).ok_or_else(invalid)?)?;
                name = char::decode_utf16(
                    chars
                        .chunks_exact(2)
                        .map(|c| u16::from_be_bytes([c[0], c[1]])),
                )
                .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
                .collect();
            }
            let reason = if vector_mask {
                "vector and combined masks cannot be represented"
            } else {
                "obsolete inverted bitmap masks cannot be represented"
            };
            return Err(ImportError::new(
                "UNSUPPORTED_LAYER",
                format!("{name}: {reason}"),
            ));
        }
    }
    Ok(())
}

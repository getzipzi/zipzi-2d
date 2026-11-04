use super::*;
use std::collections::BTreeSet;
use zip::{CompressionMethod, ZipArchive};

// Bound cumulative local-header/index reads too, including adversarial seeks.
// The central-directory cap alone does not cap local extra-field work.
struct BoundedInput<R> {
    input: R,
    remaining: usize,
}
impl<R: Read> Read for BoundedInput<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if !bytes.is_empty() && self.remaining == 0 {
            return Err(io::Error::other("preview read budget"));
        }
        let count = bytes.len().min(self.remaining);
        let read = self.input.read(&mut bytes[..count])?;
        self.remaining -= read;
        Ok(read)
    }
}
impl<R: Seek> Seek for BoundedInput<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.input.seek(position)
    }
}

fn invalid(message: &'static str) -> PreviewError {
    PreviewError::Invalid(message)
}
fn u16_at(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes(b[i..i + 2].try_into().unwrap())
}
fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}
fn u64_at(b: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(b[i..i + 8].try_into().unwrap())
}
fn region(a: u64, b: u64, limit: u64) -> Result<u64> {
    a.checked_add(b)
        .filter(|n| *n <= limit)
        .ok_or(invalid("ZIP region"))
}
fn at<const N: usize>(input: &mut (impl Read + Seek), offset: u64) -> Result<[u8; N]> {
    input.seek(SeekFrom::Start(offset))?;
    let mut bytes = [0; N];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}

// Check before ZipArchive allocates an index. No full archive buffering.
fn admit(input: &mut (impl Read + Seek)) -> Result<(usize, u64)> {
    let length = input.seek(SeekFrom::End(0))?;
    if !(22..=16 << 30).contains(&length) {
        return Err(invalid("archive size"));
    }
    let tail_len = length.min(65557) as usize;
    input.seek(SeekFrom::Start(length - tail_len as u64))?;
    let mut tail = vec![0; tail_len];
    input.read_exact(&mut tail)?;
    let e = (0..=tail_len - 22)
        .rev()
        .find(|i| {
            tail[*i..*i + 4] == *b"PK\x05\x06"
                && *i + 22 + usize::from(u16_at(&tail, *i + 20)) == tail_len
        })
        .ok_or(invalid("ZIP end"))?;
    let end = length - tail_len as u64 + e as u64;
    let h = &tail[e..];
    if u16_at(h, 4) != 0 || u16_at(h, 6) != 0 || u16_at(h, 8) != u16_at(h, 10) {
        return Err(invalid("split ZIP"));
    }
    let (mut count, mut size, mut offset) = (
        u64::from(u16_at(h, 10)),
        u64::from(u32_at(h, 12)),
        u64::from(u32_at(h, 16)),
    );
    let mut directory_end = end;
    if count == 65535 || size == u32::MAX as u64 || offset == u32::MAX as u64 {
        let locator = at::<20>(input, end.checked_sub(20).ok_or(invalid("ZIP64 locator"))?)?;
        if locator[..4] != *b"PK\x06\x07" || u32_at(&locator, 4) != 0 || u32_at(&locator, 16) != 1 {
            return Err(invalid("ZIP64 locator"));
        }
        let zoffset = u64_at(&locator, 8);
        let z = at::<56>(input, zoffset)?;
        if z[..4] != *b"PK\x06\x06"
            || u64_at(&z, 4) != 44
            || region(zoffset, 56, end)? != end - 20
            || u32_at(&z, 16) != 0
            || u32_at(&z, 20) != 0
            || u64_at(&z, 24) != u64_at(&z, 32)
        {
            return Err(invalid("ZIP64 end"));
        }
        (count, size, offset) = (u64_at(&z, 32), u64_at(&z, 40), u64_at(&z, 48));
        directory_end = zoffset;
    }
    if count > 16384 || size > 8 << 20 || region(offset, size, directory_end)? != directory_end {
        return Err(invalid("ZIP index limit or region"));
    }
    Ok((count as usize, offset))
}

#[derive(Deserialize)]
struct Format {
    format: String,
    container_version: u32,
    document: String,
    required_features: Vec<String>,
    #[serde(default)]
    preview: Option<PreviewDescriptor>,
}
fn entry(
    input: &mut ZipArchive<impl Read + Seek>,
    name: &str,
    limit: usize,
    directory: u64,
) -> Result<Vec<u8>> {
    let mut f = input.by_name(name)?;
    if f.is_dir()
        || f.size() > limit as u64
        || f.compressed_size() > limit as u64
        || !matches!(
            f.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        )
        || f.unix_mode()
            .is_some_and(|m| !matches!(m & 0o170000, 0 | 0o100000))
    {
        return Err(invalid("preview entry"));
    }
    let start = f.data_start().ok_or(invalid("entry offset"))?;
    region(start, f.compressed_size(), directory)?;
    let declared = f.size();
    let mut bytes = Vec::new();
    f.by_ref().take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != declared || bytes.len() > limit {
        return Err(invalid("entry length"));
    }
    Ok(bytes)
}
pub(super) fn read(input: impl Read + Seek) -> Result<Option<ProjectPreview>> {
    let mut input = BoundedInput {
        input,
        remaining: 32 << 20,
    };
    let (count, directory) = admit(&mut input)?;
    let mut zip = ZipArchive::new(input)?;
    if zip.len() != count {
        return Err(invalid("ZIP count"));
    }
    let mut names = BTreeSet::new();
    for index in 0..count {
        let f = zip.by_index(index)?;
        let name = f.name()?;
        let name = name.as_ref();
        if !names.insert(name.to_owned()) || name.len() > 75 {
            return Err(invalid("ZIP paths"));
        }
        if !matches!(
            name,
            "format.json" | "project.json" | "preview.png" | "assets/"
        ) {
            let hash = name
                .strip_prefix("assets/")
                .and_then(|n| n.strip_suffix(".png"));
            if !hash.is_some_and(|h| {
                h.len() == 64
                    && h.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            }) {
                return Err(invalid("ZIP path"));
            }
        }
    }
    if !names.contains("project.json") {
        return Err(invalid("document absent"));
    }
    let format: Format =
        serde_json::from_slice(&entry(&mut zip, "format.json", 64 << 10, directory)?)?;
    if format.format != "zipzi-project-package"
        || format.container_version != 1
        || format.document != "project.json"
        || !format.required_features.is_empty()
    {
        return Err(invalid("unsupported project"));
    }
    let Some(descriptor) = format.preview else {
        if names.contains(PREVIEW_PATH) {
            return Err(invalid("undeclared preview"));
        }
        return Ok(None);
    };
    let preview =
        ProjectPreview::from_png(entry(&mut zip, PREVIEW_PATH, MAX_PREVIEW_BYTES, directory)?)?;
    if preview.descriptor() != &descriptor {
        return Err(invalid("preview hash or dimensions"));
    }
    Ok(Some(preview))
}

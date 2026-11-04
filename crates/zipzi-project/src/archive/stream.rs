use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom, Write};

use super::{check, ArchiveError, Result};

const BUFFER_BYTES: usize = 1 << 20;

/// Hash the finalized file, including headers, resources and central directory.
pub use zipzi_contract::authoring::ProjectFingerprint;

pub fn read_fingerprint(
    reader: &mut impl Read,
    limit: u64,
    mut keep_going: impl FnMut() -> bool,
) -> Result<ProjectFingerprint> {
    copy(reader, &mut std::io::sink(), limit, &mut keep_going)
}

/// Copy/hash without buffering an entry or archive. Reading limit+1 detects a
/// growing or lying input; the excess byte is never written to the output.
pub(super) fn copy(
    reader: &mut impl Read,
    writer: &mut impl Write,
    limit: u64,
    keep_going: &mut impl FnMut() -> bool,
) -> Result<ProjectFingerprint> {
    let mut buffer = vec![0; BUFFER_BYTES];
    let mut hasher = Sha256::new();
    let mut length = 0u64;
    loop {
        check(keep_going)?;
        let capacity = limit
            .saturating_sub(length)
            .saturating_add(1)
            .min(buffer.len() as u64) as usize;
        let count = match reader.read(&mut buffer[..capacity]) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            break;
        }
        length = length
            .checked_add(count as u64)
            .filter(|n| *n <= limit)
            .ok_or(ArchiveError::Limit("actual read bytes"))?;
        writer.write_all(&buffer[..count])?;
        hasher.update(&buffer[..count]);
    }
    check(keep_going)?;
    let mut sha256 = String::with_capacity(64);
    for byte in hasher.finalize() {
        use std::fmt::Write;
        write!(&mut sha256, "{byte:02x}").unwrap();
    }
    Ok(ProjectFingerprint { sha256, length })
}

pub(super) fn copy_exact(
    reader: &mut impl Read,
    writer: &mut impl Write,
    length: u64,
    keep_going: &mut impl FnMut() -> bool,
) -> Result<ProjectFingerprint> {
    let result = copy(reader, writer, length, keep_going)?;
    if result.length != length {
        return Err(ArchiveError::Invalid(
            "Input length changed or is truncated".into(),
        ));
    }
    Ok(result)
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct IoCounts {
    pub read_bytes: u64,
    pub write_bytes: u64,
}

impl std::ops::Add for IoCounts {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self {
            read_bytes: self.read_bytes + other.read_bytes,
            write_bytes: self.write_bytes + other.write_bytes,
        }
    }
}

/// Logical IO at the wrapped stream, including ZIP backpatches and hash rereads.
/// This is not physical device traffic (which depends on OS filesystem caches).
pub struct MeteredIo<T> {
    inner: T,
    counts: IoCounts,
}

impl<T> MeteredIo<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            counts: IoCounts::default(),
        }
    }
    pub fn inner(&self) -> &T {
        &self.inner
    }
    pub fn counts(&self) -> IoCounts {
        self.counts
    }
}

impl<T: Read> Read for MeteredIo<T> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let count = self.inner.read(bytes)?;
        self.counts.read_bytes += count as u64;
        Ok(count)
    }
}
impl<T: Write> Write for MeteredIo<T> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let count = self.inner.write(bytes)?;
        self.counts.write_bytes += count as u64;
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
impl<T: Seek> Seek for MeteredIo<T> {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// Enforce the output extent while writing, including ZIP header backpatches.
/// Checking only the final file length would permit an unbounded staging file.
pub(super) struct BoundedWriter<'a, W> {
    inner: &'a mut W,
    position: u64,
    limit: u64,
}
impl<'a, W: Write + Seek> BoundedWriter<'a, W> {
    pub fn new(inner: &'a mut W, limit: u64) -> Self {
        Self {
            inner,
            position: 0,
            limit,
        }
    }
}
impl<W: Write + Seek> Write for BoundedWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.limit.saturating_sub(self.position) {
            return Err(std::io::Error::other(
                "Project archive output limit exceeded",
            ));
        }
        let n = self.inner.write(bytes)?;
        self.position += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
impl<W: Write + Seek> Seek for BoundedWriter<'_, W> {
    fn seek(&mut self, offset: SeekFrom) -> std::io::Result<u64> {
        let position = self.inner.seek(offset)?;
        if position > self.limit {
            return Err(std::io::Error::other(
                "Project archive output limit exceeded",
            ));
        }
        self.position = position;
        Ok(position)
    }
}

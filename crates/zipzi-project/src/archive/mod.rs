//! P0 storage experiment, available only with `archive-prototype`.
//!
//! This deliberately uses the current v21 codec, advertised as a required
//! experimental feature. It is not a second production project backend. The
//! reader is for locally generated benchmark inputs: central-directory admission
//! and full local/central header validation are still required before exposing
//! it to user input. Publication and Session integration belong to P1/P2.

mod stream;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tempfile::TempDir;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipArchive, ZipWriter};
use zipzi_core::{Document, Status};

use crate::{content_sha256, decode_project, encode_project, read_project_texture_bounded};

pub use stream::{IoCounts, MeteredIo, ProjectFingerprint};

const EXPERIMENT: &str = "prototype-directory-document-v21";

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{code}: {message}")]
    Document { code: String, message: String },
    #[error("Invalid prototype archive: {0}")]
    Invalid(String),
    #[error("Prototype limit exceeded: {0}")]
    Limit(&'static str),
    #[error("Archive operation cancelled before completion")]
    Cancelled,
}

impl From<Status> for ArchiveError {
    fn from(status: Status) -> Self {
        Self::Document {
            code: status.code,
            message: status.message,
        }
    }
}

pub type Result<T> = std::result::Result<T, ArchiveError>;

/// Experimental ceilings, not yet the host's shared admission/disk budget.
#[derive(Clone, Copy, Debug)]
pub struct PrototypeLimits {
    pub archive_bytes: u64,
    pub format_bytes: u64,
    pub document_bytes: u64,
    pub expanded_bytes: u64,
    pub entries: usize,
}

impl Default for PrototypeLimits {
    fn default() -> Self {
        Self {
            archive_bytes: 16 << 30,
            format_bytes: 64 << 10,
            document_bytes: 256 << 20,
            expanded_bytes: 16 << 30,
            entries: 100_000,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Format {
    format: String,
    container_version: u32,
    document: String,
    document_sha256: String,
    required_features: Vec<String>,
}

pub struct WrittenArchive {
    /// Same authoring data with complete content-addressed asset paths.
    pub document: Document,
    pub unique_assets: usize,
    pub asset_bytes: u64,
    pub document_bytes: u64,
    /// Logical reads for resource preparation and verification during writing.
    pub resource_read_bytes: u64,
}

/// A snapshot holds the private directory independently of its originating open.
/// This models the lease boundary; History/Job integration is deliberately pending.
#[derive(Clone)]
pub struct OpenedArchive {
    pub document: Document,
    pub fingerprint: ProjectFingerprint,
    pub counts: IoCounts,
    pub unique_assets: usize,
    pub asset_bytes: u64,
    pub peak_working_bytes: u64,
    working_set: Arc<TempDir>,
}

impl OpenedArchive {
    pub fn asset_root(&self) -> &Path {
        self.working_set.path()
    }
}

struct Resource {
    path: PathBuf,
    fingerprint: ProjectFingerprint,
    dimensions: (u32, u32),
}

fn invalid(message: impl Into<String>) -> ArchiveError {
    ArchiveError::Invalid(message.into())
}

pub(super) fn check(keep_going: &mut impl FnMut() -> bool) -> Result<()> {
    if keep_going() {
        Ok(())
    } else {
        Err(ArchiveError::Cancelled)
    }
}

fn add(total: &mut u64, amount: u64, limit: u64) -> Result<()> {
    *total = total
        .checked_add(amount)
        .filter(|n| *n <= limit)
        .ok_or(ArchiveError::Limit("expanded bytes"))?;
    Ok(())
}

fn options(method: CompressionMethod) -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(method)
        .compression_level((method == CompressionMethod::Deflated).then_some(6))
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o600)
        // Force ZIP64 even on small fixtures to exercise its headers in P0.
        .large_file(true)
}

/// Write to a fresh, disposable seekable stream. The caller must discard it on
/// any error, then finalize/sync/hash it before considering publication. Only
/// encoded disk resources are supported here; unsaved memory drafts come in P2.
/// No PNG encoder or RGBA decoder runs on this path.
pub fn write_archive<W: Write + Seek>(
    document: &Document,
    asset_root: &Path,
    output: &mut W,
    limits: PrototypeLimits,
    mut keep_going: impl FnMut() -> bool,
) -> Result<WrittenArchive> {
    check(&mut keep_going)?;
    if document.transaction_active() {
        return Err(Status::error("TRANSACTION_ACTIVE", "Commit or cancel edits first").into());
    }
    if output.stream_position()? != 0 {
        return Err(invalid("Writer must start at offset zero"));
    }
    let mut candidate = document.fork_candidate();
    let mut resources = BTreeMap::<String, Resource>::new();
    let mut prepared = BTreeMap::<PathBuf, String>::new();
    let mut asset_bytes = 0;
    let mut read_bytes = 0;
    for id in document.asset_order() {
        check(&mut keep_going)?;
        let mut asset = document.get_asset(id).unwrap().clone();
        let Some(content) = asset.content.as_mut() else {
            continue;
        };
        let path = crate::store::asset_path(asset_root, document.get_asset(id).unwrap())?;
        let hash = if let Some(hash) = prepared.get(&path) {
            hash.clone()
        } else {
            let mut file = File::open(&path)?;
            let size = file.metadata()?.len();
            if size > limits.expanded_bytes {
                return Err(ArchiveError::Limit("PNG bytes"));
            }
            let mut header = [0; 33];
            file.read_exact(&mut header)?;
            read_bytes += header.len() as u64;
            let dimensions = zipzi_core::image::png_dimensions(&header)?;
            file.rewind()?;
            let fingerprint = ProjectFingerprint::read(&mut file, size, &mut keep_going)?;
            read_bytes += fingerprint.length;
            if fingerprint.length != size {
                return Err(invalid("Resource changed during preparation"));
            }
            let hash = fingerprint.sha256.clone();
            if !resources.contains_key(&hash) {
                add(&mut asset_bytes, size, limits.expanded_bytes)?;
                resources.insert(
                    hash.clone(),
                    Resource {
                        path: path.clone(),
                        fingerprint,
                        dimensions,
                    },
                );
            }
            prepared.insert(path, hash.clone());
            hash
        };
        let resource = &resources[&hash];
        if !content.sha256.is_empty() && content.sha256 != hash {
            return Err(Status::error("RESOURCE_HASH", &asset.id).into());
        }
        if (content.width, content.height) != resource.dimensions {
            return Err(Status::error("RESOURCE_DIMENSIONS", &asset.id).into());
        }
        content.source = format!("assets/{hash}.png");
        content.sha256 = hash;
        let result = candidate.replace_asset(asset);
        if !result.status.is_ok() {
            return Err(result.status.into());
        }
    }
    if resources
        .len()
        .checked_add(2)
        .is_none_or(|n| n > limits.entries)
    {
        return Err(ArchiveError::Limit("entries"));
    }
    let json = encode_project(&candidate)?;
    if json.len() as u64 > limits.document_bytes {
        return Err(ArchiveError::Limit("document bytes"));
    }
    let format = serde_json::to_vec(&Format {
        format: "zipzi-project-package".into(),
        container_version: 1,
        document: "project.json".into(),
        document_sha256: content_sha256(json.as_bytes()),
        required_features: vec![EXPERIMENT.into()],
    })?;
    if format.len() as u64 > limits.format_bytes {
        return Err(ArchiveError::Limit("format bytes"));
    }
    let mut expanded = asset_bytes;
    add(&mut expanded, json.len() as u64, limits.expanded_bytes)?;
    add(&mut expanded, format.len() as u64, limits.expanded_bytes)?;
    let mut zip = ZipWriter::new(&mut *output);
    zip.start_file("format.json", options(CompressionMethod::Deflated))?;
    zip.write_all(&format)?;
    zip.start_file("project.json", options(CompressionMethod::Deflated))?;
    zip.write_all(json.as_bytes())?;
    for (hash, resource) in &resources {
        check(&mut keep_going)?;
        zip.start_file(
            format!("assets/{hash}.png"),
            options(CompressionMethod::Stored),
        )?;
        let actual = stream::copy_exact(
            &mut File::open(&resource.path)?,
            &mut zip,
            resource.fingerprint.length,
            &mut keep_going,
        )?;
        read_bytes += actual.length;
        if actual != resource.fingerprint {
            return Err(invalid("Resource changed while writing"));
        }
    }
    check(&mut keep_going)?;
    zip.finish()?;
    if output.stream_position()? > limits.archive_bytes {
        return Err(ArchiveError::Limit("archive bytes"));
    }
    check(&mut keep_going)?;
    Ok(WrittenArchive {
        document: candidate,
        unique_assets: resources.len(),
        asset_bytes,
        document_bytes: json.len() as u64,
        resource_read_bytes: read_bytes,
    })
}

fn asset_hash(name: &str) -> Option<&str> {
    let hash = name.strip_prefix("assets/")?.strip_suffix(".png")?;
    (hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    .then_some(hash)
}

fn read_json<R: Read + Seek>(zip: &mut ZipArchive<R>, name: &str, limit: u64) -> Result<Vec<u8>> {
    let mut entry = zip.by_name(name)?;
    if entry.size() > limit {
        return Err(ArchiveError::Limit("JSON bytes"));
    }
    let size = entry.size();
    let mut bytes = Vec::new();
    // copy performs actual-length checks as well as driving the ZIP CRC reader.
    stream::copy_exact(&mut entry, &mut bytes, size, &mut || true)?;
    Ok(bytes)
}

/// Copy first, then open only the private archive. This is a benchmark reader,
/// not the P1 hardened reader. Failures drop the entire candidate directory.
/// PNGs are fully decoded one at a time after extraction to match directory-open
/// resource validation; only the encoded files survive in the returned lease.
pub fn open_archive(
    path: &Path,
    workspace_parent: &Path,
    limits: PrototypeLimits,
    mut keep_going: impl FnMut() -> bool,
) -> Result<OpenedArchive> {
    check(&mut keep_going)?;
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(invalid("Expected regular archive file"));
    }
    let working_set = Arc::new(
        tempfile::Builder::new()
            .prefix("zipzi-archive-p0-")
            .tempdir_in(workspace_parent)?,
    );
    let copy_path = working_set.path().join("input.zip");
    let mut source = MeteredIo::new(File::open(path)?);
    let before = source.inner().metadata()?;
    if before.len() > limits.archive_bytes {
        return Err(ArchiveError::Limit("archive bytes"));
    }
    let mut copy = MeteredIo::new(
        File::options()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&copy_path)?,
    );
    let fingerprint = stream::copy_exact(&mut source, &mut copy, before.len(), &mut keep_going)?;
    let after = source.inner().metadata()?;
    if before.len() != after.len() || before.modified()? != after.modified()? {
        return Err(invalid("Source changed during copy"));
    }
    copy.rewind()?;
    let mut zip = ZipArchive::new(copy)?;
    // P1 must move admission before constructing ZipArchive, and must validate
    // every local header against the central directory. Do not expose to UI yet.
    if zip.len() > limits.entries {
        return Err(ArchiveError::Limit("entries"));
    }
    if zip.has_overlapping_entries()? {
        return Err(invalid("Overlapping ZIP entries"));
    }
    let mut names = BTreeSet::new();
    let mut expanded = 0;
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        let name = std::str::from_utf8(entry.name_raw())
            .map_err(|_| invalid("Invalid UTF-8 entry name"))?;
        if !names.insert(name.to_owned()) {
            return Err(invalid("Duplicate ZIP entry"));
        }
        if entry.encrypted()
            || !matches!(
                entry.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
            || entry
                .unix_mode()
                .is_some_and(|m| !matches!(m & 0o170000, 0 | 0o100000 | 0o040000))
        {
            return Err(invalid("Unsupported ZIP entry type"));
        }
        if !matches!(name, "format.json" | "project.json")
            && asset_hash(name).is_none()
            && !(name == "assets/" && entry.size() == 0 && entry.is_dir())
        {
            return Err(invalid(format!("Unknown ZIP entry: {name}")));
        }
        if name != "assets/" && !entry.is_file() {
            return Err(invalid("Expected regular entry"));
        }
        add(&mut expanded, entry.size(), limits.expanded_bytes)?;
    }
    let format: Format =
        serde_json::from_slice(&read_json(&mut zip, "format.json", limits.format_bytes)?)?;
    if format.format != "zipzi-project-package"
        || format.container_version != 1
        || format.document != "project.json"
        || format.required_features != [EXPERIMENT]
    {
        return Err(invalid("Unsupported prototype format/version/feature"));
    }
    let bytes = read_json(&mut zip, "project.json", limits.document_bytes)?;
    if content_sha256(&bytes) != format.document_sha256 {
        return Err(invalid("Document hash mismatch"));
    }
    let document = decode_project(
        std::str::from_utf8(&bytes).map_err(|_| invalid("Invalid UTF-8 document"))?,
    )?;
    let mut assets = BTreeMap::<String, Vec<&zipzi_core::ImageAsset>>::new();
    for id in document.asset_order() {
        let asset = document.get_asset(id).unwrap();
        let Some(content) = &asset.content else {
            continue;
        };
        if asset_hash(&content.source) != Some(content.sha256.as_str()) {
            return Err(invalid("Invalid content-addressed source"));
        }
        assets
            .entry(content.source.clone())
            .or_default()
            .push(asset);
    }
    if names
        .iter()
        .any(|n| asset_hash(n).is_some() && !assets.contains_key(n))
    {
        return Err(invalid("Unreferenced ZIP resource"));
    }
    fs::create_dir(working_set.path().join("assets"))?;
    let mut asset_bytes = 0;
    for (name, references) in &assets {
        check(&mut keep_going)?;
        let mut entry = zip.by_name(name)?;
        let size = entry.size();
        // `name` was checked against the fixed full-hash grammar above.
        let target = working_set.path().join(name);
        let mut output = File::options().write(true).create_new(true).open(&target)?;
        let actual = stream::copy_exact(&mut entry, &mut output, size, &mut keep_going)?;
        if actual.sha256 != asset_hash(name).unwrap() {
            return Err(invalid("Resource hash mismatch"));
        }
        asset_bytes += size;
        drop(output);
        for asset in references {
            read_project_texture_bounded(
                working_set.path(),
                asset,
                zipzi_core::image::PngDecodeLimits::default(),
                &mut keep_going,
            )?;
        }
    }
    let copy = zip.into_inner();
    let mut counts = source.counts() + copy.counts();
    // Bounded validation rereads each asset reference. Decoder reads from the
    // acquired bytes in memory, so it adds no further filesystem traffic.
    for (name, references) in &assets {
        counts.read_bytes +=
            fs::metadata(working_set.path().join(name))?.len() * references.len() as u64;
    }
    counts.write_bytes += asset_bytes;
    let unique_assets = assets.len();
    drop(assets);
    drop(copy);
    fs::remove_file(copy_path)?;
    check(&mut keep_going)?;
    Ok(OpenedArchive {
        document,
        fingerprint: fingerprint.clone(),
        counts,
        unique_assets,
        asset_bytes,
        peak_working_bytes: fingerprint.length + asset_bytes,
        working_set,
    })
}

#[cfg(test)]
mod tests;

use super::*;
use std::collections::BTreeSet;

fn entry_bytes<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
    limit: u64,
    keep_going: &mut impl FnMut() -> bool,
) -> Result<Vec<u8>> {
    let mut entry = zip.by_name(name)?;
    if entry.size() > limit {
        return Err(ArchiveError::Limit("JSON bytes"));
    }
    let size = entry.size();
    let mut bytes = Vec::new();
    stream::copy_exact(&mut entry, &mut bytes, size, keep_going)?;
    Ok(bytes)
}
fn unsupported(code: &str, message: &str) -> ArchiveError {
    Status::error(code, message).into()
}

pub(super) fn open_content(
    path: &Path,
    workspace_parent: &Path,
    limits: ArchiveIoLimits,
    disk_budget: ProjectDiskBudget,
    memory: Option<&crate::TransferReservation>,
    mut keep_going: impl FnMut() -> bool,
) -> Result<OpenedContent> {
    check(&mut keep_going)?;
    if let Some(memory) = memory {
        memory.ensure_at_least(2 << 20)?;
    }
    crate::filesystem::reject_symlink(path)?;
    let path = crate::filesystem::local_path(path)?;
    if !fs::symlink_metadata(&path)?.is_file() {
        return Err(Status::error(
            "UNSUPPORTED_PROJECT_PATH",
            "Expected a project file; directory projects are unsupported",
        )
        .into());
    }
    let mut source = MeteredIo::new(File::open(&path)?);
    let before = source.inner().metadata()?;
    if before.len() > limits.archive_bytes {
        return Err(ArchiveError::Limit("archive bytes"));
    }
    let working_set = ProjectResourceLease::create(workspace_parent, disk_budget)?;
    let copy_reservation = working_set.reserve(before.len())?;
    let copy_path = working_set.path().join("input.zip");
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
        return Err(invalid("Source changed during copy; retry opening"));
    }
    let names = structure::validate_budgeted(
        &mut copy,
        fingerprint.length,
        limits,
        &mut keep_going,
        memory,
    )?;
    let decode_workspace = memory.map_or(0, crate::TransferReservation::reserved_bytes);
    let mut zip = ZipArchive::new(copy)?;
    // Verify the library indexed exactly the admitted raw paths; no name override
    // extra field can redirect a lookup after preflight.
    let indexed: BTreeSet<_> = zip
        .file_names()
        .map(|name| name.map(|s| s.into_owned()))
        .collect::<std::result::Result<_, _>>()?;
    if indexed != names || zip.len() != names.len() {
        return Err(invalid("ZIP index differs from admitted directory"));
    }
    let format_bytes = entry_bytes(
        &mut zip,
        "format.json",
        limits.format_bytes,
        &mut keep_going,
    )?;
    crate::codec::validate_json_syntax(
        std::str::from_utf8(&format_bytes).map_err(|_| invalid("Invalid UTF-8 JSON"))?,
    )?;
    let format: Format = serde_json::from_slice(&format_bytes)?;
    drop(format_bytes);
    if format.format != "zipzi-project-package" || format.document != "project.json" {
        return Err(invalid("Not a Zipzi project package"));
    }
    if format.container_version != 1 {
        return Err(unsupported(
            "UNSUPPORTED_CONTAINER_VERSION",
            "Unsupported project container version",
        ));
    }
    if !format.required_features.is_empty() {
        return Err(unsupported(
            "UNSUPPORTED_PROJECT_FEATURE",
            "Unsupported required project feature",
        ));
    }
    // Preview pixels are advisory. A bad PNG must not prevent editing the
    // document, but malformed ZIP regions still fail the structural preflight.
    if names.contains(zipzi_project_preview::PREVIEW_PATH) && format.preview.is_none() {
        return Err(invalid("Undeclared project preview"));
    }
    let bytes = entry_bytes(
        &mut zip,
        "project.json",
        limits.document_bytes,
        &mut keep_going,
    )?;
    if content_sha256(&bytes) != format.document_sha256 {
        return Err(invalid("Document hash mismatch"));
    }
    let document = crate::codec::decode_state(
        std::str::from_utf8(&bytes).map_err(|_| invalid("Invalid UTF-8 document"))?,
    )?;
    drop(bytes);
    check(&mut keep_going)?;
    let mut assets = BTreeMap::<String, Vec<&zipzi_core::ImageAsset>>::new();
    let mut diagnostics = Vec::new();
    for asset in &document.images {
        let id = &asset.id;
        let Some(content) = &asset.content else {
            diagnostics.push(crate::ResourceDiagnostic {
                asset_id: id.to_string(),
                code: "MISSING_IMAGE_CONTENT".into(),
                message: "Asset has no image content".into(),
            });
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
    // All present resources are document-owned, including assets unused by meshes.
    if names
        .iter()
        .any(|n| asset_hash(n).is_some() && !assets.contains_key(n))
    {
        return Err(invalid("Unreferenced ZIP resource"));
    }
    let mut asset_bytes = 0;
    let mut decoded_read_bytes = 0;
    let mut unique_assets = 0;
    for (name, references) in &assets {
        check(&mut keep_going)?;
        let mut report = |code: &str, message: &str| {
            for asset in references {
                diagnostics.push(crate::ResourceDiagnostic {
                    asset_id: asset.id.to_string(),
                    code: code.into(),
                    message: message.into(),
                });
            }
        };
        if !names.contains(name) {
            report("RESOURCE_IO", "Declared image is missing from the project");
            continue;
        }
        let mut entry = zip.by_name(name)?;
        let size = entry.size();
        let png_limits = zipzi_core::image::PngDecodeLimits::default();
        if size > png_limits.max_encoded_bytes as u64 {
            report("ENCODED_READ_LIMIT", "Image exceeds encoded PNG limit");
            continue;
        }
        let reservation = working_set.reserve(size)?;
        let mut candidate = tempfile::NamedTempFile::new_in(working_set.path())?;
        let actual = match stream::copy_exact(&mut entry, &mut candidate, size, &mut keep_going) {
            Ok(actual) => actual,
            Err(ArchiveError::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::InvalidData
                        | std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::Other
                ) =>
            {
                // zip's CRC/deflate readers use InvalidData or Other. Disk write
                // failures (ENOSPC, access denied, etc.) retain their OS error.
                if e.raw_os_error().is_some() {
                    return Err(e.into());
                }
                report(
                    "RESOURCE_INVALID",
                    &format!("Image CRC or compressed data is invalid: {e}"),
                );
                continue;
            }
            Err(e) => return Err(e),
        };
        if actual.sha256 != asset_hash(name).unwrap() {
            report(
                "RESOURCE_HASH",
                "Image SHA-256 differs from its declaration",
            );
            continue;
        }
        let mut valid = false;
        // A private extracted PNG has immutable, verified bytes. Reuse only a
        // successful decode with these exact declared dimensions; mismatched
        // references still pass through bounded acquisition and diagnostics.
        let mut verified_dimensions = None;
        for asset in references {
            check(&mut keep_going)?;
            let content = asset.content.as_ref().unwrap();
            if verified_dimensions == Some((content.width, content.height)) {
                valid = true;
                continue;
            }
            if let Some(memory) = memory {
                if let Ok(required) = zipzi_core::image::png_decode_workspace_bytes(
                    content.width,
                    content.height,
                    size as usize,
                ) {
                    if required <= png_limits.max_workspace_bytes {
                        memory.ensure_at_least(
                            decode_workspace
                                .checked_add(required)
                                .ok_or(ArchiveError::Limit("PNG workspace overflow"))?,
                        )?;
                    }
                }
            }
            let mut temporary = (*asset).clone();
            temporary.content.as_mut().unwrap().source =
                candidate.path().to_string_lossy().into_owned();
            match read_project_texture_bounded(
                working_set.path(),
                &temporary,
                png_limits,
                &mut keep_going,
            ) {
                Ok(_) => {
                    verified_dimensions = Some((content.width, content.height));
                    valid = true;
                    decoded_read_bytes += size;
                }
                Err(status) if status.code == "CANCELLED" => return Err(ArchiveError::Cancelled),
                Err(status) => diagnostics.push(crate::ResourceDiagnostic {
                    asset_id: asset.id.to_string(),
                    code: status.code,
                    message: status.message,
                }),
            }
        }
        if valid {
            candidate
                .persist_noclobber(working_set.path().join(name))
                .map_err(|e| e.error)?;
            working_set.retain(reservation);
            asset_bytes += size;
            unique_assets += 1;
        }
    }
    let copy = zip.into_inner();
    let mut counts = source.counts() + copy.counts();
    counts.read_bytes += decoded_read_bytes;
    counts.write_bytes += asset_bytes;
    drop(assets);
    drop(copy);
    fs::remove_file(copy_path)?;
    drop(copy_reservation);
    check(&mut keep_going)?;
    Ok(OpenedContent {
        document,
        fingerprint: fingerprint.clone(),
        counts,
        unique_assets,
        asset_bytes,
        peak_working_bytes: fingerprint.length + asset_bytes,
        diagnostics,
        working_set,
    })
}

pub(super) struct OpenedContent {
    pub document: crate::codec::DecodedState,
    pub fingerprint: ProjectFingerprint,
    pub counts: IoCounts,
    pub unique_assets: usize,
    pub asset_bytes: u64,
    pub peak_working_bytes: u64,
    pub diagnostics: Vec<crate::ResourceDiagnostic>,
    pub working_set: ProjectResourceLease,
}

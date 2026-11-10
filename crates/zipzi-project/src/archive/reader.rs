use super::decode_workspace::{document_decode_workspace, document_freeze_workspace};
use super::*;
use crate::{ImageBufferKind, ImageResource, PreparedImageContent, ResourceStore};
use std::collections::BTreeSet;
use zipzi_core::image::{decode_png_bounded, png_decode_workspace_bytes, png_dimensions};

fn entry_bytes<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    name: &str,
    limit: u64,
    memory: Option<&crate::TransferReservation>,
    index_workspace: usize,
    keep_going: &mut impl FnMut() -> bool,
) -> Result<Vec<u8>> {
    let mut entry = zip.by_name(name)?;
    if entry.size() > limit {
        return Err(ArchiveError::Limit("JSON bytes"));
    }
    let size = usize::try_from(entry.size()).map_err(|_| ArchiveError::Limit("JSON bytes"))?;
    if let Some(memory) = memory {
        let scale = if name == "format.json" { 64 } else { 1 };
        memory.ensure_at_least(
            size.checked_mul(scale)
                .and_then(|n| index_workspace.checked_add(n))
                .ok_or(ArchiveError::Limit("JSON workspace overflow"))?,
        )?;
    }
    let mut bytes = Vec::with_capacity(size);
    stream::copy_exact(&mut entry, &mut bytes, size as u64, keep_going)?;
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
    resources: &ResourceStore,
    mut keep_going: impl FnMut() -> bool,
) -> Result<OpenedContent> {
    check(&mut keep_going)?;
    let reservation_floor = memory.map_or(0, crate::TransferReservation::reserved_bytes);
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
    let (names, index_workspace) = structure::validate_budgeted(
        &mut copy,
        fingerprint.length,
        limits,
        &mut keep_going,
        memory,
    )?;
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
        memory,
        index_workspace,
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
        memory,
        index_workspace,
        &mut keep_going,
    )?;
    if content_sha256(&bytes) != format.document_sha256 {
        return Err(invalid("Document hash mismatch"));
    }
    if let Some(memory) = memory {
        memory.ensure_at_least(
            index_workspace
                .checked_add(document_decode_workspace(&bytes, &mut keep_going)?)
                .ok_or(ArchiveError::Limit("document workspace overflow"))?,
        )?;
    }
    let document = crate::codec::decode_state(
        std::str::from_utf8(&bytes).map_err(|_| invalid("Invalid UTF-8 document"))?,
    )?;
    drop(bytes);
    check(&mut keep_going)?;
    // Keep the owned model and headroom for freezing it, then reuse the rest
    // of the budget for image decoding and captured pixels.
    let model_workspace = match memory {
        Some(memory) => {
            let workspace =
                index_workspace.saturating_add(document_freeze_workspace(&document.model));
            memory.ensure_at_least(workspace)?;
            memory.shrink_to(workspace.max(reservation_floor));
            workspace
        }
        None => 0,
    };
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
    let mut images = BTreeMap::new();
    let budget = resources.budget();
    let shared_budget = memory.is_some_and(|memory| memory.shares_budget(budget));
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
        let encoded_bytes = size as usize * 2;
        let encoded = budget.reserve(encoded_bytes)?;
        let mut bytes = Vec::with_capacity(size as usize);
        let actual = match stream::copy_exact(&mut entry, &mut bytes, size, &mut keep_going) {
            Ok(actual) => actual,
            Err(ArchiveError::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::InvalidData
                        | std::io::ErrorKind::UnexpectedEof
                        | std::io::ErrorKind::Other
                ) =>
            {
                // ZIP CRC/deflate readers use InvalidData or Other. Preserve
                // underlying OS I/O failures.
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
        // The entry now has immutable, verified encoded bytes. Reuse only a
        // successful decode with these exact declared dimensions; mismatched
        // references still pass through bounded acquisition and diagnostics.
        let mut decoded = None;
        let dimensions = png_dimensions(&bytes);
        for asset in references {
            check(&mut keep_going)?;
            let content = asset.content.as_ref().unwrap();
            let expected = (content.width, content.height);
            if decoded.is_some() && dimensions.as_ref().is_ok_and(|&actual| actual == expected) {
                continue;
            }
            let admission =
                png_decode_workspace_bytes(content.width, content.height, size as usize).and_then(
                    |required| {
                        if required > png_limits.max_workspace_bytes {
                            return Err(Status::error(
                                "DECODE_WORKSPACE_LIMIT",
                                format!(
                                    "{}: PNG decode requires {required} bytes, limit {}",
                                    asset.id, png_limits.max_workspace_bytes
                                ),
                            ));
                        }
                        let actual = *dimensions.as_ref().map_err(Clone::clone)?;
                        if actual != expected {
                            return Err(Status::error(
                                "RESOURCE_DIMENSIONS",
                                format!(
                                    "PNG is {}x{}, expected {}x{}",
                                    actual.0, actual.1, expected.0, expected.1
                                ),
                            ));
                        }
                        Ok(required)
                    },
                );
            let required = match admission {
                Ok(required) => required,
                Err(status) => {
                    diagnostics.push(crate::ResourceDiagnostic {
                        asset_id: asset.id.to_string(),
                        code: status.code,
                        message: status.message,
                    });
                    continue;
                }
            };
            let pixel_bytes = crate::store::rgba_size(content.width, content.height)? * 2;
            if let Some(memory) = memory {
                // The PNG peak already covers both buffer reservations. Charge
                // only the remainder when they draw from the same host budget.
                let workspace = if shared_budget {
                    required - encoded_bytes - pixel_bytes
                } else {
                    required
                };
                memory.ensure_at_least(
                    model_workspace
                        .checked_add(workspace)
                        .ok_or(ArchiveError::Limit("PNG workspace overflow"))?,
                )?;
            }
            let pixels = budget.reserve(pixel_bytes)?;
            match decode_png_bounded(&bytes, expected, png_limits) {
                Ok(image) => decoded = Some((image, pixels)),
                Err(status) => diagnostics.push(crate::ResourceDiagnostic {
                    asset_id: asset.id.to_string(),
                    code: status.code,
                    message: status.message,
                }),
            }
        }
        if let Some((image, pixels)) = decoded {
            check(&mut keep_going)?;
            let bytes: Arc<[u8]> = bytes.into();
            let rgba: Arc<[u8]> = image.rgba.into();
            encoded.commit(&bytes, ImageBufferKind::Png)?;
            pixels.commit(&rgba, ImageBufferKind::Rgba)?;
            let image = PreparedImageContent::from_decoded_png(
                bytes,
                actual.sha256,
                image.width,
                image.height,
                rgba,
                budget,
            );
            images.insert(name.clone(), resources.adopt_image(image)?);
            asset_bytes += size;
        }
        if let Some(memory) = memory {
            memory.shrink_to(model_workspace.max(reservation_floor));
        }
    }
    let copy = zip.into_inner();
    let counts = source.counts() + copy.counts();
    drop(assets);
    drop(copy);
    fs::remove_file(copy_path)?;
    drop(copy_reservation);
    check(&mut keep_going)?;
    Ok(OpenedContent {
        document,
        fingerprint: fingerprint.clone(),
        counts,
        unique_assets: images.len(),
        asset_bytes,
        peak_working_bytes: fingerprint.length,
        diagnostics,
        images,
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
    pub images: BTreeMap<String, ImageResource>,
}

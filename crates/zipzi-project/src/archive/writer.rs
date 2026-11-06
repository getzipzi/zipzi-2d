use super::*;

pub(super) struct PreparedArchive<D = Document> {
    result: WrittenArchive<D>,
    resources: BTreeMap<String, Resource>,
    json: Vec<u8>,
    format: Vec<u8>,
    pub extent: u64,
    preview: Option<ProjectPreview>,
}

impl<D> PreparedArchive<D> {
    pub fn preview_format(&self) -> Vec<u8> {
        self.format.clone()
    }

    pub fn omit_preview(&mut self, format: Vec<u8>, extent: u64) {
        self.format = format;
        self.extent = extent;
        self.preview = None;
    }

    pub fn set_preview(
        &mut self,
        preview: Option<ProjectPreview>,
        limits: ArchiveIoLimits,
    ) -> Result<()> {
        if let Some(preview) = preview {
            if self.resources.len() + 3 > limits.entries {
                return Ok(());
            }
            let mut format: Format = serde_json::from_slice(&self.format)?;
            format.preview = Some(serde_json::to_value(preview.descriptor())?);
            let format = serde_json::to_vec(&format)?;
            if format.len() as u64 > limits.format_bytes {
                return Ok(());
            }
            let expanded = self.result.asset_bytes
                + self.json.len() as u64
                + format.len() as u64
                + preview.png().len() as u64;
            if expanded > limits.expanded_bytes {
                return Ok(());
            }
            let extent = self
                .extent
                .checked_add(preview.png().len() as u64 + 2048)
                .ok_or(ArchiveError::Limit("archive extent"))?;
            if extent > limits.archive_bytes {
                return Ok(());
            }
            self.extent = extent;
            self.format = format;
            self.preview = Some(preview);
        }
        Ok(())
    }
}

pub(super) fn prepare(
    document: &Document,
    asset_root: &Path,
    limits: ArchiveIoLimits,
    mut keep_going: impl FnMut() -> bool,
    format_kind: ArchiveDocumentFormat,
) -> Result<PreparedArchive> {
    check(&mut keep_going)?;
    if document.transaction_active() {
        return Err(Status::error("TRANSACTION_ACTIVE", "Commit or cancel edits first").into());
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
            let fingerprint = crate::archive::read_fingerprint(&mut file, size, &mut keep_going)?;
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
                        input: ResourceInput::File(path.clone()),
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
        let expected = candidate.get_asset(&asset.id).unwrap().clone();
        let content = asset.content.as_ref().unwrap();
        let result = candidate.relocate_asset_storage(
            &expected,
            content.source.clone(),
            content.sha256.clone(),
        );
        if !result.status.is_ok() {
            return Err(result.status.into());
        }
    }
    let json = match format_kind {
        ArchiveDocumentFormat::NativeV22 => crate::codec::encode_native_project(&candidate)?,
        #[cfg(any(test, feature = "archive-prototype"))]
        ArchiveDocumentFormat::Prototype => encode_project(&candidate)?.into_bytes(),
    };
    candidate.mark_saved();
    prepare_encoded(
        candidate,
        resources,
        json,
        asset_bytes,
        read_bytes,
        limits,
        format_kind,
    )
}
fn prepare_encoded<D>(
    document: D,
    resources: BTreeMap<String, Resource>,
    json: Vec<u8>,
    asset_bytes: u64,
    read_bytes: u64,
    limits: ArchiveIoLimits,
    format_kind: ArchiveDocumentFormat,
) -> Result<PreparedArchive<D>> {
    if resources
        .len()
        .checked_add(2)
        .is_none_or(|n| n > limits.entries)
    {
        return Err(ArchiveError::Limit("entries"));
    }
    if json.len() as u64 > limits.document_bytes {
        return Err(ArchiveError::Limit("document bytes"));
    }
    let format = serde_json::to_vec(&Format {
        format: "zipzi-project-package".into(),
        container_version: 1,
        document: "project.json".into(),
        document_sha256: content_sha256(&json),
        required_features: match format_kind {
            #[cfg(any(test, feature = "archive-prototype"))]
            ArchiveDocumentFormat::Prototype => vec![EXPERIMENT.into()],
            ArchiveDocumentFormat::NativeV22 => Vec::new(),
        },
        preview: None,
    })?;
    if format.len() as u64 > limits.format_bytes {
        return Err(ArchiveError::Limit("format bytes"));
    }
    let mut expanded = asset_bytes;
    add(&mut expanded, json.len() as u64, limits.expanded_bytes)?;
    add(&mut expanded, format.len() as u64, limits.expanded_bytes)?;
    // Stored assets plus a conservative Deflate bound for JSON and fixed ZIP
    // metadata. Reserve before allocating the staging file; enforce the same
    // extent on every write, including finalization.
    let extent = asset_bytes
        .checked_add((json.len() as u64 + format.len() as u64).saturating_mul(2))
        .and_then(|n| n.checked_add((resources.len() as u64 + 2).saturating_mul(1024)))
        .and_then(|n| n.checked_add(65536))
        .ok_or(ArchiveError::Limit("archive extent"))?
        .min(limits.archive_bytes);
    Ok(PreparedArchive {
        result: WrittenArchive {
            document,
            unique_assets: resources.len(),
            asset_bytes,
            document_bytes: json.len() as u64,
            resource_read_bytes: read_bytes,
        },
        resources,
        json,
        format,
        extent,
        preview: None,
    })
}

pub(super) fn write<W: Write + Seek, D>(
    prepared: PreparedArchive<D>,
    output: &mut W,
    mut keep_going: impl FnMut() -> bool,
    progress: Option<&ProjectSaveProgress>,
) -> Result<WrittenArchive<D>> {
    if output.stream_position()? != 0 {
        return Err(invalid("Writer must start at offset zero"));
    }
    let PreparedArchive {
        mut result,
        resources,
        json,
        format,
        extent,
        preview,
    } = prepared;
    if let Some(progress) = progress {
        progress.writing(
            result.asset_bytes
                + json.len() as u64
                + format.len() as u64
                + preview.as_ref().map_or(0, |p| p.png().len() as u64),
        );
    }
    let mut bounded = stream::BoundedWriter::new(output, extent);
    let mut zip = ZipWriter::new(&mut bounded);
    zip.start_file("format.json", options(CompressionMethod::Deflated))?;
    progress::ProgressWriter {
        writer: &mut zip,
        progress,
    }
    .write_all(&format)?;
    if let Some(preview) = &preview {
        check(&mut keep_going)?;
        zip.start_file(
            zipzi_project_preview::PREVIEW_PATH,
            options(CompressionMethod::Stored),
        )?;
        progress::ProgressWriter {
            writer: &mut zip,
            progress,
        }
        .write_all(preview.png())?;
    }
    zip.start_file("project.json", options(CompressionMethod::Deflated))?;
    progress::ProgressWriter {
        writer: &mut zip,
        progress,
    }
    .write_all(&json)?;
    for (hash, resource) in &resources {
        check(&mut keep_going)?;
        zip.start_file(
            format!("assets/{hash}.png"),
            options(CompressionMethod::Stored),
        )?;
        let mut input: Box<dyn Read + '_> = match &resource.input {
            ResourceInput::File(path) => Box::new(File::open(path)?),
            ResourceInput::Bytes(bytes) => Box::new(std::io::Cursor::new(bytes.as_ref())),
        };
        let actual = stream::copy_exact(
            &mut input,
            &mut progress::ProgressWriter {
                writer: &mut zip,
                progress,
            },
            resource.fingerprint.length,
            &mut keep_going,
        )?;
        result.resource_read_bytes += actual.length;
        if actual != resource.fingerprint {
            return Err(invalid("Resource changed while writing"));
        }
    }
    check(&mut keep_going)?;
    if let Some(progress) = progress {
        progress.set_stage(ProjectSaveStage::Finishing);
    }
    zip.finish()?;
    drop(bounded);
    if output.stream_position()? > extent {
        return Err(ArchiveError::Limit("archive bytes"));
    }
    check(&mut keep_going)?;
    Ok(result)
}

pub(super) fn prepare_state(
    state: &zipzi_core::state::DocumentState,
    view: &crate::ResourceView,
    limits: ArchiveIoLimits,
    mut keep_going: impl FnMut() -> bool,
) -> Result<PreparedArchive<()>> {
    use zipzi_core::{state::AssetContent, ImageAsset, ImageContent};
    let view = view.for_state(state)?;
    let mut images = Vec::new();
    let mut resources = BTreeMap::new();
    let mut asset_bytes = 0;
    for id in state.asset_order() {
        check(&mut keep_going)?;
        let asset = state.asset(id).unwrap();
        let mut image = ImageAsset {
            id: id.clone(),
            name: asset.name.clone(),
            ..Default::default()
        };
        match &asset.content {
            AssetContent::Empty => (),
            AssetContent::Unresolved { source } => image.unresolved_source = Some(source.clone()),
            AssetContent::Image {
                resource,
                width,
                height,
            } => {
                let (bytes, hash) = view
                    .image(*resource)
                    .expect("validated resource")
                    .encoded_png()?;
                image.content = Some(ImageContent {
                    source: format!("assets/{hash}.png"),
                    width: *width,
                    height: *height,
                    sha256: hash.clone(),
                });
                if !resources.contains_key(&hash) {
                    add(&mut asset_bytes, bytes.len() as u64, limits.expanded_bytes)?;
                    resources.insert(
                        hash.clone(),
                        Resource {
                            fingerprint: ProjectFingerprint {
                                length: bytes.len() as u64,
                                sha256: hash,
                            },
                            input: ResourceInput::Bytes(bytes),
                            dimensions: (*width, *height),
                        },
                    );
                }
            }
        }
        images.push(image);
    }
    let json = crate::codec::encode_state(state, images)?;
    prepare_encoded(
        (),
        resources,
        json,
        asset_bytes,
        0,
        limits,
        ArchiveDocumentFormat::NativeV22,
    )
}

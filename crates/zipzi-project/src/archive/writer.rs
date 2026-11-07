use super::*;

pub(super) struct PreparedArchive {
    result: WrittenArchive,
    resources: BTreeMap<String, Resource>,
    json: Vec<u8>,
    format: Vec<u8>,
    pub extent: u64,
    preview: Option<ProjectPreview>,
}

impl PreparedArchive {
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

fn prepare_encoded(
    resources: BTreeMap<String, Resource>,
    json: Vec<u8>,
    asset_bytes: u64,
    limits: ArchiveIoLimits,
) -> Result<PreparedArchive> {
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
        required_features: Vec::new(),
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
            #[cfg(test)]
            unique_assets: resources.len(),
            asset_bytes,
            document_bytes: json.len() as u64,
        },
        resources,
        json,
        format,
        extent,
        preview: None,
    })
}

pub(super) fn write<W: Write + Seek>(
    prepared: PreparedArchive,
    output: &mut W,
    mut keep_going: impl FnMut() -> bool,
    progress: Option<&ProjectSaveProgress>,
) -> Result<WrittenArchive> {
    if output.stream_position()? != 0 {
        return Err(invalid("Writer must start at offset zero"));
    }
    let PreparedArchive {
        result,
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
) -> Result<PreparedArchive> {
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
                        },
                    );
                }
            }
        }
        images.push(image);
    }
    let json = crate::codec::encode_state(state, images)?;
    prepare_encoded(resources, json, asset_bytes, limits)
}

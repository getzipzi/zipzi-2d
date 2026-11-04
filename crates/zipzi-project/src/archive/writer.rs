use super::*;

pub(super) struct PreparedArchive {
    result: WrittenArchive,
    resources: BTreeMap<String, Resource>,
    json: Vec<u8>,
    format: Vec<u8>,
    pub extent: u64,
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
    let json = match format_kind {
        ArchiveDocumentFormat::NativeV22 => crate::codec::encode_native_project(&candidate)?,
        #[cfg(any(test, feature = "archive-prototype"))]
        ArchiveDocumentFormat::Prototype => encode_project(&candidate)?.into_bytes(),
    };
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
    candidate.mark_saved();
    Ok(PreparedArchive {
        result: WrittenArchive {
            document: candidate,
            unique_assets: resources.len(),
            asset_bytes,
            document_bytes: json.len() as u64,
            resource_read_bytes: read_bytes,
        },
        resources,
        json,
        format,
        extent,
    })
}

pub(super) fn write<W: Write + Seek>(
    prepared: PreparedArchive,
    output: &mut W,
    mut keep_going: impl FnMut() -> bool,
) -> Result<WrittenArchive> {
    if output.stream_position()? != 0 {
        return Err(invalid("Writer must start at offset zero"));
    }
    let PreparedArchive {
        mut result,
        resources,
        json,
        format,
        extent,
    } = prepared;
    let mut bounded = stream::BoundedWriter::new(output, extent);
    let mut zip = ZipWriter::new(&mut bounded);
    zip.start_file("format.json", options(CompressionMethod::Deflated))?;
    zip.write_all(&format)?;
    zip.start_file("project.json", options(CompressionMethod::Deflated))?;
    zip.write_all(&json)?;
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
        result.resource_read_bytes += actual.length;
        if actual != resource.fingerprint {
            return Err(invalid("Resource changed while writing"));
        }
    }
    check(&mut keep_going)?;
    zip.finish()?;
    drop(bounded);
    if output.stream_position()? > extent {
        return Err(ArchiveError::Limit("archive bytes"));
    }
    check(&mut keep_going)?;
    Ok(result)
}

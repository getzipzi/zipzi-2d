//! Native transfer adapters: fixed resource bytes, linked project imports and
//! complete grouped output. The editor never performs a second source read.
use crate::{AssetData, DocumentSession, Publication};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};
use zipzi_core::{Document, Status};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferFormat {
    Runtime,
    Cubism,
}

pub struct TransferSnapshot {
    document: Document,
    resources: HashMap<String, CapturedAsset>,
    root: PathBuf,
    protected_paths: Vec<PathBuf>,
}
enum CapturedAsset {
    Encoded(AssetData),
    Pixels {
        texture: crate::TextureData,
        encoded: std::sync::OnceLock<Result<AssetData, Status>>,
    },
}
impl CapturedAsset {
    fn data(&self) -> Result<&AssetData, Status> {
        match self {
            Self::Encoded(data) => Ok(data),
            Self::Pixels { texture, encoded } => encoded
                .get_or_init(|| {
                    let mut bytes = Vec::new();
                    {
                        let mut encoder =
                            png::Encoder::new(&mut bytes, texture.width, texture.height);
                        encoder.set_color(png::ColorType::Rgba);
                        encoder.set_depth(png::BitDepth::Eight);
                        encoder
                            .write_header()
                            .map_err(|e| Status::error("PNG_ENCODE", e.to_string()))?
                            .write_image_data(&texture.rgba)
                            .map_err(|e| Status::error("PNG_ENCODE", e.to_string()))?;
                    }
                    Ok(AssetData {
                        sha256: crate::content_sha256(&bytes),
                        bytes,
                        rgba: Vec::new(),
                        width: texture.width,
                        height: texture.height,
                    })
                })
                .as_ref()
                .map_err(Clone::clone),
        }
    }
}
fn codec(e: zipzi_cmo3::Cmo3Error) -> Status {
    Status::error(
        "CUBISM_TRANSFER",
        match e {
            zipzi_cmo3::Cmo3Error::Unsupported(report) => report
                .diagnostics
                .iter()
                .filter(|d| matches!(d.severity, zipzi_cmo3::DiagnosticSeverity::Loss))
                .map(|d| format!("{}: {} ({})", d.code, d.message, d.object_id))
                .collect::<Vec<_>>()
                .join("\n"),
            e => e.to_string(),
        },
    )
}
impl DocumentSession {
    pub fn transfer_snapshot(&self) -> Result<TransferSnapshot, Status> {
        let mut resources = HashMap::new();
        let mut protected_paths = self.import_sources.clone();
        let root = self.root();
        if !root.as_os_str().is_empty() {
            protected_paths.push(
                fs::canonicalize(&root).map_err(|e| Status::error("RESOURCE_IO", e.to_string()))?,
            );
        }
        for id in self.document().asset_order() {
            let asset = self.document().get_asset(id).unwrap();
            let captured = if self.has_memory_asset(asset) {
                CapturedAsset::Pixels {
                    texture: self.read_texture(id)?,
                    encoded: Default::default(),
                }
            } else {
                CapturedAsset::Encoded(self.read_asset(id)?)
            };
            resources.insert(id.clone(), captured);
            if !self.has_memory_asset(asset) {
                let p = Path::new(&asset.content.as_ref().unwrap().source);
                protected_paths.push(
                    fs::canonicalize(if p.is_absolute() {
                        p.to_path_buf()
                    } else {
                        root.join(p)
                    })
                    .map_err(|e| Status::error("RESOURCE_IO", e.to_string()))?,
                );
            }
        }
        Ok(TransferSnapshot {
            document: self.document().clone(),
            resources,
            root,
            protected_paths,
        })
    }
    pub fn import_cubism_authoring(&mut self, path: &Path) -> Result<(), Status> {
        if self.document().transaction_active() {
            return Err(Status::error("EDIT_ACTIVE", "Finish the active edit first"));
        }
        // Parsing overlaps with the encoded source and prepared image buffers.
        // Leave headroom for those allocations even when the host's remaining
        // budget is below the normal decoder workspace cap.
        let workspace = (self.image_memory_budget().available_bytes() / 2).min(512 << 20);
        let _decoded = self.image_memory_budget().reserve_transfer(workspace)?;
        let (model, animation) = linked_project_with_budget(path, workspace)?;
        let length = fs::metadata(&model)
            .map_err(|e| Status::error("CUBISM_IO", e.to_string()))?
            .len() as usize;
        let _input = self
            .image_memory_budget()
            .reserve_transfer(length.saturating_mul(2))?;
        let bytes = fs::read(&model).map_err(|e| Status::error("CUBISM_IO", e.to_string()))?;
        let mut imported =
            zipzi_cmo3::import_model_with_budget(&bytes, workspace).map_err(codec)?;
        if let Some(animation) = animation {
            zipzi_cmo3::import_animation_with_budget(&animation, &mut imported.document, workspace)
                .map_err(codec)?;
        }
        let mut candidate = DocumentSession::from_authoring_document(imported.document);
        candidate.import_sources = vec![
            model,
            fs::canonicalize(path).map_err(|e| Status::error("CUBISM_IO", e.to_string()))?,
        ];
        candidate.set_image_memory_budget(self.image_memory_budget().clone())?;
        let mut document = candidate.document().clone();
        for asset_id in document.asset_order().to_vec() {
            let old = document.get_asset(&asset_id).unwrap().clone();
            let bytes = imported
                .images
                .get(&old.content.as_ref().unwrap().source)
                .ok_or_else(|| Status::error("CUBISM_RESOURCE", "Missing imported image"))?;
            let image =
                crate::prepare_png_content(bytes.clone().into(), candidate.image_memory_budget())?;
            let mut asset = old;
            asset.content = Some(image.content().clone());
            asset.unresolved_source = None;
            let result = document.replace_asset(asset);
            if !result.status.is_ok() {
                return Err(result.status);
            }
            candidate.retain_prepared_image(&image);
        }
        // Source images may be original artwork or packed pages. Only known
        // atlas inputs and shared pages need extraction; keep independent originals.
        candidate.document_mut().clone_from(&document);
        candidate.unpack_atlas_images(&imported.atlas_assets)?;
        document = candidate.document().clone();
        // Keep the candidate resource store; publication replaces the session
        // only after every model, animation and texture has been validated.
        self.store_import_candidate(candidate, document);
        Ok(())
    }
}
impl TransferSnapshot {
    pub fn document(&self) -> &Document {
        &self.document
    }
    pub fn publish(
        self,
        format: TransferFormat,
        destination: &Path,
    ) -> Result<Publication, Status> {
        let parent = destination
            .parent()
            .ok_or_else(|| Status::error("INVALID_DESTINATION", "Missing output parent"))?;
        let resolved = fs::canonicalize(parent)
            .map_err(|e| Status::error("INVALID_DESTINATION", e.to_string()))?
            .join(
                destination
                    .file_name()
                    .ok_or_else(|| Status::error("INVALID_DESTINATION", "Missing output name"))?,
            );
        if self
            .protected_paths
            .iter()
            .any(|p| p.starts_with(&resolved))
        {
            return Err(Status::error(
                "INVALID_DESTINATION",
                "Output would replace source project or resources",
            ));
        }
        if let Ok(meta) = fs::symlink_metadata(destination) {
            if meta.file_type().is_symlink() {
                return Err(Status::error(
                    "INVALID_DESTINATION",
                    "Output must not be a symbolic link",
                ));
            }
        }
        match format {
            TransferFormat::Runtime => {
                let plan =
                    crate::package::build_export_plan_with(&self.document, &self.root, &|asset| {
                        self.resources
                            .get(&asset.id)
                            .ok_or_else(|| Status::error("MISSING_RESOURCE", &asset.id))?
                            .data()
                            .cloned()
                    })?;
                crate::package::publish_managed_export_plan(&plan, destination)
            }
            TransferFormat::Cubism => {
                let name = destination
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| Status::error("INVALID_DESTINATION", "Invalid output name"))?;
                let resolver = |asset: &zipzi_core::ImageAsset| {
                    let image = self.resources.get(&asset.id).ok_or_else(|| {
                        zipzi_cmo3::Cmo3Error::Asset {
                            asset_id: asset.id.clone(),
                            message: "Missing snapshot image".into(),
                        }
                    })?;
                    image.data().map(|data| data.bytes.clone()).map_err(|s| {
                        zipzi_cmo3::Cmo3Error::Asset {
                            asset_id: asset.id.clone(),
                            message: s.message,
                        }
                    })
                };
                let options = zipzi_cmo3::Cmo3Options {
                    model_name: self
                        .document
                        .model3_settings()
                        .extensions
                        .get("Name")
                        .and_then(|value| value.as_str())
                        .unwrap_or(name)
                        .into(),
                    ..Default::default()
                };
                if self.document.motion_order().is_empty()
                    && self.document.expression_order().is_empty()
                {
                    let artifact = zipzi_cmo3::encode_cmo3(&self.document, &resolver, &options)
                        .map_err(codec)?;
                    crate::cubism::publish_cubism_model(artifact, name, destination).map_err(codec)
                } else {
                    let artifact = zipzi_cmo3::encode_cubism(
                        &self.document,
                        &resolver,
                        &zipzi_cmo3::CubismOptions {
                            base_name: name.into(),
                            model: options,
                            bake_animation: true,
                            auto_frame_rate: true,
                        },
                    )
                    .map_err(codec)?;
                    crate::publish_cubism(&artifact, destination).map_err(codec)
                }
            }
        }
    }
}

pub fn resolve_model3_entry(path: &Path) -> Result<PathBuf, Status> {
    if path.extension().and_then(|s| s.to_str()) != Some("moc3") {
        return Ok(path.to_path_buf());
    }
    let selected = fs::canonicalize(path).map_err(|e| Status::error("MODEL3_IO", e.to_string()))?;
    let parent = path
        .parent()
        .ok_or_else(|| Status::error("MODEL3_ENTRY", "Missing model directory"))?;
    let mut matches = Vec::new();
    for entry in fs::read_dir(parent).map_err(|e| Status::error("MODEL3_IO", e.to_string()))? {
        let entry = entry.map_err(|e| Status::error("MODEL3_IO", e.to_string()))?;
        let candidate = entry.path();
        if !candidate
            .file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with(".model3.json"))
        {
            continue;
        }
        if let Ok(bytes) = fs::read(&candidate) {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                if let Some(reference) =
                    json.pointer("/FileReferences/Moc").and_then(|v| v.as_str())
                {
                    if fs::canonicalize(parent.join(reference)).ok().as_ref() == Some(&selected) {
                        matches.push(candidate);
                    }
                }
            }
        }
    }
    if matches.len() != 1 {
        return Err(Status::error(
            "MODEL3_ENTRY",
            format!(
                "Expected one model3 entry referencing {}, found {}",
                path.display(),
                matches.len()
            ),
        ));
    }
    Ok(matches.remove(0))
}
fn resolve_cubism_model(parent: &Path, reference: &str) -> Result<PathBuf, Status> {
    let portable = reference.replace('\\', "/");
    let reference = Path::new(&portable);
    let filename = reference
        .file_name()
        .ok_or_else(|| Status::error("CUBISM_ENTRY", "Invalid model reference"))?;
    // Absolute references and paths relative to the CAN3 take precedence over
    // relocation fallback. A same-named sibling may be a different model.
    let original = parent.join(reference);
    let candidates = [original, parent.join(filename)];
    for candidate in &candidates {
        match fs::metadata(candidate) {
            Ok(metadata) if metadata.is_file() => {
                return fs::canonicalize(candidate)
                    .map_err(|e| Status::error("CUBISM_IO", e.to_string()));
            }
            Ok(_) => {
                return Err(Status::error(
                    "CUBISM_ENTRY",
                    format!("Model reference is not a file: {}", candidate.display()),
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Status::error("CUBISM_IO", error.to_string())),
        }
    }
    Err(Status::error(
        "CUBISM_ENTRY",
        format!("Associated CMO3 not found: {}", candidates[0].display()),
    ))
}

#[cfg(test)]
fn linked_project(path: &Path) -> Result<(PathBuf, Option<Vec<u8>>), Status> {
    linked_project_with_budget(path, usize::MAX)
}
fn linked_project_with_budget(
    path: &Path,
    workspace: usize,
) -> Result<(PathBuf, Option<Vec<u8>>), Status> {
    let parent = path
        .parent()
        .ok_or_else(|| Status::error("CUBISM_ENTRY", "Missing project directory"))?;
    let extension = path.extension().and_then(|n| n.to_str());
    if extension == Some("can3") {
        let animation = read_cubism_animation(path, workspace)?;
        let paths =
            zipzi_cmo3::animation_model_paths_with_budget(&animation, workspace).map_err(codec)?;
        if paths.len() != 1 {
            return Err(Status::error(
                "CUBISM_ENTRY",
                "CAN3 must reference exactly one model",
            ));
        }
        let model = resolve_cubism_model(parent, &paths[0])?;
        return Ok((model, Some(animation)));
    }
    if extension != Some("cmo3") {
        return Err(Status::error("CUBISM_ENTRY", "Choose a CMO3 or CAN3 file"));
    }
    let selected = fs::canonicalize(path).map_err(|e| Status::error("CUBISM_IO", e.to_string()))?;
    let mut matches = Vec::new();
    for entry in fs::read_dir(parent).map_err(|e| Status::error("CUBISM_IO", e.to_string()))? {
        let entry = entry.map_err(|e| Status::error("CUBISM_IO", e.to_string()))?;
        let candidate = entry.path();
        if candidate.extension().and_then(|n| n.to_str()) != Some("can3") {
            continue;
        }
        let bytes = read_cubism_animation(&candidate, workspace)?;
        match zipzi_cmo3::animation_model_paths_with_budget(&bytes, workspace) {
            Ok(paths) => {
                if paths
                    .iter()
                    .any(|p| resolve_cubism_model(parent, p).is_ok_and(|model| model == selected))
                {
                    if paths.len() != 1 {
                        return Err(Status::error("CUBISM_ENTRY", "Multiple model references"));
                    }
                    if !matches.is_empty() {
                        return Err(Status::error(
                            "CUBISM_ENTRY",
                            "Multiple matching CAN3 files",
                        ));
                    }
                    matches.push(bytes);
                }
            }
            Err(e) if candidate.file_stem() == path.file_stem() => return Err(codec(e)),
            _ => {}
        }
    }
    if matches.len() > 1 {
        return Err(Status::error(
            "CUBISM_ENTRY",
            "Multiple matching CAN3 projects",
        ));
    }
    Ok((path.to_path_buf(), matches.pop()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cubism_import_leaves_room_for_source_bytes_with_a_bounded_host_budget() {
        let root =
            std::env::temp_dir().join(format!("zipzi-cubism-budget-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let mut doc = zipzi_core::Document::new();
        assert!(doc
            .initialize(
                &uuid::Uuid::new_v4().to_string(),
                zipzi_core::ModelSetup::default()
            )
            .is_ok());
        let source = DocumentSession::from_authoring_document(doc);
        let artifact = source.encode_cmo3(&Default::default()).unwrap();
        let path = root.join("model.cmo3");
        fs::write(&path, artifact.bytes).unwrap();
        let budget = crate::ImageMemoryBudget::new(128 << 20);
        budget.bind_host_budget(0, 128 << 20);
        let mut imported = DocumentSession::new();
        imported.set_image_memory_budget(budget.clone()).unwrap();
        let result = imported.import_cubism_authoring(&path);
        fs::remove_dir_all(root).unwrap();
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(imported.document().id(), source.document().id());
        assert_eq!(budget.usage().transfer_bytes, 0);
    }

    #[test]
    fn model_reference_prefers_original_relative_or_absolute_path_over_sibling() {
        let root =
            std::env::temp_dir().join(format!("zipzi-model-reference-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("models")).unwrap();
        let model = root.join("models/model.cmo3");
        let sibling = root.join("model.cmo3");
        fs::write(&model, b"referenced model").unwrap();
        fs::write(&sibling, b"different model").unwrap();
        for reference in [
            "models/model.cmo3",
            "models\\model.cmo3",
            model.to_str().unwrap(),
        ] {
            assert_eq!(
                resolve_cubism_model(&root, reference).unwrap(),
                fs::canonicalize(&model).unwrap()
            );
        }
        assert_eq!(
            resolve_cubism_model(&root.join("models"), "../model.cmo3").unwrap(),
            fs::canonicalize(&sibling).unwrap()
        );
        fs::remove_file(&model).unwrap();
        assert_eq!(
            resolve_cubism_model(&root, "models/model.cmo3").unwrap(),
            fs::canonicalize(&sibling).unwrap()
        );
        assert_eq!(
            resolve_cubism_model(&root, "C:\\old\\model.cmo3").unwrap(),
            fs::canonicalize(&sibling).unwrap()
        );
        fs::remove_file(&sibling).unwrap();
        assert_eq!(
            resolve_cubism_model(&root, "models/model.cmo3")
                .unwrap_err()
                .code,
            "CUBISM_ENTRY"
        );
        fs::remove_dir_all(root).unwrap();
    }

    // A single uncompressed CAFF resource keeps this path-resolution fixture
    // independent of the production encoder's sibling-only export options.
    fn animation_reference(reference: &str) -> Vec<u8> {
        let xml = format!(
            r#"<root><main><CAnimation><CResource_Linked_Model>
          <File xs.n="srcFile" path="{reference}"/>
        </CResource_Linked_Model></CAnimation></main></root>"#
        );
        let mut table = 1u32.to_be_bytes().to_vec();
        table.extend([8]);
        table.extend(b"main.xml");
        table.extend([0]); // empty tag
        let offset = 54 + table.len() + 8 + 4 + 2 + 8;
        table.extend((offset as u64).to_be_bytes());
        table.extend((xml.len() as u32).to_be_bytes());
        table.extend([1, 16]); // present, uncompressed
        table.extend([0; 8]);
        let mut bytes = vec![0; 54];
        bytes[..4].copy_from_slice(b"CAFF");
        bytes.extend(table);
        bytes.extend(xml.as_bytes());
        bytes.extend(b"bc");
        bytes
    }

    #[test]
    fn linked_project_matches_resolved_identity_in_both_entry_directions() {
        let root =
            std::env::temp_dir().join(format!("zipzi-linked-reference-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("models")).unwrap();
        let model = root.join("models/model.cmo3");
        let sibling = root.join("model.cmo3");
        fs::write(&model, b"nested").unwrap();
        fs::write(&sibling, b"sibling").unwrap();
        let animation = root.join("animation.can3");
        fs::write(&animation, animation_reference("models/model.cmo3")).unwrap();
        assert_eq!(
            linked_project(&animation).unwrap().0,
            fs::canonicalize(&model).unwrap()
        );
        // Selecting the same-named sibling must not attach the other model's animation.
        assert!(linked_project(&sibling).unwrap().1.is_none());
        // An absolute reference to the sibling is a real match.
        fs::write(&animation, animation_reference(sibling.to_str().unwrap())).unwrap();
        assert!(linked_project(&sibling).unwrap().1.is_some());
        fs::write(root.join("second.can3"), animation_reference("model.cmo3")).unwrap();
        assert!(linked_project(&sibling)
            .unwrap_err()
            .message
            .contains("Multiple matching"));
        fs::remove_dir_all(root).unwrap();
    }
}

fn read_cubism_animation(path: &Path, workspace: usize) -> Result<Vec<u8>, Status> {
    use std::io::Read;
    let limit = (workspace / 16).min(512 << 20);
    let file = fs::File::open(path).map_err(|e| Status::error("CUBISM_IO", e.to_string()))?;
    if file
        .metadata()
        .map_err(|e| Status::error("CUBISM_IO", e.to_string()))?
        .len()
        > limit as u64
    {
        return Err(Status::error(
            "RESOURCE_BUDGET_EXCEEDED",
            "CAN3 source exceeds reserved workspace",
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Status::error("CUBISM_IO", e.to_string()))?;
    if bytes.len() > limit {
        return Err(Status::error(
            "RESOURCE_BUDGET_EXCEEDED",
            "CAN3 grew beyond reserved workspace",
        ));
    }
    Ok(bytes)
}

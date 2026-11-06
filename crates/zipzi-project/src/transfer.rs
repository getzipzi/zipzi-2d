//! Native transfer adapters: fixed resource bytes, linked project imports and
//! complete grouped output. The editor never performs a second source read.
use crate::Publication;
use std::{
    fs,
    path::{Path, PathBuf},
};
use zipzi_core::{state::DocumentState, Status};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferFormat {
    Runtime,
    Cubism,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransferPublishOptions {
    /// Permit reported Cubism conversion losses. Defaults to strict rejection;
    /// runtime package exports do not use this option.
    pub allow_loss: bool,
}

pub struct TransferSnapshot {
    document: DocumentState,
    resources: crate::ResourceView,
    protected_paths: Vec<PathBuf>,
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
impl crate::ProjectImporter {
    pub fn import_cubism_authoring(&self, path: &Path) -> Result<crate::ImportedProject, Status> {
        let workspace = (self.resources.budget().available_bytes() / 2).min(2usize << 30);
        let _decoded = self.resources.budget().reserve_transfer(workspace)?;
        let (model, animation) = linked_project_with_budget(path, workspace)?;
        let length = fs::metadata(&model)
            .map_err(|e| Status::error("CUBISM_IO", e.to_string()))?
            .len() as usize;
        let _input = self
            .resources
            .budget()
            .reserve_transfer(length.saturating_mul(2))?;
        let bytes = fs::read(&model).map_err(|e| Status::error("CUBISM_IO", e.to_string()))?;
        let mut imported =
            zipzi_cmo3::import_model_with_budget(&bytes, workspace).map_err(codec)?;
        if let Some(animation) = animation {
            imported.document =
                zipzi_cmo3::import_animation_with_budget(&animation, &imported.document, workspace)
                    .map_err(codec)?;
        }
        let mut b = imported.document.builder();
        let mut images = Vec::new();
        for id in imported.document.asset_order() {
            let mut record = imported.document.asset(id).unwrap().clone();
            let zipzi_core::state::AssetContent::Unresolved { source } = &record.content else {
                continue;
            };
            let bytes = imported
                .images
                .get(source)
                .cloned()
                .ok_or_else(|| Status::error("CUBISM_RESOURCE", "Missing imported image"))?;
            let image = self.resources.adopt_image(crate::prepare_png_content(
                bytes.into(),
                self.resources.budget(),
            )?)?;
            record.content = zipzi_core::state::AssetContent::Image {
                resource: image.key(),
                width: image.width(),
                height: image.height(),
            };
            b.replace_asset(record)?;
            images.push(image);
        }
        let document = b.freeze()?.state;
        let resources = crate::ResourceView::default()
            .with_images(images)
            .for_state(&document)?;
        let (document, resources) = crate::atlas::unpack_atlas_images(
            &document,
            &resources,
            &self.resources,
            &imported.atlas_assets,
        )?;
        Ok(crate::ImportedProject {
            document,
            resources,
            protected_paths: vec![
                model,
                fs::canonicalize(path).map_err(|e| Status::error("CUBISM_IO", e.to_string()))?,
            ],
        })
    }
}
impl TransferSnapshot {
    pub fn new(
        document: DocumentState,
        resources: crate::ResourceView,
        protected_paths: Vec<PathBuf>,
    ) -> Result<Self, Status> {
        let resources = resources.for_state(&document)?;
        Ok(Self {
            document,
            resources,
            protected_paths,
        })
    }

    pub fn document(&self) -> &DocumentState {
        &self.document
    }
    pub fn publish(
        self,
        format: TransferFormat,
        destination: &Path,
    ) -> Result<Publication, Status> {
        self.publish_with_options(format, destination, &TransferPublishOptions::default())
    }
    pub fn publish_with_options(
        self,
        format: TransferFormat,
        destination: &Path,
        publish_options: &TransferPublishOptions,
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
                let plan = crate::package::build_export_plan_with(
                    &self.document,
                    &self.protected_paths,
                    &|asset| self.resources.read_asset(asset),
                )?;
                crate::package::publish_managed_export_plan(&plan, destination)
            }
            TransferFormat::Cubism => {
                let name = destination
                    .file_name()
                    .and_then(|n| n.to_str())
                    .ok_or_else(|| Status::error("INVALID_DESTINATION", "Invalid output name"))?;
                let resolver = |asset: &zipzi_core::state::AssetRecord| {
                    self.resources
                        .read_asset(asset)
                        .map(|data| data.bytes)
                        .map_err(|s| zipzi_cmo3::Cmo3Error::Asset {
                            asset_id: asset.id.clone(),
                            message: s.message,
                        })
                };
                let loss_policy = if publish_options.allow_loss {
                    zipzi_cmo3::LossPolicy::Allow
                } else {
                    zipzi_cmo3::LossPolicy::Reject
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
                    loss_policy,
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
        let doc = DocumentState::new(uuid::Uuid::new_v4().to_string(), Default::default()).unwrap();
        let artifact =
            crate::encode_document_cmo3(&doc, &Default::default(), &Default::default()).unwrap();
        let path = root.join("model.cmo3");
        fs::write(&path, artifact.bytes).unwrap();
        let budget = crate::ImageMemoryBudget::new(128 << 20);
        budget.bind_host_budget(0, 128 << 20);
        let imported = crate::ProjectImporter {
            resources: crate::ResourceStore::new(budget.clone()),
        };
        let result = imported.import_cubism_authoring(&path);
        fs::remove_dir_all(root).unwrap();
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(result.unwrap().document.id(), doc.id());
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

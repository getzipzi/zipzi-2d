//! Filesystem adapters for a linked editable Cubism model/animation package.
use crate::filesystem::{self as io, FileSystem, NativeFileSystem};
use crate::{DocumentSession, DocumentStore, Publication};
use kasane_cmo3::{
    encode_can3, encode_cubism, Can3Artifact, Can3Options, Cmo3Error, Cmo3Report, CubismArtifact,
    CubismOptions,
};
use kasane_core::{Document, ImageAsset};
use std::{
    fs,
    path::{Component, Path},
};

pub fn encode_document_cubism(
    document: &Document,
    root: &Path,
    options: &CubismOptions,
) -> Result<CubismArtifact, Cmo3Error> {
    encode_cubism(
        document,
        &|asset: &ImageAsset| {
            crate::read_project_asset(root, asset)
                .map(|data| data.bytes)
                .map_err(|status| Cmo3Error::Asset {
                    asset_id: asset.id.clone(),
                    message: status.message,
                })
        },
        options,
    )
}
pub fn encode_project_cubism(
    source: &Path,
    options: &CubismOptions,
) -> Result<CubismArtifact, Cmo3Error> {
    let (result, snapshot) = DocumentStore::new().open(source);
    let snapshot = snapshot.ok_or_else(|| {
        Cmo3Error::InvalidDocument(format!("{}: {}", result.status.code, result.status.message))
    })?;
    encode_document_cubism(
        &snapshot.document,
        snapshot.manifest.parent().unwrap(),
        options,
    )
}
/// Stage and atomically rename a standalone animation file.
pub fn publish_can3(artifact: &Can3Artifact, destination: &Path) -> Result<Publication, Cmo3Error> {
    crate::cmo3::publish_bytes(&artifact.bytes, destination, &NativeFileSystem)
}
/// Publish both files in a managed directory. Existing directories may contain
/// only the same two filenames, export-report.json and expression companions
/// declared in expressions.json; other content is refused.
/// Replacement stages both files first and rolls back if the directory rename fails.
pub fn publish_cubism(
    artifact: &CubismArtifact,
    destination: &Path,
) -> Result<Publication, Cmo3Error> {
    publish_with_filesystem(artifact, destination, &NativeFileSystem)
}
fn status(s: kasane_core::Status) -> Cmo3Error {
    Cmo3Error::InvalidDocument(format!("{}: {}", s.code, s.message))
}
fn validate_expression_filename(name: &str) -> Result<(), Cmo3Error> {
    if name == "expressions.json" {
        return Ok(());
    }
    let id = name
        .strip_prefix("expression-")
        .and_then(|s| s.strip_suffix(".exp3.json"));
    if !id.is_some_and(|id| id.len() == 36 && uuid::Uuid::parse_str(id).is_ok()) {
        return Err(Cmo3Error::InvalidDocument(
            "Unsafe Cubism expression companion filename".into(),
        ));
    }
    Ok(())
}

fn previous_expression_files(destination: &Path) -> Result<Vec<String>, Cmo3Error> {
    let path = destination.join("expressions.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    if fs::symlink_metadata(&path)?.file_type().is_symlink() {
        return Err(Cmo3Error::InvalidDocument(
            "Expression manifest must not be a symbolic link".into(),
        ));
    }
    declared_expression_files(&fs::read(path)?)
}

fn declared_expression_files(bytes: &[u8]) -> Result<Vec<String>, Cmo3Error> {
    let manifest: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| Cmo3Error::InvalidDocument(error.to_string()))?;
    if manifest["Version"] != 1 {
        return Err(Cmo3Error::InvalidDocument(
            "Unsupported expression companion manifest".into(),
        ));
    }
    let mut names = vec!["expressions.json".into()];
    let items = manifest["Expressions"].as_array().ok_or_else(|| {
        Cmo3Error::InvalidDocument("Invalid expression companion manifest".into())
    })?;
    for item in items {
        let name = item["File"].as_str().ok_or_else(|| {
            Cmo3Error::InvalidDocument("Missing expression companion filename".into())
        })?;
        validate_expression_filename(name)?;
        if name == "expressions.json" || names.iter().any(|previous| previous == name) {
            return Err(Cmo3Error::InvalidDocument(
                "Duplicate expression companion filename".into(),
            ));
        }
        names.push(name.into());
    }
    Ok(names)
}
fn publish_with_filesystem(
    artifact: &CubismArtifact,
    destination: &Path,
    files: &dyn FileSystem,
) -> Result<Publication, Cmo3Error> {
    // Both sibling names remain managed when the last animation is removed.
    // Staging below writes only the files present in the new artifact.
    let names = [
        artifact.model_file_name.as_str(),
        artifact.animation_file_name.as_str(),
        "export-report.json",
    ];
    for name in artifact.can3.expression_files.keys() {
        validate_expression_filename(name)?;
    }
    if !artifact.can3.expression_files.is_empty() {
        let manifest = artifact
            .can3
            .expression_files
            .get("expressions.json")
            .ok_or_else(|| {
                Cmo3Error::InvalidDocument("Missing expression companion manifest".into())
            })?;
        let declared = declared_expression_files(manifest)?;
        if declared.len() != artifact.can3.expression_files.len()
            || declared
                .iter()
                .any(|name| !artifact.can3.expression_files.contains_key(name))
        {
            return Err(Cmo3Error::InvalidDocument(
                "Expression companions do not match their manifest".into(),
            ));
        }
    }
    for name in &names {
        if !matches!(
            Path::new(name).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        ) || name.contains(['\\', ':'])
            || name.chars().any(char::is_control)
        {
            return Err(Cmo3Error::InvalidDocument(
                "Package contains an unsafe output filename".into(),
            ));
        }
    }
    if !artifact.model_file_name.ends_with(".cmo3")
        || !artifact.animation_file_name.ends_with(".can3")
        || artifact.model_file_name.strip_suffix(".cmo3")
            != artifact.animation_file_name.strip_suffix(".can3")
    {
        return Err(Cmo3Error::InvalidDocument(
            "Cubism package filenames must have the same basename".into(),
        ));
    }
    io::reject_symlink(destination).map_err(status)?;
    let destination = io::local_path(destination).map_err(status)?;
    let parent = destination
        .parent()
        .filter(|_| destination.file_name().is_some())
        .ok_or_else(|| Cmo3Error::InvalidDocument("Output cannot be a filesystem root".into()))?;
    let _lock = io::lock(parent).map_err(status)?;
    io::reject_symlink(&destination).map_err(status)?;
    if destination.exists() {
        let previous = previous_expression_files(&destination)?;
        for entry in fs::read_dir(&destination)? {
            let entry = entry?;
            if !entry.file_type()?.is_file()
                || !(names.iter().any(|name| entry.file_name() == *name)
                    || previous
                        .iter()
                        .any(|name| entry.file_name() == name.as_str()))
            {
                return Err(Cmo3Error::InvalidDocument(
                    "Refusing to replace a directory containing unmanaged files".into(),
                ));
            }
        }
    }
    let stage = io::Stage::new(parent).map_err(status)?;
    files.write_new(&stage.0.join(names[0]), &artifact.cmo3.bytes)?;
    if !artifact.can3.bytes.is_empty() {
        files.write_new(
            &stage.0.join(&artifact.animation_file_name),
            &artifact.can3.bytes,
        )?;
    }
    for (name, bytes) in &artifact.can3.expression_files {
        files.write_new(&stage.0.join(name), bytes)?;
    }
    let report = serde_json::to_vec_pretty(&artifact.report)
        .map_err(|e| Cmo3Error::InvalidDocument(e.to_string()))?;
    files.write_new(&stage.0.join("export-report.json"), &report)?;
    files.sync_directory(&stage.0)?;
    let backup = io::Stage::new(parent).map_err(status)?;
    let previous = backup.0.join("previous");
    let replacing = destination.exists();
    if replacing {
        files.rename(&destination, &previous)?;
    }
    if let Err(error) = files.rename(&stage.0, &destination) {
        if replacing {
            if let Err(rollback) = files.rename(&previous, &destination) {
                let recovery = backup.preserve().join("previous");
                return Err(Cmo3Error::InvalidDocument(format!(
                    "Publication failed: {error}; rollback failed: {rollback}; recover from {}",
                    recovery.display()
                )));
            }
        }
        return Err(error.into());
    }
    let mut publication = Publication::finish(files, parent);
    if !artifact.report.diagnostics.is_empty() {
        publication.notices.push(
            "Cubism export completed. See export-report.json for details of any pose, curve or timing conversions.".into()
        );
    }
    if let Err(error) = fs::remove_dir_all(&backup.0) {
        let recovery = backup.preserve();
        publication.warnings.push(format!(
            "Published; old package retained at {}: {error}",
            recovery.display()
        ));
    }
    Ok(publication)
}
impl DocumentSession {
    pub fn encode_can3(&self, options: &Can3Options) -> Result<Can3Artifact, Cmo3Error> {
        encode_can3(self.document(), options)
    }
    pub fn export_can3(
        &self,
        destination: &Path,
        options: &Can3Options,
    ) -> Result<(Cmo3Report, Publication), Cmo3Error> {
        let artifact = self.encode_can3(options)?;
        let publication = publish_can3(&artifact, destination)?;
        Ok((artifact.report, publication))
    }
    pub fn encode_cubism(&self, options: &CubismOptions) -> Result<CubismArtifact, Cmo3Error> {
        encode_cubism(
            self.document(),
            &|asset: &ImageAsset| {
                self.read_asset(&asset.id)
                    .map(|data| data.bytes)
                    .map_err(|s| Cmo3Error::Asset {
                        asset_id: asset.id.clone(),
                        message: s.message,
                    })
            },
            options,
        )
    }
    pub fn export_cubism(
        &self,
        destination: &Path,
        options: &CubismOptions,
    ) -> Result<(Cmo3Report, Publication), Cmo3Error> {
        let artifact = self.encode_cubism(options)?;
        let publication = publish_cubism(&artifact, destination)?;
        Ok((artifact.report, publication))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct FailFs {
        fail_write: usize,
        fail_rename: usize,
        writes: AtomicUsize,
        renames: AtomicUsize,
    }
    impl FileSystem for FailFs {
        fn write_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
            if self.writes.fetch_add(1, Ordering::Relaxed) + 1 == self.fail_write {
                return Err(std::io::Error::other("injected write failure"));
            }
            NativeFileSystem.write_new(path, bytes)
        }
        fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
            if self.renames.fetch_add(1, Ordering::Relaxed) + 1 == self.fail_rename {
                return Err(std::io::Error::other("injected rename failure"));
            }
            NativeFileSystem.rename(from, to)
        }
        fn sync_directory(&self, path: &Path) -> std::io::Result<()> {
            NativeFileSystem.sync_directory(path)
        }
    }
    #[test]
    fn paired_publication_restores_both_files_on_write_or_commit_failure() {
        let temp =
            std::env::temp_dir().join(format!("kasane-can3-rollback-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&temp).unwrap();
        let output = temp.join("output");
        fs::create_dir(&output).unwrap();
        let artifact = CubismArtifact {
            model_file_name: "model.cmo3".into(),
            animation_file_name: "model.can3".into(),
            cmo3: kasane_cmo3::Cmo3Artifact {
                bytes: b"new model".to_vec(),
                report: Default::default(),
            },
            can3: Can3Artifact {
                bytes: b"new animation".to_vec(),
                report: Default::default(),
                expression_files: Default::default(),
            },
            report: Default::default(),
        };
        for (fail_write, fail_rename) in [(2, 0), (0, 2)] {
            fs::write(output.join("model.cmo3"), b"old model").unwrap();
            fs::write(output.join("model.can3"), b"old animation").unwrap();
            let files = FailFs {
                fail_write,
                fail_rename,
                writes: AtomicUsize::new(0),
                renames: AtomicUsize::new(0),
            };
            assert!(publish_with_filesystem(&artifact, &output, &files).is_err());
            assert_eq!(fs::read(output.join("model.cmo3")).unwrap(), b"old model");
            assert_eq!(
                fs::read(output.join("model.can3")).unwrap(),
                b"old animation"
            );
            assert!(!fs::read_dir(&temp).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".kasane-stage-")));
        }
        let mut unsafe_artifact = artifact.clone();
        unsafe_artifact.model_file_name = "../model.cmo3".into();
        assert!(publish_cubism(&unsafe_artifact, &output).is_err());
        fs::remove_dir_all(temp).unwrap();
    }
}

/// Publish a complete model-only engineering group, without an empty animation.
pub fn publish_cubism_model(
    model: kasane_cmo3::Cmo3Artifact,
    name: &str,
    destination: &Path,
) -> Result<Publication, Cmo3Error> {
    let report = model.report.clone();
    let artifact = CubismArtifact {
        model_file_name: format!("{name}.cmo3"),
        animation_file_name: format!("{name}.can3"),
        cmo3: model,
        can3: Can3Artifact {
            bytes: Vec::new(),
            report: Default::default(),
            expression_files: Default::default(),
        },
        report,
    };
    publish_cubism(&artifact, destination)
}

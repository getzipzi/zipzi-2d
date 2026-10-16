//! Model3/MOC3/PSD import assembly and attachment preservation.
use super::manifest::publication_result;
use super::{DocumentSnapshot, DocumentStore, ProjectResult, ResourceDiagnostic};
use crate::codec::encode_project;
use crate::filesystem::{self as io, Publication};
use crate::resources::content_sha256;
use kasane_core::document::{valid_attachment_path, PackageAttachment};
use kasane_core::{Document, Status};
use kasane_moc3::{import_from_bare_moc3, import_from_model3_json, ImportReport};
use kasane_psd::{import_psd, ImportReport as PsdImportReport};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

impl DocumentStore {
    pub fn import_model3(
        &self,
        path: &Path,
    ) -> (
        ProjectResult,
        Option<DocumentSnapshot>,
        Option<ImportReport>,
    ) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                return (
                    ProjectResult::failed("PROJECT_IO", &format!("{}: {}", path.display(), e)),
                    None,
                    None,
                );
            }
        };
        let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let mut res = match import_from_model3_json(&content, base_dir) {
            Ok(r) => r,
            Err(s) => return (ProjectResult::from_status(s), None, None),
        };

        let model3 = match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(root) => root,
            Err(error) => {
                return (
                    ProjectResult::failed("INVALID_MODEL3_JSON", &error.to_string()),
                    None,
                    None,
                );
            }
        };
        let file_refs = &model3["FileReferences"];
        let cdi_ref = file_refs
            .get("DisplayInfo")
            .filter(|value| !value.is_null())
            .cloned();
        let cdi_diagnostics = if let Some(reference) = cdi_ref {
            let Some(relative) = reference.as_str() else {
                return (
                    ProjectResult::failed(
                        "INVALID_MODEL3_JSON",
                        "DisplayInfo path must be a string",
                    ),
                    None,
                    None,
                );
            };
            let path = Path::new(relative);
            if relative.is_empty()
                || path.components().any(|component| {
                    !matches!(
                        component,
                        std::path::Component::Normal(_) | std::path::Component::CurDir
                    )
                })
            {
                return (
                    ProjectResult::failed(
                        "INVALID_MODEL3_JSON",
                        "DisplayInfo path must stay inside the model directory",
                    ),
                    None,
                    None,
                );
            }
            let source = base_dir.join(path);
            let source = match source.canonicalize() {
                Ok(path) => path,
                Err(error) => {
                    return (
                        ProjectResult::failed(
                            "CDI_IO_ERROR",
                            &format!("{}: {error}", source.display()),
                        ),
                        None,
                        None,
                    );
                }
            };
            if !base_dir
                .canonicalize()
                .is_ok_and(|base| source.starts_with(base))
            {
                return (
                    ProjectResult::failed(
                        "INVALID_MODEL3_JSON",
                        "DisplayInfo path must resolve inside the model directory",
                    ),
                    None,
                    None,
                );
            }
            let text = match fs::read_to_string(&source) {
                Ok(text) => text,
                Err(error) => {
                    return (
                        ProjectResult::failed(
                            "CDI_IO_ERROR",
                            &format!("{}: {error}", source.display()),
                        ),
                        None,
                        None,
                    );
                }
            };
            match crate::import_cdi3(&res.document, &text) {
                Ok(imported) => {
                    res.document = imported.candidate;
                    res.report
                        .unimported_attachments
                        .retain(|item| !item.starts_with("DisplayInfo:"));
                    imported.diagnostics
                }
                Err(error) => {
                    return (
                        ProjectResult::failed(
                            &error.code,
                            &format!("{}: {}", error.path, error.message),
                        ),
                        None,
                        None,
                    );
                }
            }
        } else {
            Vec::new()
        };

        let pose_diagnostics =
            if let Some(reference) = file_refs.get("Pose").filter(|value| !value.is_null()) {
                let Some(relative) = reference.as_str() else {
                    return (
                        ProjectResult::failed("INVALID_MODEL3_JSON", "Pose path must be a string"),
                        None,
                        None,
                    );
                };
                let text = match read_model_attachment(base_dir, relative, "Pose") {
                    Ok(text) => text,
                    Err(status) => return (ProjectResult::from_status(status), None, None),
                };
                let id = stable_pose_id(&res.document);
                match crate::import_pose3(&res.document, &id, &text) {
                    Ok(imported) => {
                        res.document = imported.candidate;
                        res.report
                            .unimported_attachments
                            .retain(|item| !item.starts_with("Pose:"));
                        imported.diagnostics
                    }
                    Err(error) => {
                        return (
                            ProjectResult::failed(
                                &error.code,
                                &format!("Pose {}: {}", error.path, error.message),
                            ),
                            None,
                            None,
                        )
                    }
                }
            } else {
                Vec::new()
            };

        let physics_diagnostics = if let Some(reference) =
            file_refs.get("Physics").filter(|value| !value.is_null())
        {
            let Some(relative) = reference.as_str() else {
                return (
                    ProjectResult::failed("INVALID_MODEL3_JSON", "Physics path must be a string"),
                    None,
                    None,
                );
            };
            if !valid_attachment_path(relative) {
                return (
                    ProjectResult::failed("INVALID_ATTACHMENT_PATH", relative),
                    None,
                    None,
                );
            }
            if !base_dir.join(relative).is_file() {
                vec![crate::PhysicsDiagnostic {
                    code: "MISSING_PHYSICS_ATTACHMENT".into(),
                    path: "$.FileReferences.Physics".into(),
                    message: format!("{relative} is absent"),
                }]
            } else {
                let text = match read_model_attachment(base_dir, relative, "Physics") {
                    Ok(text) => text,
                    Err(status) => return (ProjectResult::from_status(status), None, None),
                };
                let id = stable_attachment_id(&res.document, b"physics");
                match crate::import_physics3(&res.document, &id, &text) {
                    Ok(imported) => {
                        res.document = imported.candidate;
                        res.report
                            .unimported_attachments
                            .retain(|item| !item.starts_with("Physics:"));
                        imported.diagnostics
                    }
                    Err(error) => {
                        return (
                            ProjectResult::failed(
                                &error.code,
                                &format!("Physics {}: {}", error.path, error.message),
                            ),
                            None,
                            None,
                        )
                    }
                }
            }
        } else {
            Vec::new()
        };

        let mut expression_diagnostics = Vec::new();
        if let Some(references) = file_refs
            .get("Expressions")
            .filter(|value| !value.is_null())
        {
            let Some(references) = references.as_array() else {
                return (
                    ProjectResult::failed("INVALID_MODEL3_JSON", "Expressions must be an array"),
                    None,
                    None,
                );
            };
            let mut expression_paths = std::collections::HashMap::new();
            for (index, registration) in references.iter().enumerate() {
                let Some(name) = registration.get("Name").and_then(|value| value.as_str()) else {
                    return (
                        ProjectResult::failed(
                            "INVALID_MODEL3_JSON",
                            &format!("Expressions[{index}].Name must be a string"),
                        ),
                        None,
                        None,
                    );
                };
                let Some(relative) = registration.get("File").and_then(|value| value.as_str())
                else {
                    return (
                        ProjectResult::failed(
                            "INVALID_MODEL3_JSON",
                            &format!("Expressions[{index}].File must be a string"),
                        ),
                        None,
                        None,
                    );
                };
                if let Some(previous) = expression_paths.insert(name, relative) {
                    if previous == relative {
                        continue;
                    }
                    return (
                        ProjectResult::failed(
                            "DUPLICATE_EXPRESSION_NAME",
                            &format!("Expressions[{index}].Name {name} has conflicting files"),
                        ),
                        None,
                        None,
                    );
                }
                let text = match read_model_attachment(base_dir, relative, "Expression") {
                    Ok(text) => text,
                    Err(status) => return (ProjectResult::from_status(status), None, None),
                };
                let id = stable_expression_id(&res.document, index, name);
                match crate::import_expression3(&res.document, &id, name, &text) {
                    Ok(imported) => {
                        res.document = imported.candidate;
                        expression_diagnostics.extend(imported.diagnostics.into_iter().map(
                            |diagnostic| ResourceDiagnostic {
                                asset_id: name.into(),
                                code: diagnostic.code,
                                message: format!("{}: {}", diagnostic.path, diagnostic.message),
                            },
                        ));
                    }
                    Err(error) => {
                        return (
                            ProjectResult::failed(
                                &error.code,
                                &format!("Expressions[{index}] {}: {}", error.path, error.message),
                            ),
                            None,
                            None,
                        );
                    }
                }
            }
            res.report
                .unimported_attachments
                .retain(|item| !item.starts_with("Expressions:"));
        }

        let mut motion_diagnostics = Vec::new();
        if let Some(references) = file_refs.get("Motions").filter(|value| !value.is_null()) {
            let Some(groups) = references.as_object() else {
                return (
                    ProjectResult::failed("INVALID_MODEL3_JSON", "Motions must be an object"),
                    None,
                    None,
                );
            };
            let mut imported_paths: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            let mut motion_groups = Vec::new();
            for (group_name, entries) in groups {
                let Some(entries) = entries.as_array() else {
                    return (
                        ProjectResult::failed(
                            "INVALID_MODEL3_JSON",
                            &format!("Motions.{group_name} must be an array"),
                        ),
                        None,
                        None,
                    );
                };
                let mut registrations = Vec::new();
                for (index, entry) in entries.iter().enumerate() {
                    let Some(object) = entry.as_object() else {
                        return (
                            ProjectResult::failed(
                                "INVALID_MODEL3_JSON",
                                &format!("Motions.{group_name}[{index}] must be an object"),
                            ),
                            None,
                            None,
                        );
                    };
                    let Some(relative) = object.get("File").and_then(|value| value.as_str()) else {
                        return (
                            ProjectResult::failed(
                                "INVALID_MODEL3_JSON",
                                &format!("Motions.{group_name}[{index}].File must be a string"),
                            ),
                            None,
                            None,
                        );
                    };
                    let valid_path = !relative.is_empty()
                        && std::path::Path::new(relative)
                            .components()
                            .all(|component| {
                                matches!(
                                    component,
                                    std::path::Component::Normal(_) | std::path::Component::CurDir
                                )
                            });
                    if !valid_path {
                        return (
                            ProjectResult::failed(
                                "INVALID_MODEL3_JSON",
                                &format!(
                                    "Motions.{group_name}[{index}].File escapes model directory"
                                ),
                            ),
                            None,
                            None,
                        );
                    }
                    if !base_dir.join(relative).is_file() {
                        motion_diagnostics.push(ResourceDiagnostic {
                            asset_id: relative.into(),
                            code: "MISSING_MOTION_ATTACHMENT".into(),
                            message: format!("Motions.{group_name}[{index}].File is absent"),
                        });
                        continue;
                    }
                    let fade = |key: &str| -> Result<Option<f32>, String> {
                        match object.get(key) {
                            None => Ok(None),
                            Some(value) => value.as_f64().map(|number| number as f32).filter(|number| number.is_finite() && *number >= 0.0).map(Some)
                                .ok_or_else(|| format!("Motions.{group_name}[{index}].{key} must be nonnegative finite number")),
                        }
                    };
                    let fade_in = match fade("FadeInTime") {
                        Ok(value) => value,
                        Err(message) => {
                            return (
                                ProjectResult::failed("INVALID_MODEL3_JSON", &message),
                                None,
                                None,
                            )
                        }
                    };
                    let fade_out = match fade("FadeOutTime") {
                        Ok(value) => value,
                        Err(message) => {
                            return (
                                ProjectResult::failed("INVALID_MODEL3_JSON", &message),
                                None,
                                None,
                            )
                        }
                    };
                    let sound = match object.get("Sound") {
                        None => None,
                        Some(value) => match value.as_str() {
                            Some(value) => Some(value.to_string()),
                            None => {
                                return (
                                    ProjectResult::failed(
                                        "INVALID_MODEL3_JSON",
                                        &format!(
                                            "Motions.{group_name}[{index}].Sound must be a string"
                                        ),
                                    ),
                                    None,
                                    None,
                                )
                            }
                        },
                    };
                    let clip_id = if let Some(id) = imported_paths.get(relative) {
                        id.clone()
                    } else {
                        let text = match read_model_attachment(base_dir, relative, "Motion") {
                            Ok(text) => text,
                            Err(status) => return (ProjectResult::from_status(status), None, None),
                        };
                        let id = stable_motion_id(&res.document, relative);
                        let source_file = relative.rsplit('/').next().unwrap_or(relative);
                        let mut name = source_file
                            .strip_suffix(".motion3.json")
                            .filter(|stem| !stem.is_empty())
                            .unwrap_or(source_file)
                            .to_string();
                        if name.is_empty() {
                            name = format!("{group_name}_{index}");
                        }
                        let base_name = name.clone();
                        let mut suffix = 0;
                        while res.document.motion_order().iter().any(|existing| {
                            res.document
                                .get_motion(existing)
                                .is_some_and(|clip| clip.name == name)
                        }) {
                            suffix += 1;
                            name = if suffix == 1 {
                                format!("{base_name}-{}", &id[..8])
                            } else {
                                format!("{base_name}-{}-{suffix}", &id[..8])
                            };
                        }
                        match crate::import_motion3(&res.document, &id, &name, &text) {
                            Ok(imported) => {
                                res.document = imported.candidate;
                                motion_diagnostics.extend(imported.diagnostics.into_iter().map(
                                    |diagnostic| ResourceDiagnostic {
                                        asset_id: relative.into(),
                                        code: diagnostic.code,
                                        message: format!(
                                            "{}: {}",
                                            diagnostic.path, diagnostic.message
                                        ),
                                    },
                                ));
                            }
                            Err(error) => {
                                return (
                                    ProjectResult::failed(
                                        &error.code,
                                        &format!(
                                            "Motions.{group_name}[{index}] {}: {}",
                                            error.path, error.message
                                        ),
                                    ),
                                    None,
                                    None,
                                )
                            }
                        }
                        imported_paths.insert(relative.to_string(), id.clone());
                        id
                    };
                    let extensions = object
                        .iter()
                        .filter(|(key, _)| {
                            !matches!(
                                key.as_str(),
                                "File" | "FadeInTime" | "FadeOutTime" | "Sound"
                            )
                        })
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect();
                    registrations.push(kasane_core::document::MotionRegistration {
                        clip_id,
                        fade_in,
                        fade_out,
                        sound,
                        extensions,
                    });
                }
                motion_groups.push(kasane_core::document::MotionGroup {
                    name: group_name.clone(),
                    entries: registrations,
                });
            }
            let result = res.document.set_motion_groups(motion_groups);
            if !result.status.is_ok() {
                return (ProjectResult::from_status(result.status), None, None);
            }
            res.report
                .unimported_attachments
                .retain(|item| !item.starts_with("Motions:"));
        }

        let settings = match crate::model3::import_settings(&res.document, &model3) {
            Ok(settings) => settings,
            Err(status) => return (ProjectResult::from_status(status), None, None),
        };
        let result = res.document.set_model3_settings(settings);
        if !result.status.is_ok() {
            return (ProjectResult::from_status(result.status), None, None);
        }
        res.report.unimported_attachments.retain(|item| {
            !item.starts_with("Groups:")
                && !item.starts_with("Layout:")
                && !item.starts_with("HitAreas:")
        });
        let mut managed_diagnostics = Vec::new();
        let mut managed = Vec::new();
        let mut paths = std::collections::HashSet::new();
        let user_data_value = file_refs.get("UserData").filter(|value| !value.is_null());
        if user_data_value.is_some_and(|value| !value.is_string()) {
            return (
                ProjectResult::failed("INVALID_MODEL3_JSON", "UserData must be a string"),
                None,
                None,
            );
        }
        let referenced_files = user_data_value
            .and_then(|value| value.as_str())
            .map(|path| ("UserData", path.to_string()))
            .into_iter()
            .chain(
                res.document
                    .motion_groups()
                    .iter()
                    .flat_map(|group| group.entries.iter())
                    .filter_map(|entry| entry.sound.as_ref().map(|path| ("Sound", path.clone()))),
            );
        for (kind, path) in referenced_files {
            if !valid_attachment_path(&path) {
                return (
                    ProjectResult::failed("INVALID_ATTACHMENT_PATH", &path),
                    None,
                    None,
                );
            }
            if !paths.insert(path.clone()) {
                continue;
            }
            if !base_dir.join(&path).is_file() {
                managed_diagnostics.push(ResourceDiagnostic {
                    asset_id: path.clone(),
                    code: format!("MISSING_{}_ATTACHMENT", kind.to_uppercase()),
                    message: format!("{kind} {path} is absent"),
                });
                continue;
            }
            let bytes = match read_model_attachment_bytes(base_dir, &path, kind) {
                Ok(bytes) => bytes,
                Err(status) => return (ProjectResult::from_status(status), None, None),
            };
            if kind == "UserData" && serde_json::from_slice::<serde_json::Value>(&bytes).is_err() {
                return (
                    ProjectResult::failed("INVALID_USERDATA_JSON", &path),
                    None,
                    None,
                );
            }
            managed.push(PackageAttachment { path, bytes });
        }
        let result = res.document.set_package_attachments(managed);
        if !result.status.is_ok() {
            return (ProjectResult::from_status(result.status), None, None);
        }
        if !managed_diagnostics
            .iter()
            .any(|item| item.code == "MISSING_USERDATA_ATTACHMENT")
        {
            res.report
                .unimported_attachments
                .retain(|item| !item.starts_with("UserData:"));
        }
        let mut project_result = ProjectResult::ok();
        for d in &res.diagnostics {
            project_result.diagnostics.push(ResourceDiagnostic {
                asset_id: d.code.clone(),
                code: d.code.clone(),
                message: d.message.clone(),
            });
        }
        for diagnostic in cdi_diagnostics {
            project_result.diagnostics.push(ResourceDiagnostic {
                asset_id: "DisplayInfo".into(),
                code: diagnostic.code,
                message: format!("{}: {}", diagnostic.path, diagnostic.message),
            });
        }
        for diagnostic in pose_diagnostics {
            project_result.diagnostics.push(ResourceDiagnostic {
                asset_id: "Pose".into(),
                code: diagnostic.code,
                message: format!("{}: {}", diagnostic.path, diagnostic.message),
            });
        }
        for diagnostic in physics_diagnostics {
            project_result.diagnostics.push(ResourceDiagnostic {
                asset_id: "Physics".into(),
                code: diagnostic.code,
                message: format!("{}: {}", diagnostic.path, diagnostic.message),
            });
        }
        project_result.diagnostics.extend(expression_diagnostics);
        project_result.diagnostics.extend(motion_diagnostics);
        project_result.diagnostics.extend(managed_diagnostics);
        let mut incomplete = res.report.unimported_attachments.clone();
        incomplete.extend(
            project_result
                .diagnostics
                .iter()
                .filter(|item| {
                    matches!(
                        item.code.as_str(),
                        "MISSING_MOTION_ATTACHMENT"
                            | "MISSING_SOUND_ATTACHMENT"
                            | "MISSING_USERDATA_ATTACHMENT"
                    )
                })
                .map(|item| format!("{}: {}", item.code, item.asset_id)),
        );
        let result = res.document.set_missing_attachments(incomplete);
        if !result.status.is_ok() {
            return (ProjectResult::from_status(result.status), None, None);
        }
        for w in &res.report.warnings {
            project_result.warnings.push(w.clone());
        }

        self.remember_imported_assets(&res.document);
        let snapshot = DocumentSnapshot {
            document: res.document,
            manifest: PathBuf::new(),
            manifest_sha256: String::new(),
        };

        (project_result, Some(snapshot), Some(res.report))
    }

    pub fn import_bare_moc3(
        &self,
        moc3_path: &Path,
        texture_map: &HashMap<usize, PathBuf>,
    ) -> (
        ProjectResult,
        Option<DocumentSnapshot>,
        Option<ImportReport>,
    ) {
        let bytes = match fs::read(moc3_path) {
            Ok(b) => b,
            Err(e) => {
                return (
                    ProjectResult::failed("PROJECT_IO", &format!("{}: {}", moc3_path.display(), e)),
                    None,
                    None,
                );
            }
        };
        let res = match import_from_bare_moc3(&bytes, texture_map) {
            Ok(r) => r,
            Err(s) => return (ProjectResult::from_status(s), None, None),
        };

        let mut project_result = ProjectResult::ok();
        for d in &res.diagnostics {
            project_result.diagnostics.push(ResourceDiagnostic {
                asset_id: d.code.clone(),
                code: d.code.clone(),
                message: d.message.clone(),
            });
        }
        for w in &res.report.warnings {
            project_result.warnings.push(w.clone());
        }

        self.remember_imported_assets(&res.document);
        let snapshot = DocumentSnapshot {
            document: res.document,
            manifest: PathBuf::new(),
            manifest_sha256: String::new(),
        };

        (project_result, Some(snapshot), Some(res.report))
    }

    /// Publish a new project from a PSD. The destination becomes visible only
    /// after its manifest and every extracted PNG have been written and synced.
    pub fn import_psd(
        &self,
        source: &Path,
        destination: &Path,
    ) -> (
        ProjectResult,
        Option<DocumentSnapshot>,
        Option<PsdImportReport>,
    ) {
        match self.import_psd_inner(source, destination) {
            Ok((result, snapshot, report)) => (result, Some(snapshot), Some(report)),
            Err(status) => (ProjectResult::from_status(status), None, None),
        }
    }

    fn import_psd_inner(
        &self,
        source: &Path,
        destination: &Path,
    ) -> Result<(ProjectResult, DocumentSnapshot, PsdImportReport), Status> {
        let bundle = read_psd_bundle(source)?;
        if let Some(issue) = bundle.document.validate_structure().into_iter().next() {
            return Err(issue.status);
        }
        let manifest_text = encode_project(&bundle.document)?;
        io::reject_symlink(destination)?;
        let destination = io::local_path(destination)?;
        let parent = destination
            .parent()
            .filter(|_| destination.file_name().is_some())
            .ok_or_else(|| {
                Status::error(
                    "INVALID_DESTINATION",
                    "Cannot create a project at a filesystem root",
                )
            })?;
        fs::create_dir_all(parent).map_err(io::io_error)?;
        let _lock = io::lock(parent)?;
        io::reject_symlink(&destination)?;
        if destination.exists() {
            return Err(Status::error(
                "DESTINATION_EXISTS",
                "PSD project destination already exists",
            ));
        }
        let stage = io::Stage::new(parent)?;
        let assets_dir = stage.0.join("assets");
        fs::create_dir(&assets_dir).map_err(io::io_error)?;
        let files = self.filesystem.as_ref();
        for asset in &bundle.assets {
            files
                .write_new(&stage.0.join(&asset.source), &asset.bytes)
                .map_err(io::io_error)?;
        }
        files
            .write_new(
                &stage.0.join("project.kasane.json"),
                manifest_text.as_bytes(),
            )
            .map_err(io::io_error)?;
        files.sync_directory(&assets_dir).map_err(io::io_error)?;
        files.sync_directory(&stage.0).map_err(io::io_error)?;
        io::reject_symlink(&destination)?;
        if destination.exists() {
            return Err(Status::error(
                "DESTINATION_EXISTS",
                "PSD project destination already exists",
            ));
        }
        let manifest = destination.join("project.kasane.json");
        let mut document = bundle.document;
        document.mark_saved();
        let snapshot = DocumentSnapshot {
            document,
            manifest,
            manifest_sha256: content_sha256(manifest_text.as_bytes()),
        };
        files.rename(&stage.0, &destination).map_err(io::io_error)?;
        let mut result = publication_result(Publication::finish(files, parent));
        result
            .warnings
            .extend(bundle.report.warnings.iter().cloned());
        Ok((result, snapshot, bundle.report))
    }
}

fn read_psd_bundle(source: &Path) -> Result<kasane_psd::ImportBundle, Status> {
    let bytes = read_psd_bytes(source)?;
    import_psd(&bytes).map_err(|error| Status::error(error.code, error.message))
}

pub(super) fn read_psd_bytes(source: &Path) -> Result<Vec<u8>, Status> {
    let size = fs::metadata(source).map_err(io::io_error)?.len();
    if size > 512 * 1024 * 1024 {
        return Err(Status::error("PSD_LIMIT", "PSD exceeds 512 MiB"));
    }
    fs::read(source).map_err(io::io_error)
}

fn read_model_attachment(base_dir: &Path, relative: &str, kind: &str) -> Result<String, Status> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path.components().any(|component| {
            !matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
    {
        return Err(Status::error(
            "INVALID_MODEL3_JSON",
            format!("{kind} path must stay inside the model directory"),
        ));
    }
    let source = base_dir.join(path);
    let source = source.canonicalize().map_err(|error| {
        Status::error(
            "ATTACHMENT_IO_ERROR",
            format!("{}: {error}", source.display()),
        )
    })?;
    if !base_dir
        .canonicalize()
        .is_ok_and(|base| source.starts_with(base))
    {
        return Err(Status::error(
            "INVALID_MODEL3_JSON",
            format!("{kind} path resolves outside the model directory"),
        ));
    }
    fs::read_to_string(&source).map_err(|error| {
        Status::error(
            "ATTACHMENT_IO_ERROR",
            format!("{}: {error}", source.display()),
        )
    })
}

fn read_model_attachment_bytes(
    base_dir: &Path,
    relative: &str,
    kind: &str,
) -> Result<Vec<u8>, Status> {
    if !valid_attachment_path(relative) {
        return Err(Status::error("INVALID_ATTACHMENT_PATH", relative));
    }
    let source = base_dir.join(relative);
    let source = source.canonicalize().map_err(|error| {
        Status::error(
            "ATTACHMENT_IO_ERROR",
            format!("{}: {error}", source.display()),
        )
    })?;
    if !base_dir
        .canonicalize()
        .is_ok_and(|base| source.starts_with(base))
    {
        return Err(Status::error(
            "INVALID_ATTACHMENT_PATH",
            format!("{kind} resolves outside model directory"),
        ));
    }
    fs::read(&source).map_err(|error| {
        Status::error(
            "ATTACHMENT_IO_ERROR",
            format!("{}: {error}", source.display()),
        )
    })
}

fn stable_expression_id(document: &Document, index: usize, name: &str) -> String {
    for salt in 0u64.. {
        let mut digest = Sha256::new();
        digest.update(document.id());
        digest.update(b"expression");
        digest.update(index.to_le_bytes());
        digest.update(name.as_bytes());
        digest.update(salt.to_le_bytes());
        let mut bytes: [u8; 16] = digest.finalize()[..16]
            .try_into()
            .expect("SHA-256 has 16 bytes");
        bytes[6] = (bytes[6] & 0x0f) | 0x80;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let id = format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        );
        if !document.contains_id(&id) {
            return id;
        }
    }
    unreachable!("salt space exhausted")
}

fn stable_motion_id(document: &Document, relative: &str) -> String {
    for salt in 0u64.. {
        let mut digest = Sha256::new();
        digest.update(document.id());
        digest.update(b"motion");
        digest.update(relative.as_bytes());
        digest.update(salt.to_le_bytes());
        let mut bytes: [u8; 16] = digest.finalize()[..16]
            .try_into()
            .expect("SHA-256 has 16 bytes");
        bytes[6] = (bytes[6] & 0x0f) | 0x80;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let id = format!(
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        );
        if !document.contains_id(&id) {
            return id;
        }
    }
    unreachable!("salt space exhausted")
}

fn stable_pose_id(document: &Document) -> String {
    stable_attachment_id(document, b"pose")
}

fn stable_attachment_id(document: &Document, marker: &[u8]) -> String {
    for salt in 0u64.. {
        let mut digest = Sha256::new();
        digest.update(document.id());
        digest.update(marker);
        digest.update(salt.to_le_bytes());
        let mut bytes: [u8; 16] = digest.finalize()[..16]
            .try_into()
            .expect("SHA-256 has 16 bytes");
        bytes[6] = (bytes[6] & 0x0f) | 0x80;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
        let id = format!(
            "{}-{}-{}-{}-{}",
            &hex[..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..]
        );
        if !document.contains_id(&id) {
            return id;
        }
    }
    unreachable!("salt space exhausted")
}

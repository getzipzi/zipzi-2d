//! Model3/MOC3/PSD import assembly and attachment preservation.
use super::{ImportedProject, ProjectImporter, ProjectResult, ResourceDiagnostic};
use crate::filesystem::{self as io};

use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
use zipzi_core::document::{valid_attachment_path, MotionRegistration, PackageAttachment};
use zipzi_core::state::{DocumentBuilder, DocumentState};
use zipzi_core::Status;
use zipzi_moc3::{import_from_bare_moc3, import_from_model3_json, ImportReport};
use zipzi_psd::{import_psd_with_masks, ImportReport as PsdImportReport};

impl ProjectImporter {
    pub fn import_model3(
        &self,
        path: &Path,
    ) -> (ProjectResult, Option<ImportedProject>, Option<ImportReport>) {
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
                    Ok((_, text)) => text,
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
                    Ok((_, text)) => text,
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

        let mut referenced_animations = HashSet::new();
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
            let mut sources = HashMap::new();
            let mut batch = ModelAnimationBatch::new(&res.document);
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
                let (source, text) = match read_model_attachment(base_dir, relative, "Expression") {
                    Ok(value) => value,
                    Err(status) => return (ProjectResult::from_status(status), None, None),
                };
                referenced_animations.insert(source);
                let origin = format!("Expressions[{index}] ({relative})");
                let id = batch.stable_id(&[b"expression", &index.to_le_bytes(), name.as_bytes()]);
                if let Err(status) =
                    batch.add_expression(&id, name, &text, &mut expression_diagnostics)
                {
                    return (
                        ProjectResult::failed(
                            &status.code,
                            &format!("{origin}: {}", status.message),
                        ),
                        None,
                        None,
                    );
                }
                sources.insert(id, origin);
            }
            res.document = match batch.finish_registered(&sources) {
                Ok(state) => state,
                Err(status) => return (ProjectResult::from_status(status), None, None),
            };
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
            let mut sources = HashMap::new();
            let mut motions = ModelAnimationBatch::new(&res.document);
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
                    let origin = format!("Motions.{group_name}[{index}] ({relative})");
                    let clip_id = if let Some(id) = imported_paths.get(relative) {
                        id.clone()
                    } else {
                        let (source, text) =
                            match read_model_attachment(base_dir, relative, "Motion") {
                                Ok(value) => value,
                                Err(status) => {
                                    return (ProjectResult::from_status(status), None, None)
                                }
                            };
                        referenced_animations.insert(source);
                        let source_file = relative.rsplit('/').next().unwrap_or(relative);
                        let mut name = source_file
                            .strip_suffix(".motion3.json")
                            .filter(|stem| !stem.is_empty())
                            .unwrap_or(source_file)
                            .to_string();
                        if name.is_empty() {
                            name = format!("{group_name}_{index}");
                        }
                        let id = match motions.add_motion(
                            relative,
                            &name,
                            &text,
                            &mut motion_diagnostics,
                        ) {
                            Ok(id) => id,
                            Err(status) => {
                                return (
                                    ProjectResult::failed(
                                        &status.code,
                                        &format!("{origin}: {}", status.message),
                                    ),
                                    None,
                                    None,
                                )
                            }
                        };
                        sources.insert(id.clone(), origin.clone());
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
                    let registration_id = uuid::Uuid::new_v4().to_string();
                    sources.insert(registration_id.clone(), origin);
                    registrations.push(MotionRegistration {
                        id: registration_id,
                        clip_id,
                        fade_in,
                        fade_out,
                        sound: sound.map(Into::into),
                        extensions,
                    });
                }
                let group_id = uuid::Uuid::new_v4().to_string();
                sources.insert(group_id.clone(), format!("Motions.{group_name}"));
                motion_groups.push(zipzi_core::document::MotionGroup {
                    id: group_id,
                    name: group_name.clone(),
                    entries: registrations,
                });
            }
            let result = motions
                .builder
                .set_motion_groups(motion_groups)
                .and_then(|()| motions.builder.set_motion_roots(vec![]))
                .and_then(|()| motions.finish_registered(&sources));
            res.document = match result {
                Ok(state) => state,
                Err(status) => return (ProjectResult::from_status(status), None, None),
            };
            res.report
                .unimported_attachments
                .retain(|item| !item.starts_with("Motions:"));
        }

        let settings = match crate::model3::import_settings(&res.document, &model3) {
            Ok(settings) => settings,
            Err(status) => return (ProjectResult::from_status(status), None, None),
        };
        let result = import_edit(&mut res.document, |builder| {
            builder.set_model3_settings(settings)
        });
        if let Err(status) = result {
            return (ProjectResult::from_status(status), None, None);
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
                    .filter_map(|entry| {
                        entry.sound.as_ref().map(|path| {
                            (
                                "Sound",
                                res.document.attachment_path(path).unwrap().to_string(),
                            )
                        })
                    }),
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
            managed.push(PackageAttachment {
                id: uuid::Uuid::new_v4().to_string(),
                path,
                bytes: Some(std::sync::Arc::new(bytes)),
            });
        }
        if let Err(status) = register_package_attachments(&mut res.document, managed) {
            return (ProjectResult::from_status(status), None, None);
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
        let discovered_animations = import_unreferenced_animations(
            &mut res.document,
            path,
            base_dir,
            &referenced_animations,
            &mut project_result.diagnostics,
        );
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
        let result = import_edit(&mut res.document, |builder| {
            builder.set_missing_attachments(incomplete)
        });
        if let Err(status) = result {
            return (ProjectResult::from_status(status), None, None);
        }
        for w in &res.report.warnings {
            project_result.warnings.push(w.clone());
        }

        let mut snapshot = match self.capture_files(
            res.document,
            &res.textures,
            &mut project_result.diagnostics,
        ) {
            Ok(value) => value,
            Err(status) => return (ProjectResult::from_status(status), None, None),
        };

        match io::local_path(path) {
            Ok(source) => snapshot.protected_paths.push(source),
            Err(status) => return (ProjectResult::from_status(status), None, None),
        }
        fn sources(value: &serde_json::Value, root: &Path, out: &mut Vec<PathBuf>) {
            match value {
                serde_json::Value::String(v) => {
                    if let Ok(path) = root.join(v).canonicalize() {
                        out.push(path);
                    }
                }
                serde_json::Value::Object(v) => {
                    for value in v.values() {
                        sources(value, root, out)
                    }
                }
                serde_json::Value::Array(v) => {
                    for value in v {
                        sources(value, root, out)
                    }
                }
                _ => {}
            }
        }
        sources(file_refs, base_dir, &mut snapshot.protected_paths);
        snapshot.protected_paths.extend(discovered_animations);
        (project_result, Some(snapshot), Some(res.report))
    }

    pub fn import_bare_moc3(
        &self,
        moc3_path: &Path,
        texture_map: &HashMap<usize, PathBuf>,
    ) -> (ProjectResult, Option<ImportedProject>, Option<ImportReport>) {
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

        let mut snapshot = match self.capture_files(
            res.document,
            &res.textures,
            &mut project_result.diagnostics,
        ) {
            Ok(value) => value,
            Err(status) => return (ProjectResult::from_status(status), None, None),
        };

        match io::local_path(moc3_path) {
            Ok(source) => snapshot.protected_paths.push(source),
            Err(status) => return (ProjectResult::from_status(status), None, None),
        }
        (project_result, Some(snapshot), Some(res.report))
    }

    /// Decode and capture PSD artwork without owning or publishing a live model.
    pub fn import_psd(&self, source: &Path) -> Result<(ImportedProject, PsdImportReport), Status> {
        self.import_psd_with_masks(source, false)
    }

    pub fn import_psd_with_masks(
        &self,
        source: &Path,
        preserve_masks: bool,
    ) -> Result<(ImportedProject, PsdImportReport), Status> {
        let bytes = read_psd_bytes(source)?;
        let bundle = import_psd_with_masks(&bytes, preserve_masks)
            .map_err(|error| Status::error(error.code, error.message))?;
        let mut builder = bundle.document.builder();
        let mut images = Vec::new();
        for asset in bundle.assets {
            let image = self.resources.adopt_image(crate::prepare_png_content(
                asset.bytes.into(),
                self.resources.budget(),
            )?)?;
            let mut record = builder.asset(&asset.id).unwrap().clone();
            record.content = image_content(&image);
            builder.replace_asset(record)?;
            images.push(image);
        }
        let document = builder.freeze()?.state;
        let resources = crate::ResourceView::default()
            .with_images(images)
            .for_state(&document)?;
        Ok((
            ImportedProject {
                document,
                resources,
                protected_paths: vec![source.canonicalize().map_err(io::io_error)?],
            },
            bundle.report,
        ))
    }
}

pub(super) fn read_psd_bytes(source: &Path) -> Result<Vec<u8>, Status> {
    let size = fs::metadata(source).map_err(io::io_error)?.len();
    if size > 512 * 1024 * 1024 {
        return Err(Status::error("PSD_LIMIT", "PSD exceeds 512 MiB"));
    }
    fs::read(source).map_err(io::io_error)
}

fn read_model_attachment(
    base_dir: &Path,
    relative: &str,
    kind: &str,
) -> Result<(PathBuf, String), Status> {
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
    let text = fs::read_to_string(&source).map_err(|error| {
        Status::error(
            "ATTACHMENT_IO_ERROR",
            format!("{}: {error}", source.display()),
        )
    })?;
    Ok((source, text))
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

enum AnimationSidecar {
    Expression,
    Motion,
}

struct DiscoveredAnimation {
    path: PathBuf,
    relative: String,
    name: String,
    kind: AnimationSidecar,
}

fn import_unreferenced_animations(
    document: &mut DocumentState,
    model_path: &Path,
    base_dir: &Path,
    referenced: &HashSet<PathBuf>,
    diagnostics: &mut Vec<ResourceDiagnostic>,
) -> Vec<PathBuf> {
    let (Ok(base_dir), Ok(model_path)) = (base_dir.canonicalize(), model_path.canonicalize())
    else {
        return Vec::new();
    };
    let mut directories = vec![base_dir.clone()];
    let mut files = Vec::new();
    while let Some(directory) = directories.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        let entries: Vec<_> = entries.flatten().collect();
        // A nested model owns its subtree. A shared package root is ambiguous,
        // so only its explicitly registered animations are imported.
        if entries.iter().any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .to_ascii_lowercase()
                .ends_with(".model3.json")
                && entry
                    .file_type()
                    .is_ok_and(|kind| kind.is_file() || kind.is_symlink())
                && entry.path().canonicalize().ok().as_ref() != Some(&model_path)
        }) {
            continue;
        }
        for entry in entries {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                directories.push(entry.path());
            } else if file_type.is_file() {
                let file_name = entry.file_name();
                let file_name = file_name.to_string_lossy();
                let lower = file_name.to_ascii_lowercase();
                let (suffix, kind) = if lower.ends_with(".exp3.json") {
                    (".exp3.json", AnimationSidecar::Expression)
                } else if lower.ends_with(".motion3.json") {
                    (".motion3.json", AnimationSidecar::Motion)
                } else {
                    continue;
                };
                let stem = &file_name[..file_name.len() - suffix.len()];
                let name = if stem.is_empty() {
                    file_name.as_ref()
                } else {
                    stem
                };
                files.push((entry.path(), name.to_string(), kind));
            }
        }
    }
    files.sort_unstable_by(|a, b| a.0.cmp(&b.0));

    let mut sidecars = Vec::new();
    for (path, name, kind) in files {
        let Ok(source) = path.canonicalize() else {
            continue;
        };
        if !source.starts_with(&base_dir) || referenced.contains(&source) {
            continue;
        }
        let relative = path
            .strip_prefix(&base_dir)
            .expect("scanned model directory")
            .to_string_lossy()
            .into_owned();
        sidecars.push(DiscoveredAnimation {
            path: source,
            relative,
            name,
            kind,
        });
    }
    if sidecars.is_empty() {
        return Vec::new();
    }
    import_discovered_animations(document, &sidecars, diagnostics)
}

fn import_discovered_animations(
    document: &mut DocumentState,
    files: &[DiscoveredAnimation],
    diagnostics: &mut Vec<ResourceDiagnostic>,
) -> Vec<PathBuf> {
    let report_error = |file: &DiscoveredAnimation, status: Status| ResourceDiagnostic {
        asset_id: file.relative.clone(),
        code: status.code,
        message: format!("{}: {}", file.relative, status.message),
    };
    let mut batch = ModelAnimationBatch::new(document);
    let mut imported = Vec::new();
    let mut batch_diagnostics = Vec::new();
    for file in files {
        match batch.add_discovered(file, &mut batch_diagnostics) {
            Ok(()) => imported.push(file.path.clone()),
            Err(status) => batch_diagnostics.push(report_error(file, status)),
        }
    }
    if let Ok(state) = batch.finish() {
        *document = state;
        diagnostics.extend(batch_diagnostics);
        return imported;
    }

    // Discovered sidecars are optional. Isolate validation failures so a bad
    // clip only produces its own diagnostic and the other clips are kept.
    imported.clear();
    for file in files {
        let mut batch = ModelAnimationBatch::new(document);
        let mut file_diagnostics = Vec::new();
        let result = batch
            .add_discovered(file, &mut file_diagnostics)
            .and_then(|()| batch.finish());
        match result {
            Ok(state) => {
                *document = state;
                imported.push(file.path.clone());
                diagnostics.extend(file_diagnostics);
            }
            Err(status) => diagnostics.push(report_error(file, status)),
        }
    }
    imported
}

struct ModelAnimationBatch<'a> {
    document: &'a DocumentState,
    builder: DocumentBuilder,
    staged_ids: HashSet<String>,
    new_motion_roots: Vec<MotionRegistration>,
}

impl<'a> ModelAnimationBatch<'a> {
    fn new(document: &'a DocumentState) -> Self {
        Self {
            document,
            builder: document.builder(),
            staged_ids: HashSet::new(),
            new_motion_roots: Vec::new(),
        }
    }

    fn stable_id(&self, parts: &[&[u8]]) -> String {
        stable_package_id(self.document.id(), parts, |id| {
            self.staged_ids.contains(id) || self.document.contains_id(id)
        })
    }

    fn add_expression(
        &mut self,
        id: &str,
        name: &str,
        text: &str,
        diagnostics: &mut Vec<ResourceDiagnostic>,
    ) -> Result<(), Status> {
        let (asset, expression_diagnostics) =
            crate::expression::decode_expression_asset(self.document, id, name, text).map_err(
                |error| Status::error(error.code, format!("{}: {}", error.path, error.message)),
            )?;
        self.builder.create_expression(asset)?;
        self.staged_ids.insert(id.into());
        diagnostics.extend(expression_diagnostics.into_iter().map(|diagnostic| {
            ResourceDiagnostic {
                asset_id: name.into(),
                code: diagnostic.code,
                message: format!("{}: {}", diagnostic.path, diagnostic.message),
            }
        }));
        Ok(())
    }

    fn add_motion(
        &mut self,
        relative: &str,
        name: &str,
        text: &str,
        diagnostics: &mut Vec<ResourceDiagnostic>,
    ) -> Result<String, Status> {
        let id = self.stable_id(&[b"motion", relative.as_bytes()]);
        let name = unique_animation_name(name, &id, |name| {
            self.builder.motion_order().iter().any(|id| {
                self.builder
                    .motion(id)
                    .is_some_and(|clip| clip.name == name)
            })
        });
        let (clip, clip_diagnostics) =
            crate::motion::decode_motion_clip(self.document, &id, &name, text).map_err(
                |error| Status::error(error.code, format!("{}: {}", error.path, error.message)),
            )?;
        // New clip IDs use UUIDv8, like track and event IDs. Imported keys use UUIDv4.
        self.staged_ids.insert(id.clone());
        self.staged_ids
            .extend(clip.tracks.iter().map(|track| track.id.clone()));
        self.staged_ids
            .extend(clip.events.iter().map(|event| event.id.clone()));
        self.builder.create_motion(clip)?;
        diagnostics.extend(
            clip_diagnostics
                .into_iter()
                .map(|diagnostic| ResourceDiagnostic {
                    asset_id: relative.into(),
                    code: diagnostic.code,
                    message: format!("{}: {}", diagnostic.path, diagnostic.message),
                }),
        );
        Ok(id)
    }

    fn add_discovered(
        &mut self,
        file: &DiscoveredAnimation,
        diagnostics: &mut Vec<ResourceDiagnostic>,
    ) -> Result<(), Status> {
        let text = fs::read_to_string(&file.path)
            .map_err(|error| Status::error("ATTACHMENT_IO_ERROR", error.to_string()))?;
        match file.kind {
            AnimationSidecar::Expression => {
                let id = self.stable_id(&[b"expression:", file.relative.as_bytes()]);
                let name = unique_animation_name(&file.name, &id, |name| {
                    self.builder.expression_order().iter().any(|id| {
                        self.builder
                            .expression(id)
                            .is_some_and(|asset| asset.name == name)
                    })
                });
                self.add_expression(&id, &name, &text, diagnostics)?;
            }
            AnimationSidecar::Motion => {
                let clip_id = self.add_motion(&file.relative, &file.name, &text, diagnostics)?;
                self.new_motion_roots.push(MotionRegistration {
                    id: uuid::Uuid::new_v4().to_string(),
                    clip_id,
                    fade_in: None,
                    fade_out: None,
                    sound: None,
                    extensions: Default::default(),
                });
            }
        }
        Ok(())
    }

    fn finish(mut self) -> Result<DocumentState, Status> {
        if !self.new_motion_roots.is_empty() {
            let mut roots = self.builder.motion_roots().clone();
            roots.append(&mut self.new_motion_roots);
            self.builder.set_motion_roots(roots)?;
        }
        Ok(self.builder.freeze()?.state)
    }

    fn finish_registered(
        mut self,
        sources: &HashMap<String, String>,
    ) -> Result<DocumentState, Status> {
        self.builder.validate_candidate().map_err(|mut status| {
            // Animation validation reports the record ID or a path starting with it.
            let id = status.message.split('.').next().unwrap_or_default();
            let source = sources.get(id).or_else(|| {
                uuid::Uuid::parse_str(id).ok()?;
                if let Some((asset, _)) = self.builder.expression_entry(id) {
                    return sources.get(&asset.id);
                }
                self.builder.motion_order().iter().find_map(|clip_id| {
                    let clip = self.builder.motion(clip_id)?;
                    let owns_id = clip
                        .tracks
                        .iter()
                        .any(|track| track.id == id || track.keys().any(|key| key.id == id))
                        || clip.events.iter().any(|event| event.id == id);
                    owns_id.then(|| sources.get(clip_id)).flatten()
                })
            });
            if let Some(source) = source {
                status.message = format!("{source}: {}", status.message);
            }
            status
        })?;
        self.finish()
    }
}

fn unique_animation_name(stem: &str, id: &str, is_taken: impl Fn(&str) -> bool) -> String {
    if !is_taken(stem) {
        return stem.to_owned();
    }
    let base = format!("{stem}-{}", &id[..8]);
    let mut name = base.clone();
    let mut suffix = 2;
    while is_taken(&name) {
        name = format!("{base}-{suffix}");
        suffix += 1;
    }
    name
}

fn stable_package_id(
    document_id: &str,
    parts: &[&[u8]],
    contains_id: impl Fn(&str) -> bool,
) -> String {
    for salt in 0u64.. {
        let mut digest = Sha256::new();
        digest.update(document_id);
        for part in parts {
            digest.update(part);
        }
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
        if !contains_id(&id) {
            return id;
        }
    }
    unreachable!("salt space exhausted")
}

fn stable_pose_id(document: &DocumentState) -> String {
    stable_attachment_id(document, b"pose")
}

fn stable_attachment_id(document: &DocumentState, marker: &[u8]) -> String {
    stable_package_id(document.id(), &[marker], |id| document.contains_id(id))
}

fn import_edit(
    state: &mut DocumentState,
    edit: impl FnOnce(&mut zipzi_core::state::DocumentBuilder) -> Result<(), Status>,
) -> Result<(), Status> {
    let mut builder = state.builder();
    edit(&mut builder)?;
    *state = builder.freeze()?.state;
    Ok(())
}
fn register_package_attachments(
    state: &mut DocumentState,
    attachments: Vec<PackageAttachment>,
) -> Result<(), Status> {
    if attachments.is_empty() && state.package_attachments().is_empty() {
        return Ok(());
    }
    let paths: HashMap<_, _> = attachments
        .iter()
        .map(|a| (a.path.clone(), a.id.clone()))
        .collect();
    let resolve = |r: &mut zipzi_core::document::AttachmentRef| {
        if let zipzi_core::document::AttachmentRef::Unresolved { path } = r {
            if let Some(id) = paths.get(path) {
                *r = zipzi_core::document::AttachmentRef::Resolved {
                    attachment_id: id.clone(),
                };
            }
        }
    };
    import_edit(state, |b| {
        b.set_package_attachments(attachments)?;
        let mut groups = b.motion_groups().clone();
        let mut roots = b.motion_roots().clone();
        let mut settings = b.model3_settings().clone();
        for e in groups
            .iter_mut()
            .flat_map(|g| &mut g.entries)
            .chain(&mut roots)
        {
            if let Some(r) = &mut e.sound {
                resolve(r);
            }
        }
        if let Some(r) = &mut settings.user_data {
            resolve(r);
        }
        b.set_motion_groups(groups)?;
        b.set_motion_roots(roots)?;
        b.set_model3_settings(settings)
    })
}
fn image_content(image: &crate::ImageResource) -> zipzi_core::state::AssetContent {
    zipzi_core::state::AssetContent::Image {
        resource: image.key(),
        width: image.width(),
        height: image.height(),
    }
}
impl ProjectImporter {
    fn capture_files(
        &self,
        state: DocumentState,
        descriptors: &[zipzi_moc3::TextureSlotInfo],
        diagnostics: &mut Vec<ResourceDiagnostic>,
    ) -> Result<ImportedProject, Status> {
        let mut b = state.builder();
        let mut images = Vec::new();
        let mut cropped = HashMap::new();
        let mut protected_paths = Vec::new();
        let mut pages = Vec::new();
        for id in state.asset_order() {
            let record = state.asset(id).unwrap();
            let zipzi_core::state::AssetContent::Unresolved { source } = &record.content else {
                continue;
            };
            if source.starts_with("cubism-texture-slot:") {
                continue;
            }
            let Some(descriptor) = descriptors
                .iter()
                .find(|d| d.source == *source && d.width > 0 && d.height > 0)
            else {
                continue;
            };
            pages.push((record, source, descriptor));
        }
        // Decode larger pages before retained crops consume the decode workspace.
        pages.sort_by_key(|(_, _, descriptor)| {
            std::cmp::Reverse(u64::from(descriptor.width) * u64::from(descriptor.height))
        });
        let users = crate::atlas::mesh_texture_users(&state);
        for (record, source, descriptor) in pages {
            let captured = crate::prepare_image_content_with_budget(
                Path::new(source),
                self.resources.budget(),
            )
            .and_then(|image| {
                let content = image.content();
                if (content.width, content.height) != (descriptor.width, descriptor.height)
                    || (!descriptor.sha256.is_empty() && content.sha256 != descriptor.sha256)
                {
                    return Err(Status::error("RESOURCE_CHANGED", source));
                }
                self.resources.adopt_image(image)
            });
            match captured {
                Ok(image) => {
                    protected_paths.push(io::local_path(Path::new(source))?);
                    if let Some(meshes) = users.get(&record.id) {
                        crate::atlas::unpack_atlas_page(
                            &mut b,
                            &record.id,
                            meshes,
                            &image.texture(),
                            &self.resources,
                            &mut cropped,
                        )?;
                    } else {
                        let mut record = record.clone();
                        record.content = image_content(&image);
                        b.replace_asset(record)?;
                        images.push(image);
                    }
                }
                Err(status) => diagnostics.push(ResourceDiagnostic {
                    asset_id: record.id.clone(),
                    code: status.code,
                    message: status.message,
                }),
            }
        }
        let document = b.freeze()?.state;
        let resources = crate::ResourceView::default()
            .with_images(images.into_iter().chain(cropped.into_values()))
            .for_state(&document)?;
        // Reap the final page's weak buffer entries even if no more allocations follow.
        self.resources.budget().usage();
        Ok(ImportedProject {
            document,
            resources,
            protected_paths,
        })
    }
}

#[cfg(test)]
mod atlas_capture_tests {
    use super::*;
    use zipzi_core::state::{AssetContent, AssetRecord};
    use zipzi_core::{Mesh, Vec2};

    #[test]
    fn captures_multiple_atlases_under_a_single_page_decode_budget() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::ResourceStore::new(crate::ImageMemoryBudget::new(40 << 10));
        let importer = ProjectImporter {
            resources: store.clone(),
        };
        let mut builder = DocumentState::new(
            uuid::Uuid::new_v4().to_string(),
            zipzi_core::Canvas::new(64., 64., Vec2::default(), 1.).into(),
        )
        .unwrap()
        .builder();
        let mut descriptors = Vec::new();
        for slot in 0..2 {
            let path = directory.path().join(format!("{slot}.png"));
            let mut encoder = png::Encoder::new(fs::File::create(&path).unwrap(), 64, 64);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&vec![100 + slot as u8; 64 * 64 * 4])
                .unwrap();
            let asset_id = uuid::Uuid::new_v4().to_string();
            let source = path.to_str().unwrap().to_owned();
            builder
                .create_asset(AssetRecord {
                    id: asset_id.clone(),
                    name: format!("Page {slot}"),
                    content: AssetContent::Unresolved {
                        source: source.clone(),
                    },
                })
                .unwrap();
            builder
                .create_mesh(Mesh {
                    id: uuid::Uuid::new_v4().to_string(),
                    runtime_id: format!("Mesh{slot}"),
                    texture_asset_id: asset_id.clone().into(),
                    vertex_ids: vec![0, 1, 2],
                    base_positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                    uvs: vec![
                        Vec2::new(0.25, 0.25),
                        Vec2::new(0.5, 0.25),
                        Vec2::new(0.25, 0.5),
                    ],
                    triangles: vec![[0, 1, 2]],
                    ..Default::default()
                })
                .unwrap();
            descriptors.push(zipzi_moc3::TextureSlotInfo {
                slot,
                asset_id,
                name: String::new(),
                source,
                width: 64,
                height: 64,
                sha256: String::new(),
            });
        }
        let state = builder.freeze().unwrap().state;
        let mut diagnostics = Vec::new();
        let project = importer
            .capture_files(state, &descriptors, &mut diagnostics)
            .unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(project.resources.len(), 2);
        assert_eq!(project.document.asset_order().len(), 2);
        let usage = store.budget().usage();
        assert_eq!(usage.png_bytes, 0);
        assert!(usage.rgba_bytes < 16 << 10);
    }
}

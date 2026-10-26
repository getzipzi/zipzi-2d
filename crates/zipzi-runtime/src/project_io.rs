//! Project operations use the SDK lifecycle and codecs on host-owned workers.
use crate::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(rename_all = "snake_case")]
pub enum ImportFormat {
    Model3,
    Psd,
    Cubism,
    Moc3,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Package,
    Cmo3,
    Can3,
    Cubism,
    Motion3,
    Expression3,
    Physics3,
    Pose3,
    Cdi3,
}
#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectRequest {
    Diagnose,
    Import {
        /// Host-side filesystem path for this IO request; resolve it explicitly on the host.
        path: PathBuf,
        /// Explicit import/export format; does not change based on the installed backend.
        format: ImportFormat,
        /// Explicit external texture-slot to host-file mappings used by imports requiring
        /// separate images.
        textures: Vec<TextureSlot>,
    },
    Open {
        /// Host-side filesystem path for this IO request; resolve it explicitly on the host.
        path: PathBuf,
    },
    New {
        /// Stable document UUID; not a Runtime Session handle.
        document_id: String,
        /// Model source-canvas and export-frame configuration.
        setup: zipzi_core::ModelSetup,
    },
    Export {
        /// Host-side filesystem path for this IO request; resolve it explicitly on the host.
        path: PathBuf,
        /// Explicit import/export format; does not change based on the installed backend.
        format: ExportFormat,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Stable document ID of the object described or selected by this value.
        object_id: Option<String>,
        /// Explicit permission to export despite format-conversion losses; false rejects
        /// unsupported loss.
        allow_loss: bool,
        /// Whether export may bake animation into the destination representation.
        bake_animation: bool,
        /// Whether export may choose a frame rate automatically instead of requiring an
        /// explicit one.
        auto_frame_rate: bool,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
pub struct TextureSlot {
    /// Zero-based external texture slot number used by the import format.
    pub slot: usize,
    /// Host-side image file path supplying this external texture slot.
    pub path: PathBuf,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct ProjectDiagnostic {
    /// Diagnostic severity label; inspect code for stable programmatic handling.
    pub severity: String,
    /// Machine-readable reason or diagnostic code; use it instead of matching human-readable text.
    pub code: String,
    /// Stable document object ID related to this diagnostic; empty if no object was attributed.
    pub object_id: String,
    /// Human-readable diagnostic detail; code is the stable programmatic discriminator.
    pub message: String,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryOutput,
)]
pub struct ProjectReport {
    /// Exact SDK version before the operation.
    pub before: zipzi_sdk::Version,
    /// Exact SDK version after the operation; may equal before for a no-op or nonpublishing
    /// operation.
    pub after: zipzi_sdk::Version,
    /// Nonfatal operation warnings retained even when an output was produced.
    pub warnings: Vec<String>,
    /// Structured diagnostics from this query or operation; an empty list means none were reported.
    pub diagnostics: Vec<ProjectDiagnostic>,
    /// Whether the operation published its result; inspect durable separately for persistence
    /// guarantees.
    pub published: bool,
    /// Whether the reported persistence step achieved its durability guarantee; job completion
    /// alone does not imply true.
    pub durable: bool,
    /// Host-side output paths actually reported by the operation; no client-side path
    /// translation is implied.
    pub paths: Vec<String>,
}
impl ProjectReport {
    fn result(
        before: zipzi_sdk::Version,
        after: zipzi_sdk::Version,
        value: zipzi_project::ProjectResult,
    ) -> Self {
        Self {
            before,
            after,
            warnings: value.warnings,
            diagnostics: value
                .diagnostics
                .into_iter()
                .map(|d| ProjectDiagnostic {
                    severity: "warning".into(),
                    code: d.code,
                    object_id: d.asset_id,
                    message: d.message,
                })
                .collect(),
            published: value.published,
            durable: value.durable,
            paths: vec![],
        }
    }
}
pub(crate) fn execute(
    session: &mut zipzi_sdk::AuthoringSession,
    request: ProjectRequest,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<ProjectReport> {
    use std::sync::atomic::Ordering;
    use zipzi_project::*;
    let before = session.version();
    let mut report = match request {
        ProjectRequest::Diagnose => {
            let diagnostics = session
                .diagnose_resources()
                .into_iter()
                .map(|d| ProjectDiagnostic {
                    severity: "warning".into(),
                    code: d.code,
                    object_id: d.asset_id,
                    message: d.message,
                })
                .collect();
            ProjectReport {
                before,
                after: before,
                warnings: vec![],
                diagnostics,
                published: false,
                durable: true,
                paths: vec![],
            }
        }
        ProjectRequest::Import {
            path,
            format,
            textures,
        } => {
            let value = match format {
                ImportFormat::Model3 => {
                    let r = session.import_model3(&path, Some(before))?;
                    let mut p = r.project;
                    p.warnings.extend(r.report.warnings);
                    p
                }
                ImportFormat::Psd => {
                    let r = session.import_psd_in_memory(&path, Some(before))?;
                    let mut p = r.project;
                    p.warnings.extend(r.report.warnings);
                    p
                }
                ImportFormat::Cubism => {
                    session.import_cubism(&path, Some(before))?;
                    ProjectResult::ok()
                }
                ImportFormat::Moc3 => {
                    let r = session.import_bare_moc3(
                        &path,
                        &textures.into_iter().map(|s| (s.slot, s.path)).collect(),
                        Some(before),
                    )?;
                    let mut p = r.project;
                    p.warnings.extend(r.report.warnings);
                    p
                }
            };
            ProjectReport::result(before, session.version(), value)
        }
        ProjectRequest::Open { path } => {
            let result = session.open_project(&path, Some(before))?;
            ProjectReport::result(before, session.version(), result)
        }
        ProjectRequest::New { document_id, setup } => {
            session.new_project(&document_id, setup, Some(before))?;
            ProjectReport::result(before, session.version(), ProjectResult::ok())
        }
        ProjectRequest::Export {
            path,
            format,
            name,
            object_id,
            allow_loss,
            bake_animation,
            auto_frame_rate,
        } => {
            let loss_policy = if allow_loss {
                LossPolicy::Allow
            } else {
                LossPolicy::Reject
            };
            let model = Cmo3Options {
                model_name: name.clone(),
                loss_policy,
                ..Default::default()
            };
            let mut diagnostics = vec![];
            let publication = match format {
                ExportFormat::Package => {
                    return session
                        .export_package(&path, Some(before))
                        .map(|r| ProjectReport::result(before, before, r))
                        .map_err(Into::into);
                }
                ExportFormat::Cmo3 => {
                    let a = session
                        .encode_cmo3(&model)
                        .map_err(|e| codec_error(e, before))?;
                    check_cancel(cancel)?;
                    diagnostics = a.report.diagnostics.clone();
                    publish_cmo3(&a, &path).map_err(|e| codec_error(e, before))?
                }
                ExportFormat::Can3 => {
                    let a = session
                        .encode_can3(&Can3Options {
                            animation_name: name,
                            loss_policy,
                            bake_animation,
                            auto_frame_rate,
                            ..Default::default()
                        })
                        .map_err(|e| codec_error(e, before))?;
                    check_cancel(cancel)?;
                    diagnostics = a.report.diagnostics.clone();
                    publish_can3(&a, &path).map_err(|e| codec_error(e, before))?
                }
                ExportFormat::Cubism => {
                    let a = session
                        .encode_cubism(&CubismOptions {
                            base_name: name,
                            model,
                            bake_animation,
                            auto_frame_rate,
                        })
                        .map_err(|e| codec_error(e, before))?;
                    check_cancel(cancel)?;
                    diagnostics = a.report.diagnostics.clone();
                    publish_cubism(&a, &path).map_err(|e| codec_error(e, before))?
                }
                _ => {
                    let id = object_id.as_deref().unwrap_or("");
                    let text = match format {
                        ExportFormat::Motion3 => session.export_motion3(id)?,
                        ExportFormat::Expression3 => session.export_expression3(id)?,
                        ExportFormat::Physics3 | ExportFormat::Pose3 => {
                            let value = if matches!(format, ExportFormat::Physics3) {
                                session.export_physics3()?
                            } else {
                                session.export_pose3()?
                            };
                            match value {
                                Some(value) => value,
                                None => {
                                    return Ok(ProjectReport {
                                        before,
                                        after: before,
                                        warnings: vec![],
                                        diagnostics: vec![ProjectDiagnostic {
                                            severity: "info".into(),
                                            code: "NO_CONTENT".into(),
                                            object_id: id.into(),
                                            message:
                                                "No source content is configured for this sidecar"
                                                    .into(),
                                        }],
                                        published: false,
                                        durable: false,
                                        paths: vec![],
                                    });
                                }
                            }
                        }
                        ExportFormat::Cdi3 => session.export_cdi3()?,
                        _ => unreachable!(),
                    };
                    check_cancel(cancel)?;
                    zipzi_project::publish_export_bytes(text.as_bytes(), &path)
                        .map_err(|e| codec_error(e, before))?
                }
            };
            ProjectReport {
                before,
                after: before,
                warnings: publication.warnings.clone(),
                diagnostics: diagnostics
                    .into_iter()
                    .map(|d| ProjectDiagnostic {
                        severity: format!("{:?}", d.severity).to_lowercase(),
                        code: d.code,
                        object_id: d.object_id,
                        message: d.message,
                    })
                    .collect(),
                published: true,
                durable: publication.durable(),
                paths: vec![path.to_string_lossy().into_owned()],
            }
        }
    };
    // An import that has published cannot be reported as a cancellation.
    if cancel.load(Ordering::Acquire) && !report.published {
        return Err(RuntimeError::Cancelled);
    }
    report.after = session.version();
    Ok(report)
}
fn check_cancel(cancel: &std::sync::atomic::AtomicBool) -> Result<()> {
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        Err(RuntimeError::Cancelled)
    } else {
        Ok(())
    }
}
fn codec_error(error: zipzi_project::Cmo3Error, before: zipzi_sdk::Version) -> RuntimeError {
    if let zipzi_project::Cmo3Error::Unsupported(value) = &error {
        return RuntimeError::Export {
            message: error.to_string(),
            report: Box::new(ProjectReport {
                before,
                after: before,
                warnings: vec![],
                diagnostics: value
                    .diagnostics
                    .iter()
                    .map(|d| ProjectDiagnostic {
                        severity: format!("{:?}", d.severity).to_lowercase(),
                        code: d.code.clone(),
                        object_id: d.object_id.clone(),
                        message: d.message.clone(),
                    })
                    .collect(),
                published: false,
                durable: false,
                paths: vec![],
            }),
        };
    }

    RuntimeError::Sdk(zipzi_sdk::SdkError {
        code: "EXPORT_FAILED".into(),
        message: error.to_string().into(),
        operation: "project.export",
        object_ids: vec![],
        field_path: None,
        expected_version: None,
        actual_version: None,
        referrers: Box::new([]),
    })
}

//! Project operations use the SDK lifecycle and codecs on host-owned workers.
use crate::*;

pub use zipzi_contract::project::{
    ExportFormat, ImportFormat, ProjectDiagnostic, ProjectReport, ProjectRequest, TextureSlot,
};

fn project_report_result(
    before: zipzi_sdk::Version,
    after: zipzi_sdk::Version,
    value: zipzi_project::ProjectResult,
) -> ProjectReport {
    ProjectReport {
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
            project_report_result(before, session.version(), value)
        }
        ProjectRequest::Open { path } => {
            let result = session.open_project(&path, Some(before))?;
            project_report_result(before, session.version(), result)
        }
        ProjectRequest::New { document_id, setup } => {
            session.new_project(&document_id, setup, Some(before))?;
            project_report_result(before, session.version(), ProjectResult::ok())
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
                        .map(|r| project_report_result(before, before, r))
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

//! CPU metadata planning shared by admission and synchronous estimates.
use super::*;
use crate::observation::{error, ObservationViewRuntime};
use zipzi_contract::runtime::{
    ObservationEstimate, ObservationEstimateIssue, ObservationTexturePlan,
    ObservationTexturePlanning,
};
use zipzi_observe::planning::{estimated_mipmap_bytes, plan_textures, texture_sources};

pub(super) struct ObservationCost {
    pub planning: ObservationTexturePlanning,
    pub workspace: usize,
    pub gpu: usize,
    pub output: usize,
    pub task: usize,
    pub issues: Vec<ObservationEstimateIssue>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thin_output_reserves_aligned_readback_rows_without_a_backend() {
        let doc = Document::new();
        let view = crate::ObservationView {
            width: 1,
            height: 4096,
            roi: [0., 0., 1., 1.],
            padding_model: 0.,
            topology: None,
            quality: false,
            texture_policy: Default::default(),
        };
        let cost = Runtime::default()
            .observation_cost(&doc, 1, &view, |_| false)
            .unwrap();
        assert!(cost.gpu >= 256 * 4096 + 4 * 4096);
    }
    #[test]
    fn estimates_do_not_leave_an_image_pool_admission_watermark_or_reservation() {
        let mut runtime = Runtime::default();
        let client = runtime.register_client().unwrap();
        let session = runtime
            .create_session(
                client,
                "79000000-0000-4000-8000-000000000001",
                zipzi_core::ModelSetup::default(),
            )
            .unwrap();
        let view = crate::ObservationView {
            width: 16,
            height: 16,
            roi: [0., 0., 1., 1.],
            padding_model: 0.,
            topology: None,
            quality: false,
            texture_policy: Default::default(),
        };
        let bytes = runtime.accounted_bytes();
        let available = runtime.image_budget.available_bytes();
        let gpu = runtime.runtime_capabilities().gpu;
        for _ in 0..3 {
            runtime
                .estimate_observation(
                    client,
                    session,
                    None,
                    vec![Default::default()],
                    view.clone(),
                )
                .unwrap();
        }
        assert_eq!(runtime.accounted_bytes(), bytes);
        assert_eq!(runtime.image_budget.available_bytes(), available);
        assert_eq!(runtime.runtime_capabilities().gpu, gpu);
    }
}

impl ObservationCost {
    pub fn require_plan(&self) -> Result<&ObservationTexturePlan> {
        if let Some(issue) = self.issues.first() {
            return Err(error(zipzi_observe::ObservationError {
                code: issue.code.clone(),
                message: format!(
                    "{} (requested {:?}, limit {:?})",
                    issue.message, issue.requested, issue.limit
                ),
                asset_id: issue.asset_id.clone(),
            }));
        }
        let ObservationTexturePlanning::Ready { plan } = &self.planning else {
            unreachable!("infeasible plan has a diagnostic")
        };
        Ok(plan)
    }
}

impl Runtime {
    pub(super) fn observation_memory_image(
        &self,
        source: SourceRef,
        id: &str,
    ) -> Option<zipzi_project::PreparedImageContent> {
        match source {
            SourceRef::Edit { edit, .. } => self.edits.get(&edit)?.sdk.as_ref()?.memory_image(id),
            SourceRef::Formal { session, .. } => {
                let entry = self.sessions.get(&session)?;
                if let Some(sdk) = &entry.sdk {
                    sdk.memory_image(id)
                } else {
                    entry.frozen.as_ref()?.memory_image(id).cloned()
                }
            }
        }
    }

    pub(super) fn observation_cost(
        &self,
        doc: &Document,
        frames: usize,
        view: &crate::ObservationView,
        is_memory: impl Fn(&str) -> bool,
    ) -> Result<ObservationCost> {
        view.validate()?;
        if frames == 0 {
            return Err(RuntimeError::Budget("observation samples"));
        }
        let limits = &self.limits;
        // Validate even an empty reference set before building metadata tables.
        plan_textures(
            [],
            view.texture_policy.clone(),
            limits.observation_texture_rgba_bytes,
        )
        .map_err(error)?;
        let mut ids = std::collections::BTreeSet::new();
        let mut metadata = 1024usize;
        for mesh in doc.mesh_order().iter().filter_map(|id| doc.get_mesh(id)) {
            if let Some(id) = mesh.texture_asset_id.resolved_id() {
                if ids.insert(id) {
                    metadata = metadata.saturating_add(1024);
                    if let Some(asset) = doc.get_asset(id) {
                        metadata =
                            metadata.saturating_add(
                                asset
                                    .id
                                    .len()
                                    .saturating_add(asset.name.len())
                                    .saturating_add(asset.content.as_ref().map_or(0, |c| {
                                        c.source.len().saturating_add(c.sha256.len())
                                    }))
                                    .saturating_mul(4),
                            );
                    }
                    if metadata > limits.observation_metadata_bytes {
                        return Err(RuntimeError::Budget("observation metadata"));
                    }
                }
            }
        }
        if metadata > limits.observation_metadata_bytes {
            return Err(RuntimeError::Budget("observation metadata"));
        }
        let sources = texture_sources(doc).map_err(error)?;
        let planning = plan_textures(
            sources.clone(),
            view.texture_policy.clone(),
            limits.observation_texture_rgba_bytes,
        )
        .map_err(error)?;
        let mut issues = Vec::new();
        let mut issue =
            |code: &str, message: &str, asset_id: Option<String>, requested: u64, limit: u64| {
                issues.push(ObservationEstimateIssue {
                    code: code.into(),
                    message: message.into(),
                    asset_id,
                    requested: Some(requested),
                    limit: Some(limit),
                });
            };
        if frames > limits.observation_frames {
            issue(
                "OBSERVATION_FRAME_LIMIT",
                "Observation frame count exceeds host policy",
                None,
                frames as u64,
                limits.observation_frames as u64,
            );
        }
        if view.width.max(view.height) > limits.observation_max_dimension {
            issue(
                "OUTPUT_SIZE_LIMIT",
                "Output dimensions exceed host policy",
                None,
                view.width.max(view.height) as u64,
                limits.observation_max_dimension as u64,
            );
        }
        let (base, mips) = match &planning {
            ObservationTexturePlanning::Ready { plan } => {
                (plan.render_rgba_bytes, plan.estimated_mipmap_bytes)
            }
            ObservationTexturePlanning::InsufficientBudget {
                source_rgba_bytes,
                minimum_required_rgba_bytes,
                effective_max_rgba_bytes,
                ..
            } => {
                issue(
                    "OBSERVATION_BUDGET_EXCEEDED",
                    "Base RGBA texture budget is insufficient",
                    None,
                    *minimum_required_rgba_bytes,
                    *effective_max_rgba_bytes,
                );
                let mut mips = 0u64;
                for s in &sources {
                    mips =
                        mips.saturating_add(estimated_mipmap_bytes(s.source_size).map_err(error)?);
                }
                (*source_rgba_bytes, mips)
            }
        };
        let bytes = |n: u64| usize::try_from(n).unwrap_or(usize::MAX);
        let mut workspace = 0usize;
        let mut shared_base = 0usize;
        for (index, source) in sources.iter().enumerate() {
            let size = source.source_size;
            let source_bytes = bytes(
                u64::from(size.width)
                    .saturating_mul(u64::from(size.height))
                    .saturating_mul(4),
            );
            let target = match &planning {
                ObservationTexturePlanning::Ready { plan } => plan.textures[index].render_size,
                _ => size,
            };
            let render_bytes = bytes(
                u64::from(target.width)
                    .saturating_mul(u64::from(target.height))
                    .saturating_mul(4),
            );
            let memory = is_memory(&source.asset_id);
            if memory && size == target {
                shared_base = shared_base.saturating_add(render_bytes);
            }
            let alpha = if view.quality {
                let step = size.width.max(size.height).div_ceil(256).max(1);
                (size.width.div_ceil(step) as usize)
                    .saturating_mul(size.height.div_ceil(step) as usize)
            } else {
                0
            };
            let derive = (if memory { 0 } else { source_bytes })
                .saturating_add(if size != target {
                    render_bytes.saturating_mul(2)
                } else {
                    0
                })
                .saturating_add(alpha.saturating_mul(2))
                .saturating_add(4096);
            let decode = if memory {
                0
            } else {
                zipzi_core::image::png_decode_workspace_bytes(
                    size.width,
                    size.height,
                    limits.observation_encoded_bytes,
                )
                .map_err(|s| {
                    error(zipzi_observe::ObservationError {
                        code: s.code,
                        message: s.message,
                        asset_id: Some(source.asset_id.clone()),
                    })
                })?
            };
            let required = derive.max(decode);
            if required > limits.observation_workspace_bytes {
                issue(
                    "DECODE_WORKSPACE_LIMIT",
                    "Single-texture decode/downsample workspace exceeds host policy",
                    Some(source.asset_id.clone()),
                    required as u64,
                    limits.observation_workspace_bytes as u64,
                );
            }
            workspace = workspace.max(required);
        }
        let pixels = (view.width as usize).saturating_mul(view.height as usize);
        let rgba = pixels.saturating_mul(4);
        let png =
            zipzi_core::image::rgba_png_capacity(view.width, view.height).unwrap_or(usize::MAX);
        let output = rgba.saturating_add(png).saturating_mul(frames);
        if output as u64 > limits.observation_output_bytes {
            issue(
                "OBSERVATION_OUTPUT_LIMIT",
                "RGBA and bounded PNG output exceed host policy",
                None,
                output as u64,
                limits.observation_output_bytes,
            );
        }
        // Render planning caps all mask/offscreen attachments at 512 MiB.
        // Without evaluating poses, reserve that upper bound for scenes needing
        // attachments; ordinary unmasked normal-blend scenes need only targets.
        let attachments = if !doc.offscreen_order().is_empty()
            || doc
                .mesh_order()
                .iter()
                .filter_map(|id| doc.get_mesh(id))
                .any(|m| !m.masks.is_empty() || m.blend_mode != zipzi_core::BlendMode::Normal)
        {
            zipzi_core::OFFSCREEN_BUDGET_BYTES as usize
        } else {
            0
        };
        let texture_bytes = bytes(base.saturating_add(mips));
        // WGPU buffer copies align scanlines to 256 bytes. This bound also
        // covers the native Metal path without requiring a device or backend
        // dependency; thin outputs can need far more than width*height*4.
        let readback = (view.width as usize)
            .saturating_mul(4)
            .div_ceil(256)
            .saturating_mul(256)
            .saturating_mul(view.height as usize);
        let gpu = texture_bytes
            .saturating_add(attachments)
            .saturating_add(rgba.saturating_mul(3))
            .saturating_add(readback);
        let retained_cpu =
            bytes(base)
                .saturating_sub(shared_base)
                .saturating_add(if view.quality {
                    sources.len().saturating_mul(256 * 256)
                } else {
                    0
                });
        let texture_peak = retained_cpu
            .saturating_add(workspace.max(gpu.saturating_add(texture_bytes.saturating_mul(3))));
        let overlay = view.topology.as_ref().map_or(0, |o| {
            pixels
                .saturating_mul(if o.strain && o.heat_opacity > 0. {
                    8
                } else {
                    0
                })
                .saturating_add(o.max_labels.saturating_mul(1024))
                .saturating_add(o.targets.as_ref().map_or(0, Vec::len).saturating_mul(256))
        });
        let task = Self::sample_bytes(doc, frames)
            .saturating_add(if view.needs_quality() {
                Self::quality_bytes(doc, frames)
            } else {
                0
            })
            .saturating_add(overlay)
            .saturating_mul(4)
            .saturating_add(
                doc.estimated_content_bytes()
                    .saturating_mul(frames)
                    .saturating_mul(8),
            )
            .saturating_add(texture_peak)
            .saturating_add(metadata.saturating_mul(4))
            .saturating_add(output.saturating_mul(4))
            .saturating_add(1 << 20);
        Ok(ObservationCost {
            planning,
            workspace,
            gpu,
            output,
            task,
            issues,
        })
    }

    pub(super) fn observation_estimate_for(
        &self,
        client: ClientId,
        doc: &Document,
        source: SourceRef,
        cost: ObservationCost,
        extra: usize,
        preview: Option<(u64, u64)>,
    ) -> ObservationEstimate {
        let document = doc.estimated_content_bytes();
        let source_charge = if preview.is_some() {
            document.saturating_add(512)
        } else {
            document.saturating_mul(2).saturating_add(1536)
        };
        // check_bytes changes the pool's host watermark. Estimation must be
        // read-only, including the capacity available to later image uploads.
        let runtime_available = self.check_job_slot(client).is_ok()
            && self
                .accounted_bytes()
                .checked_add(
                    cost.task
                        .saturating_add(extra)
                        .saturating_add(source_charge),
                )
                .is_some_and(|n| n <= self.limits.accounted_bytes)
            && self
                .frozen_bytes()
                .checked_add(document)
                .is_some_and(|n| n <= self.limits.frozen_bytes);
        let mut issues = cost.issues;
        let gpu_supported = cfg!(feature = "gpu-observation");
        if !gpu_supported {
            issues.push(ObservationEstimateIssue {
                code: "GPU_UNAVAILABLE".into(),
                message: "Host was built without GPU observation".into(),
                asset_id: None,
                requested: None,
                limit: None,
            });
        }
        let static_feasible = issues.is_empty();
        if !runtime_available {
            issues.push(ObservationEstimateIssue {
                code: "RESOURCE_BUDGET_EXCEEDED".into(),
                message: "Current Runtime byte, frozen-source or Job capacity is insufficient"
                    .into(),
                asset_id: None,
                requested: Some(
                    cost.task
                        .saturating_add(extra)
                        .saturating_add(source_charge) as u64,
                ),
                limit: Some(self.limits.accounted_bytes as u64),
            });
        }
        ObservationEstimate {
            source,
            preview_id: preview.map(|p| p.0),
            sequence: preview.map(|p| p.1),
            texture_planning: cost.planning,
            texture_workspace_bytes: cost.workspace as u64,
            gpu_working_set_bytes: cost.gpu as u64,
            output_bytes: cost.output as u64,
            task_bytes: cost
                .task
                .saturating_add(extra)
                .saturating_add(source_charge) as u64,
            static_feasible,
            runtime_available,
            gpu_supported,
            device_max_texture_dimension: None,
            device_verified: false,
            source_identities_verified: false,
            issues,
        }
    }

    pub fn estimate_observation(
        &self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        parameters: Vec<zipzi_core::PreviewValues>,
        view: crate::ObservationView,
    ) -> Result<ObservationEstimate> {
        if parameters
            .iter()
            .any(|p| p.len() > 256 || p.values().any(|v| !v.is_finite()))
        {
            return Err(RuntimeError::Budget("observation parameters"));
        }
        self.with_authoring_source(client, session, edit, |doc, source| {
            let cost = self.observation_cost(doc, parameters.len(), &view, |id| {
                self.observation_memory_image(source, id).is_some()
            })?;
            Ok(self.observation_estimate_for(client, doc, source, cost, 0, None))
        })
    }
}

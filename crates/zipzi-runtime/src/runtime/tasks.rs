//! Domain work consumes the exact captured source, with one pose per read quantum.
use super::*;
use std::io::Write;
use zipzi_contract::kernel_tasks::{ExportFormat, SampleInput, SampleRequest};

struct Bytes {
    data: Vec<u8>,
    limit: usize,
}
impl Bytes {
    fn new(limit: usize) -> Self {
        Self {
            data: Vec::new(),
            limit,
        }
    }
    fn json(&mut self, value: &impl serde::Serialize) -> Result<()> {
        serde_json::to_writer(self, value).map_err(|e| error("RESULT_LIMIT", &e.to_string()))
    }
    fn raw(&mut self, bytes: &[u8]) -> Result<()> {
        self.write_all(bytes)
            .map_err(|e| error("RESULT_LIMIT", &e.to_string()))
    }
}
impl Write for Bytes {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let end = self
            .data
            .len()
            .checked_add(bytes.len())
            .filter(|n| *n <= self.limit)
            .ok_or_else(|| std::io::Error::other("Task result exceeds admitted bytes"))?;
        if end > self.data.capacity() {
            let capacity = end
                .max(self.data.capacity().saturating_mul(2))
                .min(self.limit);
            self.data
                .try_reserve_exact(capacity - self.data.len())
                .map_err(std::io::Error::other)?;
        }
        self.data.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn validate(input: &SampleInput) -> Result<usize> {
    let count = match input {
        SampleInput::Parameters { values } => {
            if values
                .iter()
                .any(|p| p.len() > 256 || p.values().any(|v| !v.is_finite()))
            {
                return Err(error(
                    "INVALID_SAMPLES",
                    "Parameter limit or nonfinite value",
                ));
            }
            values.len()
        }
        SampleInput::Animation {
            times,
            motions,
            expressions,
            inputs,
            ..
        } => {
            if motions.len() + expressions.len() + inputs.len() > 256
                || times
                    .iter()
                    .any(|t| !t.is_finite() || *t < 0.0 || *t > 600.0)
            {
                return Err(error("INVALID_SAMPLES", "Animation schedule or time limit"));
            }
            times.len()
        }
    };
    if count == 0 || count > 64 {
        return Err(error("RESOURCE_LIMIT", "Expected 1..64 samples"));
    }
    Ok(count)
}
fn animation_error(e: zipzi_animation::AnimationError) -> zipzi_sdk::Status {
    error("ANIMATION_SAMPLE", &e.to_string())
}
fn preview(
    source: &SourcedSnapshot,
    input: &SampleInput,
) -> Result<Option<zipzi_animation::MotionPreview>> {
    let SampleInput::Animation {
        motions,
        expressions,
        inputs,
        physics,
        stabilized_start,
        ..
    } = input
    else {
        return Ok(None);
    };
    let mut preview = zipzi_animation::MotionPreview::new(source.snapshot.document());
    preview.bind_source(source.source);
    preview.set_physics_enabled(*physics);
    preview.set_physics_stabilized_start(*stabilized_start);
    for (id, time) in motions {
        preview
            .schedule_motion(id, *time)
            .map_err(animation_error)?;
    }
    for (id, time) in expressions {
        preview
            .schedule_expression(id, *time)
            .map_err(animation_error)?;
    }
    for input in inputs {
        preview
            .schedule_parameter_input(&input.id, input.time, input.value)
            .map_err(animation_error)?;
    }
    Ok(Some(preview))
}
fn seek(
    preview: &mut zipzi_animation::MotionPreview,
    input: &SampleInput,
    index: usize,
    cancel: &Cancellation,
) -> Result<()> {
    let SampleInput::Animation { times, .. } = input else {
        unreachable!()
    };
    preview
        .seek_with_progress(times[index], |_, _| !cancel.is_cancelled())
        .map_err(animation_error)?;
    Ok(())
}
impl Runtime {
    pub fn request_samples(
        &mut self,
        client: ClientId,
        source: QuerySource,
        request: SampleRequest,
    ) -> Result<JobId> {
        let count = validate(&request.input)?;
        sampling_admission(&self.capture(client, source)?)?;
        if request.queries.len() > 64
            || request.queries.iter().any(|q| match q {
                zipzi_sdk::SpatialQuery::Bounds { targets, .. } => targets.len() > 4096,
                zipzi_sdk::SpatialQuery::Hits { max_candidates, .. } => *max_candidates > 4096,
            })
        {
            return Err(error("RESOURCE_LIMIT", "Spatial selection limit"));
        }
        let limit = self.limits.result_bytes.min(32 << 20);
        let mut output = Bytes::new(limit);
        let mut index = 0;
        let mut animation = None;
        let mut reference = None;
        self.request_read_steps(
            client,
            source,
            TaskBudget {
                workspace_bytes: 128 << 20,
                result_bytes: limit,
            },
            move |snapshot, cancel| {
                cancel.check()?;
                if index == 0 {
                    zipzi_sdk::validate_spatial_queries(
                        snapshot.snapshot.document(),
                        &request.queries,
                        &cancel.conversion_control(),
                    )?;
                    animation = preview(snapshot, &request.input)?;
                    if request.quality {
                        reference = Some(snapshot.snapshot.quality_reference()?);
                    }
                    output.raw(b"{\"source\":")?;
                    output.json(&source)?;
                    output.raw(b",\"content\":")?;
                    output.json(&snapshot.snapshot.content_id())?;
                    output.raw(b",\"frames\":[")?;
                }
                let frame = if let Some(preview) = &mut animation {
                    seek(preview, &request.input, index, cancel)?;
                    preview
                        .evaluate_drawables_including_hidden()
                        .map_err(animation_error)?
                } else {
                    let SampleInput::Parameters { values } = &request.input else {
                        unreachable!()
                    };
                    let mut frame = zipzi_core::DrawableFrame::default();
                    let status = zipzi_core::evaluation::evaluate_frame_including_hidden(
                        snapshot.snapshot.document(),
                        &values[index],
                        &mut frame,
                    );
                    if !status.is_ok() {
                        return Err(status);
                    }
                    frame
                };
                let spatial = zipzi_sdk::query_spatial_frame(
                    snapshot.snapshot.document(),
                    &frame,
                    &request.queries,
                    &cancel.conversion_control(),
                )?;
                let quality = reference
                    .as_ref()
                    .map(|r| zipzi_sdk::mesh_quality(&frame, r, &cancel.conversion_control()))
                    .transpose()?;
                if index > 0 {
                    output.raw(b",")?;
                }
                output.raw(b"{\"geometry\":")?;
                output.json(&frame)?;
                output.raw(b",\"spatial\":")?;
                output.json(&spatial)?;
                output.raw(b",\"quality\":")?;
                output.json(&quality)?;
                output.raw(b",\"animation\":")?;
                output.json(&animation.as_ref().map(|p| p.snapshot()))?;
                output.raw(b"}")?;
                index += 1;
                if index < count {
                    return Ok(ReadProgress::Yield);
                }
                output.raw(b"]}")?;
                Ok(ReadProgress::Complete(std::mem::take(&mut output.data)))
            },
        )
    }
}
#[cfg(feature = "gpu-observation")]
fn observation_error(e: zipzi_observe::ObservationError) -> zipzi_sdk::Status {
    error(&e.code, &e.message)
}
impl Runtime {
    /// Output is a sequence of [u32 big-endian JSON length, JSON, PNG bytes].
    /// Each header includes png_bytes and exact source/content provenance.
    pub fn request_observation(
        &mut self,
        client: ClientId,
        source: QuerySource,
        input: SampleInput,
        view: zipzi_contract::runtime::ObservationView,
    ) -> Result<JobId> {
        #[cfg(not(feature = "gpu-observation"))]
        {
            let _ = (client, source, input, view);
            Err(error(
                "UNSUPPORTED_OPERATION",
                "Host has no GPU observation service",
            ))
        }
        #[cfg(feature = "gpu-observation")]
        {
            use zipzi_observe::{
                ModelRoi, ObservationInput, Observer, ObserverConfig, RenderRequest,
                ResolvedObservation,
            };
            let count = validate(&input)?;
            if view.width == 0 || view.height == 0 || view.width > 4096 || view.height > 4096 {
                return Err(error(
                    "RESOURCE_LIMIT",
                    "Observation dimensions must be 1..4096",
                ));
            }
            let request = RenderRequest {
                width: view.width,
                height: view.height,
                roi: ModelRoi {
                    x0: view.roi[0],
                    y0: view.roi[1],
                    x1: view.roi[2],
                    y1: view.roi[3],
                },
                padding_model: view.padding_model,
                topology: view.topology,
                quality: view.quality,
            };
            request.mapping().map_err(observation_error)?;
            let captured = self.capture(client, source)?;
            sampling_admission(&captured)?;
            let sources = zipzi_observe::planning::texture_sources(captured.snapshot.document())
                .map_err(observation_error)?;
            let planning =
                zipzi_observe::planning::plan_textures(sources, view.texture_policy, 64 << 20)
                    .map_err(observation_error)?;
            let zipzi_contract::runtime::ObservationTexturePlanning::Ready { plan } = planning
            else {
                return Err(error(
                    "RESOURCE_LIMIT",
                    "Observation texture budget is insufficient",
                ));
            };
            let limit = self.limits.result_bytes.min(64 << 20);
            let workspace_bytes = 256usize
                .saturating_mul(1 << 20)
                .saturating_add(view.width as usize * view.height as usize * 32);
            self.request_read(client, source, TaskBudget { workspace_bytes, result_bytes: limit }, move |snapshot, cancel| {
                let mut animation = preview(&snapshot, &input)?;
                let mut observer = Observer::new(ObserverConfig { width: view.width, height: view.height }).map_err(observation_error)?;
                let mut out = Bytes::new(limit);
                for index in 0..count {
                    cancel.check()?;
                    let captured = if let Some(preview) = &mut animation {
                        seek(preview, &input, index, cancel)?;
                        ObservationInput::capture_motion_from_snapshot_with_quality(&snapshot, preview, request.needs_quality()).map_err(observation_error)?
                    } else {
                        let SampleInput::Parameters { values } = &input else { unreachable!() };
                        ObservationInput::capture_from_snapshot_with_quality(&snapshot, &values[index], request.needs_quality()).map_err(observation_error)?
                    };
                    let limits = zipzi_observe::ObservationCaptureLimits { max_frames: 1, max_rgba_bytes: 64 << 20, png: zipzi_core::image::PngDecodeLimits { max_encoded_bytes: 64 << 20, max_workspace_bytes: 256 << 20 } };
                    let resolved = ResolvedObservation::capture_many_with_plan(vec![captured], &plan, limits, request.needs_quality(), &cancel.conversion_control()).map_err(observation_error)?;
                    let frame = observer.render(&resolved[0], request.clone()).map_err(observation_error)?;
                    cancel.check()?;
                    let png = frame.png_bytes().map_err(observation_error)?;
                    let mut header = Bytes::new(1 << 20);
                    header.json(&serde_json::json!({"source":source,"content":snapshot.snapshot.content_id(),"index":index,"width":frame.width,"height":frame.height,"png_bytes":png.len(),"input_sha256":frame.input_sha256,"scene_digest":resolved[0].scene_digest(),"parameters":frame.parameters,"quality":frame.quality,"adapter_name":frame.adapter_name,"adapter_backend":frame.adapter_backend,"texture_report":if index == 0 { Some(resolved[0].texture_report()) } else { None },"view":{"roi":[frame.view.visible_roi.x0,frame.view.visible_roi.y0,frame.view.visible_roi.x1,frame.view.visible_roi.y1],"scale":frame.view.scale,"offset":frame.view.offset},"drawable_bounds":frame.drawable_bounds.iter().map(|b| serde_json::json!({"id":b.id,"visible":b.visible,"bounds":b.bounds})).collect::<Vec<_>>()}))?;
                    out.raw(&(header.data.len() as u32).to_be_bytes())?;
                    out.raw(&header.data)?;
                    out.raw(&png)?;
                }
                Ok(out.data)
            })
        }
    }
    pub fn request_export(
        &mut self,
        client: ClientId,
        source: QuerySource,
        path: &Path,
        format: ExportFormat,
        allow_loss: bool,
    ) -> Result<JobId> {
        let snapshot = self.transfer_snapshot(client, source)?;
        if !path.is_absolute() {
            return Err(error("INVALID_DESTINATION", "Export path must be absolute"));
        }
        let path = std::fs::canonicalize(
            path.parent()
                .ok_or_else(|| error("INVALID_DESTINATION", "Export parent is missing"))?,
        )
        .map_err(|e| error("INVALID_DESTINATION", &e.to_string()))?
        .join(
            path.file_name()
                .ok_or_else(|| error("INVALID_DESTINATION", "Export name is missing"))?,
        );
        // Directory exports must not contain any resident project or overlap
        // another pending output (including another client's save/export).
        if self
            .paths
            .keys()
            .chain(self.reservations.keys())
            .any(|p| p.starts_with(&path) || path.starts_with(p))
        {
            return Err(error(
                "PROJECT_BUSY",
                "Export overlaps a resident project or pending output",
            ));
        }
        let leases = self.admit_job(TaskBudget {
            workspace_bytes: 256 << 20,
            result_bytes: 1 << 20,
        })?;
        Ok(self.enqueue(
            client,
            source,
            Task::Export {
                snapshot,
                path: path.clone(),
                format,
                allow_loss,
            },
            leases,
            Some(path),
        ))
    }
}

// Bound the concrete evaluated arrays and detached animation work before queueing.
// These are task-specific record/geometry limits, independent of history size.
fn sampling_admission(snapshot: &SourcedSnapshot) -> Result<()> {
    let document = snapshot.snapshot.document();
    let vertices = document
        .mesh_order()
        .iter()
        .filter_map(|id| document.mesh(id))
        .fold(0usize, |n, mesh| {
            n.saturating_add(mesh.base_positions.len())
        });
    let indices = document
        .mesh_order()
        .iter()
        .filter_map(|id| document.mesh(id))
        .fold(0usize, |n, mesh| n.saturating_add(mesh.triangles.len()));
    let keys = document
        .motion_order()
        .iter()
        .filter_map(|id| document.motion(id))
        .flat_map(|motion| &motion.tracks)
        .fold(0usize, |n, track| {
            n.saturating_add(track.keys().take(65_537).count())
        });
    if document.mesh_order().len() > 4096
        || document.transform_order().len() > 4096
        || document.parameter_order().len() > 4096
        || vertices > 262_144
        || indices > 1_572_864
        || keys > 65_536
    {
        return Err(error(
            "RESOURCE_LIMIT",
            "Sampling geometry or animation workspace limit",
        ));
    }
    Ok(())
}

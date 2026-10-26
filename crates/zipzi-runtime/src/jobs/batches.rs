//! Worker-owned continuations. A yielded batch retains its frozen source,
//! outputs and observer under the original Job's admission reservation.
use super::*;
use std::collections::VecDeque;
use zipzi_sdk_observe::{ObservationInput, Observer, ResolvedObservation};

pub(crate) struct ReadBatch {
    source: crate::SourceSnapshot,
    kind: BatchKind,
}
enum BatchKind {
    Sample {
        pending: VecDeque<zipzi_core::PreviewValues>,
        checks: zipzi_sdk::GeometryChecks,
        output: Option<zipzi_sdk::ModelingSamples>,
    },
    Spatial {
        pending: VecDeque<zipzi_core::PreviewValues>,
        output: zipzi_sdk::SpatialSamples,
    },
    Observe {
        view: crate::observation::ObservationView,
        requested: Vec<zipzi_core::PreviewValues>,
        inputs: Vec<ObservationInput>,
        captures: Option<VecDeque<ResolvedObservation>>,
        observer: Option<Observer>,
        frames: Vec<crate::observation::ObservationImage>,
    },
    Preview(crate::runtime::previews::PreviewSampling),
}
impl ReadBatch {
    pub(super) fn from_task(task: ReadTask) -> std::result::Result<Box<Self>, ReadTask> {
        let (source, kind) = match task {
            ReadTask::Batch(batch) => return Ok(batch),
            ReadTask::Sample {
                source,
                parameters,
                checks,
            } => (
                source,
                BatchKind::Sample {
                    pending: parameters.into(),
                    checks,
                    output: None,
                },
            ),
            ReadTask::Spatial {
                source,
                parameters,
                queries,
            } => {
                let output = zipzi_sdk::SpatialSamples {
                    version: source.snapshot.version(),
                    requested: vec![],
                    queries,
                    frames: vec![],
                };
                (
                    source,
                    BatchKind::Spatial {
                        pending: parameters.into(),
                        output,
                    },
                )
            }
            ReadTask::Observe {
                source,
                parameters,
                view,
            } => (
                source,
                BatchKind::Observe {
                    view,
                    requested: parameters,
                    inputs: vec![],
                    captures: None,
                    observer: None,
                    frames: vec![],
                },
            ),
            ReadTask::Preview {
                source,
                preview_id,
                sequence,
                state,
                times,
                geometry,
                incremental,
                queries,
                view,
            } => (
                source,
                BatchKind::Preview(crate::runtime::previews::PreviewSampling::new(
                    preview_id,
                    sequence,
                    state,
                    times,
                    geometry,
                    incremental,
                    queries,
                    view,
                )),
            ),
            task => return Err(task),
        };
        Ok(Box::new(Self { source, kind }))
    }
    pub(super) fn step(
        &mut self,
        cancel: &AtomicBool,
        control: &zipzi_sdk::ConversionControl,
    ) -> Result<Option<JobValue>> {
        crate::observation::check(cancel)?;
        let source = &self.source;
        let result = match &mut self.kind {
            BatchKind::Sample {
                pending,
                checks,
                output,
            } => {
                if output.is_none() {
                    *output = Some(source.snapshot.sample_modeling_with_control(
                        vec![],
                        *checks,
                        control,
                    )?);
                }
                let samples = output.as_mut().unwrap();
                if let Some(pose) = pending.pop_front() {
                    samples.frames.push(source.snapshot.evaluate(&pose)?);
                    samples.parameters.push(pose);
                }
                if !pending.is_empty() {
                    return Ok(None);
                }
                let state = match source.state {
                    crate::SessionState::Ready => "ready",
                    crate::SessionState::Busy(_) => "busy",
                    crate::SessionState::Faulted(_) => "faulted",
                    _ => "closed",
                };
                JobValue::Sampled(Arc::new(crate::authoring::ModelingSampleBatch {
                    source: source.source,
                    lease: None,
                    state,
                    samples: output.take().unwrap(),
                }))
            }
            BatchKind::Spatial { pending, output } => {
                let poses = pending.pop_front().into_iter().collect();
                let samples = source.snapshot.sample_spatial_with_control(
                    poses,
                    output.queries.clone(),
                    control,
                )?;
                output.requested.extend(samples.requested);
                output.frames.extend(samples.frames);
                if !pending.is_empty() {
                    return Ok(None);
                }
                JobValue::Spatial(Arc::new(crate::spatial::SpatialSampleBatch {
                    source: source.source,
                    samples: zipzi_sdk::SpatialSamples {
                        version: output.version,
                        requested: std::mem::take(&mut output.requested),
                        queries: std::mem::take(&mut output.queries),
                        frames: std::mem::take(&mut output.frames),
                    },
                }))
            }
            BatchKind::Observe {
                view,
                requested,
                inputs,
                captures,
                observer,
                frames,
            } => {
                if captures.is_none() {
                    if let Some(pose) = requested.get(inputs.len()) {
                        inputs.push(
                            ObservationInput::capture_from_snapshot(&source.snapshot, pose)
                                .map_err(crate::observation::error)?,
                        );
                    } else {
                        *captures = Some(
                            ResolvedObservation::capture_many_with_control(
                                std::mem::take(inputs),
                                control,
                            )
                            .map_err(crate::observation::error)?
                            .into(),
                        );
                    }
                    return Ok(None);
                }
                let captures = captures.as_mut().unwrap();
                if observer.is_none() {
                    *observer = Some(crate::observation::observer(*view)?);
                }
                if let Some(capture) = captures.pop_front() {
                    frames.push(crate::observation::render(
                        observer.as_mut().unwrap(),
                        &capture,
                        *view,
                    )?);
                }
                if !captures.is_empty() {
                    return Ok(None);
                }
                JobValue::Observed(Arc::new(crate::observation::ObservationSampleBatch {
                    source: source.source,
                    samples: crate::observation::ObservationBatch {
                        view: *view,
                        requested: std::mem::take(requested),
                        frames: std::mem::take(frames),
                    },
                }))
            }
            BatchKind::Preview(sampling) => return sampling.step(source, cancel, control),
        };
        crate::observation::check(cancel)?;
        Ok(Some(result))
    }
}

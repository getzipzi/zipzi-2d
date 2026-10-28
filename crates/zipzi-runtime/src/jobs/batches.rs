//! Worker-owned continuations. A yielded batch retains its frozen source,
//! outputs and observer under the original Job's admission reservation.
use super::*;
#[cfg(feature = "gpu-observation")]
use std::collections::VecDeque;
#[cfg(feature = "gpu-observation")]
use zipzi_observe::{ObservationInput, Observer, ResolvedObservation};

pub(crate) struct ReadBatch {
    source: crate::SourceSnapshot,
    kind: BatchKind,
    #[cfg(test)]
    pub(crate) drop_probe: Option<std::sync::mpsc::Sender<std::thread::ThreadId>>,
}
#[cfg(test)]
impl Drop for ReadBatch {
    fn drop(&mut self) {
        if let Some(probe) = &self.drop_probe {
            let _ = probe.send(std::thread::current().id());
        }
    }
}
enum BatchKind {
    Sample(zipzi_sdk::ModelingSampler<Arc<zipzi_sdk::AuthoringSnapshot>>),
    Spatial(zipzi_sdk::SpatialSampler<Arc<zipzi_sdk::AuthoringSnapshot>>),
    #[cfg(feature = "gpu-observation")]
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
            } => {
                let sampler =
                    zipzi_sdk::ModelingSampler::new(source.snapshot.clone(), parameters, checks);
                (source, BatchKind::Sample(sampler))
            }
            ReadTask::Spatial {
                source,
                parameters,
                queries,
            } => {
                let sampler =
                    zipzi_sdk::SpatialSampler::new(source.snapshot.clone(), parameters, queries);
                (source, BatchKind::Spatial(sampler))
            }
            #[cfg(feature = "gpu-observation")]
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
        Ok(Box::new(Self {
            source,
            kind,
            #[cfg(test)]
            drop_probe: None,
        }))
    }
    pub(super) fn step(
        &mut self,
        cancel: &AtomicBool,
        control: &zipzi_sdk::ConversionControl,
    ) -> Result<Option<JobValue>> {
        crate::observation::check(cancel)?;
        let source = &self.source;
        let result = match &mut self.kind {
            BatchKind::Sample(sampler) => {
                let Some(samples) = sampler.step(control)? else {
                    return Ok(None);
                };
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
                    samples,
                }))
            }
            BatchKind::Spatial(sampler) => {
                let Some(samples) = sampler.step(control)? else {
                    return Ok(None);
                };
                JobValue::Spatial(Arc::new(crate::spatial::SpatialSampleBatch {
                    source: source.source,
                    samples,
                }))
            }
            #[cfg(feature = "gpu-observation")]
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

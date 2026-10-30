use super::*;
#[cfg(feature = "gpu-observation")]
use crate::jobs::ReadTask;
use crate::observation::ObservationViewRuntime;
use crate::observation::*;

#[cfg(all(test, feature = "gpu-observation"))]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Manual(Vec<Work>);
    impl Executor for Manual {
        fn submit(&mut self, w: Work) -> std::result::Result<(), Box<Work>> {
            self.0.push(w);
            Ok(())
        }
    }
    #[test]
    fn adapter_failure_is_visible_in_capabilities_without_poisoning_cpu_sources() {
        let mut rt = Runtime::default();
        let c = rt.register_client().unwrap();
        let s = rt
            .create_session(
                c,
                "79660000-0000-4000-8000-000000000001",
                zipzi_core::ModelSetup::default(),
            )
            .unwrap();
        let before = rt.session_info(s).unwrap();
        assert_eq!(rt.runtime_capabilities().gpu, None);
        let view = ObservationView {
            width: 8,
            height: 8,
            roi: [0., 0., 1., 1.],
            padding_model: 0.,
            topology: None,
            quality: false,
        };
        let j = rt
            .request_observation(c, s, None, vec![Default::default()], view, None)
            .unwrap();
        let mut exec = Manual::default();
        rt.dispatch(&mut exec);
        let w = exec.0.pop().unwrap();
        let completion = Completion {
            transfer: None,
            job: w.job,
            client: w.client,
            session_id: w.session_id,
            base: w.base,
            session: None,
            outcome: crate::jobs::WorkOutcome::Complete(Err(error(
                zipzi_observe::ObservationError {
                    code: "GPU_ADAPTER".into(),
                    message: "No test device".into(),
                    asset_id: None,
                },
            ))),
        };
        drop(w);
        assert!(rt.complete(completion));
        assert_eq!(rt.runtime_capabilities().gpu, Some(false));
        assert_eq!(
            rt.runtime_capabilities().gpu_error.as_deref(),
            Some("GPU_ADAPTER")
        );
        rt.release_job(c, j).unwrap();
        assert_eq!(rt.session_info(s).unwrap(), before);
        assert!(rt
            .source_snapshot(s)
            .unwrap()
            .snapshot
            .evaluate(&Default::default())
            .is_ok());
    }
}

impl Runtime {
    pub(super) fn observation_bytes(
        doc: &Document,
        frames: usize,
        view: &ObservationView,
    ) -> Result<usize> {
        view.validate()?;
        if frames == 0 || frames > 64 {
            return Err(RuntimeError::Budget("observation samples"));
        }
        let output = (view.width as usize)
            .saturating_mul(view.height as usize)
            .saturating_mul(8)
            .saturating_mul(frames);
        let overlay_workspace = view.topology.as_ref().map_or(0, |overlay| {
            (view.width as usize)
                .saturating_mul(view.height as usize)
                .saturating_mul(if overlay.strain && overlay.heat_opacity > 0. {
                    8
                } else {
                    0
                })
                .saturating_add(overlay.max_labels.saturating_mul(1024))
                .saturating_add(
                    overlay
                        .targets
                        .as_ref()
                        .map_or(0, |targets| targets.len())
                        .saturating_mul(256),
                )
        });
        if output > 128 << 20 {
            return Err(RuntimeError::Budget("observation output"));
        }
        let textures = doc
            .asset_order()
            .iter()
            .filter_map(|id| doc.get_asset(id)?.content.as_ref())
            .fold(0usize, |n, c| {
                n.saturating_add(
                    (c.width as usize)
                        .saturating_mul(c.height as usize)
                        .saturating_mul(4),
                )
            });
        if textures > 256 << 20 {
            return Err(RuntimeError::Budget("observation textures"));
        }
        Ok(Self::sample_bytes(doc, frames)
            .saturating_add(overlay_workspace)
            .saturating_add(if view.needs_quality() {
                Self::quality_bytes(doc, frames)
            } else {
                0
            })
            .saturating_mul(4)
            .saturating_add(
                doc.estimated_content_bytes()
                    .saturating_mul(frames)
                    .saturating_mul(8),
            )
            .saturating_add(textures.saturating_mul(10))
            .saturating_add(output.saturating_mul(4))
            .saturating_add(1 << 20))
    }
    #[cfg(not(feature = "gpu-observation"))]
    pub fn request_observation(
        &mut self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        _parameters: Vec<zipzi_core::PreviewValues>,
        _view: ObservationView,
        expected: Option<SourceRef>,
    ) -> Result<JobId> {
        self.check_authoring_source(client, session, edit, expected)?;
        Err(gpu_unavailable())
    }
    #[cfg(feature = "gpu-observation")]
    pub fn request_observation(
        &mut self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        parameters: Vec<zipzi_core::PreviewValues>,
        view: ObservationView,
        expected: Option<SourceRef>,
    ) -> Result<JobId> {
        self.client(client)?;
        self.check_job_slot(client)?;
        if parameters.iter().any(|p| p.len() > 256) {
            return Err(RuntimeError::Budget("observation parameters"));
        }
        let (document, bytes) =
            self.with_authoring_source(client, session, edit, |doc, source| {
                if expected.is_some_and(|s| s != source) {
                    return Err(RuntimeError::InvalidReference);
                }
                Ok((
                    doc.estimated_content_bytes(),
                    Self::observation_bytes(doc, parameters.len(), &view)?,
                ))
            })?;
        self.check_bytes(bytes.saturating_add(document), 0, document)?;
        let source = if let Some(edit) = edit {
            self.edit_snapshot(client, edit)?
        } else {
            self.source_snapshot(session)?
        };
        let task = ReadTask::Observe {
            source: source.clone(),
            parameters,
            view,
        };
        self.queue_read(client, source, task, bytes, false)
    }
    pub fn observation_response(
        &mut self,
        client: ClientId,
        value: &ObservationSampleBatch,
    ) -> Result<serde_json::Value> {
        use zipzi_core::binary_wire::WireWrite;
        let (blob, length) = self.binary_result_blob(client, &value.samples)?;
        Ok(
            serde_json::json!({"source":value.source,"blob":blob,"length":length,"signature":format!("{:032x}",ObservationBatch::SIGNATURE)}),
        )
    }
}

use super::*;
#[cfg(feature = "gpu-observation")]
use crate::jobs::ReadTask;
use crate::observation::*;

#[cfg(all(test, feature = "gpu-observation"))]
mod tests {
    use super::*;

    #[test]
    fn admission_plans_only_referenced_textures_without_gpu_or_image_io() {
        use zipzi_core::{Canvas, ImageAsset, ImageContent, Mesh, Vec2};
        let id = |n| format!("00000000-0000-4000-8000-{n:012}");
        let mut doc = Document::new();
        assert!(doc
            .initialize(&id(1), Canvas::new(100., 100., Vec2::new(50., 50.), 10.))
            .is_ok());
        let add = |doc: &mut Document, n, size| {
            assert!(doc
                .add_asset(ImageAsset {
                    id: id(n),
                    name: String::new(),
                    unresolved_source: None,
                    content: Some(ImageContent {
                        source: format!("/missing/{n}.png"),
                        width: size,
                        height: size,
                        sha256: String::new()
                    }),
                })
                .status
                .is_ok());
        };
        add(&mut doc, 2, 2);
        // The unreferenced asset exceeds both the policy limit and u64 RGBA.
        add(&mut doc, 3, u32::MAX);
        assert!(doc
            .create_mesh(Mesh {
                id: id(4),
                texture_asset_id: id(2).into(),
                enabled: false,
                ..Default::default()
            })
            .status
            .is_ok());
        let view = ObservationView {
            texture_policy: Default::default(),
            width: 8,
            height: 8,
            roi: [0., 0., 1., 1.],
            padding_model: 0.,
            topology: None,
            quality: false,
        };
        assert!(Runtime::default().observation_bytes(&doc, 1, &view).is_ok());
        // Hidden references still count, even when they are only used for masks.
        assert!(doc
            .create_mesh(Mesh {
                id: id(5),
                texture_asset_id: id(3).into(),
                enabled: false,
                ..Default::default()
            })
            .status
            .is_ok());
        let RuntimeError::Sdk(error) = Runtime::default()
            .observation_bytes(&doc, 1, &view)
            .unwrap_err()
        else {
            panic!("Malformed resource dimensions must be diagnosed")
        };
        assert_eq!(error.code.as_ref(), "RESOURCE_DIMENSIONS");
        assert_eq!(error.object_ids, [id(3)]);
    }
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
            texture_policy: Default::default(),
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
    #[cfg(all(test, feature = "gpu-observation"))]
    pub(super) fn observation_bytes(
        &self,
        doc: &Document,
        frames: usize,
        view: &ObservationView,
    ) -> Result<usize> {
        let cost = self.observation_cost(doc, frames, view, |_| false)?;
        cost.require_plan()?;
        Ok(cost.task)
    }
    #[cfg(not(feature = "gpu-observation"))]
    pub fn request_observation(
        &mut self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        parameters: Vec<zipzi_core::PreviewValues>,
        view: ObservationView,
        expected: Option<SourceRef>,
    ) -> Result<JobId> {
        self.check_authoring_source(client, session, edit, expected)?;
        self.estimate_observation(client, session, edit, parameters, view)?;
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
        let (document, bytes, plan) =
            self.with_authoring_source(client, session, edit, |doc, source| {
                if expected.is_some_and(|s| s != source) {
                    return Err(RuntimeError::InvalidReference);
                }
                let cost = self.observation_cost(doc, parameters.len(), &view, |id| {
                    self.observation_memory_image(source, id).is_some()
                })?;
                let plan = cost.require_plan()?.clone();
                Ok((doc.estimated_content_bytes(), cost.task, plan))
            })?;
        self.check_bytes(
            bytes
                .saturating_add(document.saturating_mul(2))
                .saturating_add(1536),
            0,
            document,
        )?;
        let source = if let Some(edit) = edit {
            self.edit_snapshot(client, edit)?
        } else {
            self.source_snapshot(session)?
        };
        let task = ReadTask::Observe {
            source: source.clone(),
            parameters,
            view,
            observation: crate::observation::ObservationWork {
                plan,
                limits: crate::observation::capture_limits(&self.limits),
            },
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

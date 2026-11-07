use super::common::*;
use std::collections::VecDeque;
use zipzi_contract::kernel_tasks::{ParameterActivation, SampleInput, SampleRequest};
use zipzi_editor_core::*;
use zipzi_runtime::{Executor, QuerySource, Work};

#[derive(Default)]
struct Manual(VecDeque<Work>);
impl Executor for Manual {
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
        self.0.push_back(work);
        Ok(())
    }
}

#[test]
fn r5_agent_batches_do_not_change_human_playback_physics_sliders_or_two_viewports()
-> anyhow::Result<()> {
    for manual_physics in [false, true] {
        let (document, _assets) = crate::support::combined_animation_document()?;
        let mut editor = crate::support::editor_from_document(document);
        editor.set_workspace(WorkspaceKind::Animation)?;
        let document = editor.active_id().unwrap();
        editor.set_animation_preview_mode(document, AnimationPreviewMode::Result)?;
        let motion = editor.document().unwrap().motion_resources()[0]
            .selection
            .clone();
        let parameter = editor.document().unwrap().parameters()[0].id.clone();
        editor.select_motion(document, motion.clone())?;
        if manual_physics {
            editor.set_physics_manual(document, true)?;
            editor.set_physics_input(document, &parameter, 0.3)?;
        } else {
            editor.toggle_animation()?;
        }
        let metrics = ViewportMetrics {
            width: 256.,
            height: 256.,
            scale_factor: 1.,
        };
        editor.set_canvas_metrics(metrics);
        let primary = editor.primary_viewport_binding().unwrap();
        let secondary = ViewportId::new();
        editor.register_viewport(secondary, ViewportKind::Animation)?;
        editor.set_viewport_metrics(secondary, metrics);
        let secondary = editor.viewport_binding(secondary).unwrap();
        let host = editor.runtime_host();
        let session = editor.document().unwrap().runtime_session_id();
        let client = host.write().register_client()?;
        host.write().attach(client, session)?;
        let before = host.read().subscribe(client, session)?;
        let sliders = editor
            .document()
            .unwrap()
            .parameters()
            .into_iter()
            .map(|p| (p.id, p.value))
            .collect::<Vec<_>>();
        let physics = editor.animation().unwrap().physics_preview.clone();
        let job = host.write().request_samples(
            client,
            QuerySource::Published(before.cursor),
            SampleRequest {
                input: SampleInput::Animation {
                    times: vec![1.5, 0.1, 0.75],
                    motions: vec![(motion.clip_id, 0.)],
                    expressions: vec![],
                    inputs: vec![ParameterActivation {
                        id: parameter,
                        time: 0.,
                        value: 0.9,
                    }],
                    physics: false,
                    stabilized_start: false,
                },
                queries: vec![],
                quality: false,
            },
        )?;
        let mut workers = Manual::default();
        let mut evaluator = AnimationEvaluator::default();
        for time in [0.25, 0.5, 0.75] {
            editor.advance_animation_clock(time);
            let request = editor.animation_request().unwrap();
            let frame = evaluator.evaluate(&request, |_, _| true)?;
            let expected = AnimationEvaluator::default().evaluate(&request, |_, _| true)?;
            assert_eq!(frame.snapshot, expected.snapshot);
            let a = editor
                .animation_viewport_request(primary, frame.identity)
                .unwrap()
                .0;
            let b = editor
                .animation_viewport_request(secondary, frame.identity)
                .unwrap()
                .0;
            host.write().dispatch(&mut workers);
            if let Some(work) = workers.0.pop_front() {
                host.write().complete(work.run()).unwrap();
            }
            assert!(!editor.sync_runtime()?);
            assert_eq!(editor.animation().unwrap().time, time);
            assert!(editor.animation().unwrap().playing);
            assert_eq!(editor.animation().unwrap().physics_preview, physics);
            assert_eq!(
                editor
                    .document()
                    .unwrap()
                    .parameters()
                    .into_iter()
                    .map(|p| (p.id, p.value))
                    .collect::<Vec<_>>(),
                sliders
            );
            assert!(editor.present_animation_viewport(a, frame.snapshot.clone()));
            assert!(editor.present_animation_viewport(b, frame.snapshot.clone()));
            assert_eq!(
                editor
                    .animation()
                    .unwrap()
                    .presented
                    .as_ref()
                    .unwrap()
                    .parameters,
                expected.snapshot.parameters
            );
        }
        while !host.read().job(client, job)?.state.is_terminal() {
            let work = host.write().take_work(job)?;
            host.write().complete(work.run()).unwrap();
        }
        assert_eq!(
            host.read().job(client, job)?.state,
            zipzi_runtime::JobState::Succeeded
        );
        let after = host.read().subscribe(client, session)?;
        assert_eq!(after.cursor, before.cursor);
        assert_eq!(after.history, before.history);
        host.write().release_job(client, job)?;
    }
    Ok(())
}

#[test]
fn r5_external_motion_publication_refreshes_timeline_catalog_and_rejects_old_cpu_gpu_frames()
-> anyhow::Result<()> {
    let (mut editor, _assets) = fixture(false, false)?;
    let document = editor.active_id().unwrap();
    editor.seek_timeline(document, 0.6)?;
    let original = editor.timeline_projection().unwrap();
    let frame = evaluate(&mut editor, &mut AnimationEvaluator::default())?;
    let binding = editor.primary_viewport_binding().unwrap();
    let old_gpu = editor
        .animation_viewport_request(binding, frame.identity)
        .unwrap()
        .0;
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    let agent = host.write().register_client()?;
    crate::support::publish_host(&host, agent, session, "External animation", |e| {
        let mut clip = e.read_document()?.motion(&id(4)).unwrap().clone();
        clip.name = "External clip".into();
        std::sync::Arc::make_mut(&mut clip.tracks[0].segments)[0]
            .end_mut()
            .value = 0.9;
        e.replace_motion(clip)
    })?;
    assert!(editor.sync_runtime()?);
    assert_eq!(
        editor.document().unwrap().motion_resources()[0].name,
        "External clip"
    );
    assert_eq!(
        editor
            .timeline_projection()
            .unwrap()
            .key(&key(11))
            .unwrap()
            .value,
        0.9
    );
    assert_eq!(original.key(&key(11)).unwrap().value, 0.5);
    assert!(!editor.present_animation(frame.identity, frame.snapshot.clone()));
    assert!(!editor.present_animation_viewport(old_gpu, frame.snapshot));
    let updated = evaluate(&mut editor, &mut AnimationEvaluator::default())?;
    assert_eq!(updated.snapshot.parameters[&id(2)], 0.9);
    assert!(editor.present_animation(updated.identity, updated.snapshot));
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    Ok(())
}

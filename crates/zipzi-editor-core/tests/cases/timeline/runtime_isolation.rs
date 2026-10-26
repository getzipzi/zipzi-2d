use super::common::*;
use std::collections::VecDeque;
use zipzi_editor_core::*;
use zipzi_runtime::{Executor, Work, preview::PreviewCommand as C};

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
        let before = host.read().session_info(session)?;
        let sliders = editor
            .document()
            .unwrap()
            .parameters()
            .into_iter()
            .map(|p| (p.id, p.value))
            .collect::<Vec<_>>();
        let physics = editor.animation().unwrap().physics_preview.clone();
        let (remote, _) = host.write().create_preview(client, session, None, None)?;
        host.write().apply_preview(
            client,
            remote,
            C::ScheduleMotion {
                id: motion.clip_id,
                time: 0.,
            },
        )?;
        host.write().apply_preview(
            client,
            remote,
            C::SetParameter {
                id: parameter,
                value: 0.9,
            },
        )?;
        host.write().apply_preview(
            client,
            remote,
            C::SetPhysics {
                enabled: false,
                stabilized_start: false,
            },
        )?;
        let job = host.write().request_preview_samples(
            client,
            remote,
            vec![1.5, 0.1, 0.75],
            true,
            false,
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
            let completion = workers.0.pop_front().unwrap().run_quantum();
            assert!(host.write().complete(completion));
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
        assert!(host.read().job_info(client, job)?.state.is_terminal());
        assert_eq!(host.read().session_info(session)?, before);
        host.write().release_job(client, job)?;
        host.write().release_preview(client, remote)?;
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
    let edit = host
        .write()
        .begin_edit(agent, session, "external animation", None)?;
    host.write().apply_edit(agent, edit, |e| {
        let mut clip = e.candidate_document().get_motion(&id(4)).unwrap().clone();
        clip.name = "External clip".into();
        std::sync::Arc::make_mut(&mut clip.tracks[0].segments)[0]
            .end_mut()
            .value = 0.9;
        e.replace_motion(clip)
    })?;
    host.write().commit_edit(agent, edit)?;
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

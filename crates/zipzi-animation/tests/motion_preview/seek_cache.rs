use super::common::*;
use zipzi_animation::MotionPreview;
use zipzi_core::{
    evaluate_frame, BindingAxis, DrawableFrame, ImageAsset, Mesh, MeshBinding, MeshKeyform, Vec2,
};

#[test]
fn cached_seek_matches_cold_replay_for_complete_runtime_state() {
    let mut cached = combined_cache_preview();
    let mut cold = combined_cache_preview();
    cold.set_seek_cache_budget(0);
    // Warm checkpoints, then cover exact hits, fractional tails, backward and zero seeks.
    for time in [3.37, 1.0, 1.01, 2.0, 2.7, 0.0, 0.001, 4.1, 1.0, 60.0, 61.0] {
        assert_eq!(
            cached.seek(time).unwrap(),
            cold.seek(time).unwrap(),
            "time={time}"
        );
        assert!(
            cached.seek_cache_stats().estimated_bytes <= cached.seek_cache_stats().budget_bytes
        );
    }
    assert_eq!(cached.seek_cache_stats().last_restored_time, 60.0);
    assert_eq!(cached.seek_cache_stats().last_replayed_steps, 60);
    assert_eq!(cold.seek_cache_stats().last_replayed_steps, 3660);
    assert_eq!(cold.seek_cache_stats().checkpoints, 0);
    // Noncanonical mutations must never seed checkpoints.
    cached.advance(0.031).unwrap();
    cached.stabilize_physics();
    assert_eq!(cached.seek(62.0).unwrap(), cold.seek(62.0).unwrap());
    cached.seek(1.0).unwrap();
    cold.seek(1.0).unwrap();
    for preview in [&mut cached, &mut cold] {
        preview.schedule_parameter_input(PARAM, 1.0, 0.3).unwrap();
        assert_eq!(preview.seek_cache_stats().checkpoints, 0);
    }
    assert_eq!(cached.seek(2.0).unwrap(), cold.seek(2.0).unwrap());
    cached.set_base_parameter(PARAM, 0.4).unwrap();
    cold.set_base_parameter(PARAM, 0.4).unwrap();
    assert_eq!(cached.seek_cache_stats().checkpoints, 0);
    assert_eq!(cached.seek(2.0).unwrap(), cold.seek(2.0).unwrap());
}

#[test]
fn cache_eviction_cancellation_and_failed_schedule_are_atomic() {
    let mut preview = combined_cache_preview();
    preview.seek(1.0).unwrap();
    let one = preview.seek_cache_stats().estimated_bytes;
    preview.set_seek_cache_budget(one * 2);
    preview.seek(20.0).unwrap();
    assert!(preview.seek_cache_stats().checkpoints < 20);
    assert!(preview.seek_cache_stats().estimated_bytes <= one * 2);
    let state = preview.snapshot().clone();
    let stats = preview.seek_cache_stats();
    assert!(preview.schedule_motion_entry("missing", 0, 21.0).is_err());
    assert!(preview.schedule_motion("missing", 21.0).is_err());
    assert!(preview.schedule_expression("missing", 21.0).is_err());
    assert!(preview.schedule_parameter_input(PARAM, 0.0, 1.0).is_err());
    assert_eq!(preview.seek_cache_stats(), stats);
    for target in [20.0, 22.0, 0.9] {
        assert_eq!(
            preview
                .seek_with_progress(target, |done, total| total != 0
                    && done < if target == 22.0 { 65 } else { 3 })
                .unwrap_err(),
            zipzi_animation::AnimationError::SeekCancelled
        );
        assert_eq!(preview.snapshot(), &state);
        assert_eq!(preview.seek_cache_stats(), stats);
    }
    let mut calls = Vec::new();
    preview
        .seek_with_progress(20.0, |done, total| {
            calls.push((done, total));
            true
        })
        .unwrap();
    assert_eq!(calls, [(0, 0)]);
    preview.set_seek_cache_budget(1);
    preview.seek(3.0).unwrap();
    assert_eq!(preview.seek_cache_stats().estimated_bytes, 0);
    assert_eq!(preview.seek_cache_stats().checkpoints, 0);
    preview.reset();
    assert_eq!(preview.seek_cache_stats().budget_bytes, 1);
}

#[test]
fn geometry_workspace_preserves_owned_frames_and_shared_reads() {
    let mut doc = document();
    let asset = "00000000-0000-4000-8000-000000000e01";
    let mesh = "00000000-0000-4000-8000-000000000e02";
    let binding = "00000000-0000-4000-8000-000000000e03";
    assert!(doc
        .add_asset(ImageAsset {
            unresolved_source: None,
            id: asset.into(),
            name: String::new(),
            content: Some(zipzi_core::ImageContent {
                source: "memory://geometry".into(),
                width: 8,
                height: 8,
                ..Default::default()
            }),
        })
        .status
        .is_ok());
    let positions = vec![Vec2::new(0., 0.), Vec2::new(10., 0.), Vec2::new(0., 10.)];
    assert!(doc
        .create_mesh(Mesh {
            id: mesh.into(),
            texture_asset_id: asset.into(),
            vertex_ids: vec![1, 2, 3],
            base_positions: positions.clone(),
            uvs: vec![Vec2::default(); 3],
            triangles: vec![[1, 2, 3]],
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .create_binding(MeshBinding {
            id: binding.into(),
            mesh_id: mesh.into(),
            axes: vec![BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: PARAM.into(),
                keys: vec![0., 1.]
            }],
            keyforms: [0., 1.]
                .into_iter()
                .map(|key| MeshKeyform {
                    keys: vec![key],
                    positions: positions
                        .iter()
                        .map(|p| Vec2::new(p.x + key * 20., p.y))
                        .collect(),
                    ..Default::default()
                })
                .collect(),
        })
        .status
        .is_ok());
    let mut preview = MotionPreview::new(&doc);
    let saved = preview.evaluate_drawables().unwrap();
    for value in [0.25, 1., 0., 0.75] {
        preview.set_base_parameter(PARAM, value).unwrap();
        let mut reference = DrawableFrame::default();
        assert!(evaluate_frame(&doc, &[(PARAM.into(), value)].into(), &mut reference).is_ok());
        assert_eq!(preview.evaluate_drawables().unwrap(), reference);
        let (timed, _) = preview.evaluate_drawables_timed().unwrap();
        assert_eq!(timed, reference);
        assert_eq!(
            preview.evaluate_drawables_including_hidden().unwrap(),
            reference
        );
        // Mutating a caller-owned result must never contaminate later results.
        let mut changed = preview.evaluate_drawables().unwrap();
        changed.drawables[0].positions.fill(Vec2::new(999., 999.));
        changed.parameters.clear();
        assert_eq!(preview.evaluate_drawables().unwrap(), reference);
    }
    assert_eq!(saved.parameters[0].value, 0.);
    assert_ne!(
        saved.drawables[0].positions,
        preview.evaluate_drawables().unwrap().drawables[0].positions
    );
    let expected = preview.evaluate_drawables().unwrap();
    let snapshot = preview.snapshot().clone();
    let operation = preview.operation().clone();
    // Also compile-checks that MotionPreview remains Send + Sync.
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let (preview, expected) = (&preview, &expected);
            scope.spawn(move || {
                for _ in 0..8 {
                    assert_eq!(&preview.evaluate_drawables().unwrap(), expected);
                }
            });
        }
    });
    assert_eq!(*preview.snapshot(), snapshot);
    assert_eq!(*preview.operation(), operation);
}

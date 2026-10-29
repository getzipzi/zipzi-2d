use zipzi_core::{Canvas, Parameter, PreviewValues, Vec2};
use zipzi_observe::{
    ModelRoi, ObservationInput, Observer, ObserverConfig, RenderRequest, ResolvedObservation,
};
use zipzi_sdk::{
    prepare_png_asset, rectangle_mesh, AuthoringSession, AuthoringSnapshot, ConversionControl,
};

const DOCUMENT: &str = "00000000-0000-4000-8000-000000000001";
const ASSET: &str = "00000000-0000-4000-8000-000000000002";
const MESH: &str = "00000000-0000-4000-8000-000000000003";
const PARAMETER: &str = "00000000-0000-4000-8000-000000000004";

fn fixture(path: &std::path::Path) -> AuthoringSession {
    let mut session =
        AuthoringSession::new(DOCUMENT, Canvas::new(100., 100., Vec2::new(50., 50.), 10.)).unwrap();
    let asset = prepare_png_asset(ASSET, "texture", path).unwrap();
    session
        .edit("fixture", None, |edit| {
            edit.create_asset(asset)?;
            edit.create_mesh(
                rectangle_mesh(
                    MESH,
                    "face",
                    ASSET,
                    Vec2::new(40., 40.),
                    Vec2::new(60., 60.),
                )
                .unwrap(),
            )?;
            edit.create_parameter(Parameter {
                id: PARAMETER.into(),
                name: "Shift".into(),
                ..Default::default()
            })
        })
        .unwrap();
    session
}

fn texture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png")
}

fn capture(snapshot: &AuthoringSnapshot) -> ResolvedObservation {
    resolve(ObservationInput::capture_from_snapshot(snapshot, &PreviewValues::new()).unwrap())
}

fn resolve(input: ObservationInput) -> ResolvedObservation {
    ResolvedObservation::capture_many_with_control(vec![input], &ConversionControl::default())
        .unwrap()
        .remove(0)
}

fn view() -> RenderRequest {
    RenderRequest {
        width: 64,
        height: 64,
        roi: ModelRoi {
            x0: 30.,
            y0: 30.,
            x1: 80.,
            y1: 80.,
        },
        padding_model: 0.,
        topology: None,
        quality: false,
    }
}

#[test]
fn candidate_animation_uses_snapshot_identity_not_formal_or_branch_revision_counters() {
    let session =
        AuthoringSession::new(DOCUMENT, Canvas::new(100., 100., Vec2::new(50., 50.), 10.)).unwrap();
    let mut left = session.begin_owned_edit("left", None).unwrap();
    left.create_part(zipzi_core::Part {
        id: ASSET.into(),
        name: "Left".into(),
        ..Default::default()
    })
    .unwrap();
    let a = session.candidate_snapshot(&left).unwrap();
    assert_ne!(a.version().revision, a.document().revision());
    let mut preview = zipzi_sdk::MotionPreview::new(a.document());
    preview.bind_session_identity(a.version().session_id, a.version().generation);
    ObservationInput::capture_motion_from_snapshot(&a, &preview).unwrap();
    let mut right = session.begin_owned_edit("right", None).unwrap();
    right
        .create_part(zipzi_core::Part {
            id: ASSET.into(),
            name: "Right".into(),
            ..Default::default()
        })
        .unwrap();
    let b = session.candidate_snapshot(&right).unwrap();
    assert_eq!(a.version(), b.version());
    assert_eq!(a.document().revision(), b.document().revision());
    assert_eq!(
        ObservationInput::capture_motion_from_snapshot(&b, &preview)
            .unwrap_err()
            .code,
        "STALE_ANIMATION_PREVIEW"
    );
    left.create_part(zipzi_core::Part {
        id: MESH.into(),
        name: "Later".into(),
        ..Default::default()
    })
    .unwrap();
    let later = session.candidate_snapshot(&left).unwrap();
    assert_eq!(
        ObservationInput::capture_motion_from_snapshot(&later, &preview)
            .unwrap_err()
            .code,
        "STALE_ANIMATION_PREVIEW"
    );
    drop(left);
    drop(right);
    ObservationInput::capture_motion_from_snapshot(&a, &preview).unwrap();
}

#[test]
fn cancelled_capture_releases_inputs_without_resolving_resources_or_gpu() {
    let session = fixture(&texture_path());
    let input =
        ObservationInput::capture_from_snapshot(&session.read_snapshot(), &PreviewValues::new())
            .unwrap();
    let control = ConversionControl::default();
    control.cancel();
    assert_eq!(
        ResolvedObservation::capture_many_with_control(vec![input], &control)
            .unwrap_err()
            .code,
        "CANCELLED"
    );
    assert!(!capture(&session.read_snapshot()).scene_digest().is_empty());
}

#[test]
fn batch_samples_one_document_snapshot_and_reuses_capture_identity() {
    let mut session = fixture(&texture_path());
    let snapshot = session.read_snapshot();
    let requested = [
        PreviewValues::from([(PARAMETER.into(), 0.)]),
        PreviewValues::from([(PARAMETER.into(), 0.5)]),
    ];
    session
        .edit("move after snapshot", None, |edit| {
            edit.update_positions(MESH, &[0], &[Vec2::new(50., 40.)])
        })
        .unwrap();
    let inputs = requested
        .iter()
        .map(|values| ObservationInput::capture_from_snapshot(&snapshot, values).unwrap())
        .collect();
    let scenes =
        ResolvedObservation::capture_many_with_control(inputs, &ConversionControl::default())
            .unwrap();
    assert_eq!(scenes[0].capture_id(), scenes[1].capture_id());
    assert_ne!(scenes[0].scene_digest(), scenes[1].scene_digest());
    let repeated =
        resolve(ObservationInput::capture_from_snapshot(&snapshot, &requested[0]).unwrap());
    let changed = resolve(
        ObservationInput::capture_from_snapshot(&session.read_snapshot(), &requested[0]).unwrap(),
    );
    assert_eq!(scenes[0].scene_digest(), repeated.scene_digest());
    assert_ne!(scenes[0].scene_digest(), changed.scene_digest());
    let mixed = vec![
        ObservationInput::capture_from_snapshot(&snapshot, &requested[0]).unwrap(),
        ObservationInput::capture_from_snapshot(&session.read_snapshot(), &requested[0]).unwrap(),
    ];
    assert_eq!(
        ResolvedObservation::capture_many_with_control(mixed, &ConversionControl::default())
            .unwrap_err()
            .code,
        "MIXED_DOCUMENT_SNAPSHOTS"
    );
}

#[test]
fn capture_rejects_empty_and_oversized_batches() {
    let session = fixture(&texture_path());
    let input =
        ObservationInput::capture_from_snapshot(&session.read_snapshot(), &PreviewValues::new())
            .unwrap();
    for inputs in [vec![], vec![input; 65]] {
        assert_eq!(
            ResolvedObservation::capture_many_with_control(inputs, &ConversionControl::default())
                .unwrap_err()
                .code,
            "OBSERVATION_BUDGET_EXCEEDED"
        );
    }
}

#[test]
fn captures_animation_frame_and_rejects_stale_preview() {
    let mut session = fixture(&texture_path());
    let snapshot = session.read_snapshot();
    let mut preview = session.motion_preview();
    let scene =
        resolve(ObservationInput::capture_motion_from_snapshot(&snapshot, &preview).unwrap());
    let second_preview = session.motion_preview();
    let second = resolve(
        ObservationInput::capture_motion_from_snapshot(&snapshot, &second_preview).unwrap(),
    );
    assert_ne!(scene.capture_id(), second.capture_id());
    assert_eq!(scene.scene_digest(), second.scene_digest());
    preview.reset();
    let reset =
        resolve(ObservationInput::capture_motion_from_snapshot(&snapshot, &preview).unwrap());
    assert_eq!(scene.scene_digest(), reset.scene_digest());
    session
        .edit("move", None, |edit| {
            edit.update_positions(MESH, &[0], &[Vec2::new(50., 40.)])
        })
        .unwrap();
    assert_eq!(
        ObservationInput::capture_motion_from_snapshot(&session.read_snapshot(), &preview)
            .unwrap_err()
            .code,
        "STALE_ANIMATION_PREVIEW"
    );
    ObservationInput::capture_motion_from_snapshot(&snapshot, &preview).unwrap();
}

#[test]
fn renders_unsaved_snapshot_and_reuses_gpu_across_changes() {
    let mut session = fixture(&texture_path());
    let mut observer = Observer::new(ObserverConfig {
        width: 64,
        height: 64,
    })
    .unwrap();
    let first = observer
        .render(&capture(&session.read_snapshot()), view())
        .unwrap();
    #[cfg(all(target_os = "macos", not(feature = "wgpu-backend")))]
    assert_eq!(first.adapter_backend, "Metal");
    #[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
    assert!(first.adapter_backend.starts_with("wgpu ("));
    assert_eq!(first.rgba.len(), 64 * 64 * 4);
    assert!(first.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
    assert_eq!(
        first.png_bytes().unwrap()[..8],
        [137, 80, 78, 71, 13, 10, 26, 10]
    );
    session
        .edit("move", None, |edit| {
            edit.update_positions(
                MESH,
                &[0, 1, 2, 3],
                &[
                    Vec2::new(50., 40.),
                    Vec2::new(70., 40.),
                    Vec2::new(70., 60.),
                    Vec2::new(50., 60.),
                ],
            )
        })
        .unwrap();
    let second = observer
        .render(&capture(&session.read_snapshot()), view())
        .unwrap();
    assert_ne!(first.rgba, second.rgba);
    assert_ne!(first.input_sha256, second.input_sha256);
    assert_eq!(first.textures, second.textures);
    assert_eq!(first.view, second.view);
    let replacement = prepare_png_asset(
        ASSET,
        "texture",
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/texture_00.png"),
    )
    .unwrap();
    session
        .edit("texture", None, |edit| edit.replace_asset(replacement))
        .unwrap();
    let scene = capture(&session.read_snapshot());
    let third = observer.render(&scene, view()).unwrap();
    assert_ne!(third.textures[0].sha256, first.textures[0].sha256);
    let fourth = observer
        .render(
            &scene,
            RenderRequest {
                width: 32,
                height: 48,
                ..view()
            },
        )
        .unwrap();
    assert_eq!(
        (fourth.width, fourth.height, fourth.rgba.len()),
        (32, 48, 32 * 48 * 4)
    );
    assert_eq!(third.textures, fourth.textures);
    let repeated = observer.render(&scene, view()).unwrap();
    assert_eq!(third.rgba, repeated.rgba);
    assert_eq!(third.input_sha256, repeated.input_sha256);
}

#[test]
fn resolved_capture_renders_after_source_disappears() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.png");
    std::fs::copy(texture_path(), &path).unwrap();
    let session = fixture(&path);
    let scene = capture(&session.read_snapshot());
    let duplicate = capture(&session.read_snapshot());
    assert_ne!(scene.capture_id(), duplicate.capture_id());
    assert_eq!(scene.scene_digest(), duplicate.scene_digest());
    std::fs::remove_file(path).unwrap();
    drop(session);
    let mut observer = Observer::new(ObserverConfig {
        width: 64,
        height: 64,
    })
    .unwrap();
    let frame = observer.render(&scene, view()).unwrap();
    assert!(frame.drawable_bounds[0].bounds.is_some());
    assert_eq!(frame.view, view().mapping().unwrap());
    assert_eq!(observer.render(&scene, view()).unwrap().rgba, frame.rgba);
    assert_eq!(
        scene.render_digest(view()).unwrap(),
        duplicate.render_digest(view()).unwrap()
    );
    assert_ne!(
        scene.render_digest(view()).unwrap(),
        scene
            .render_digest(RenderRequest {
                width: 65,
                ..view()
            })
            .unwrap()
    );
    assert_eq!(
        observer
            .render(
                &scene,
                RenderRequest {
                    width: 4097,
                    ..view()
                }
            )
            .unwrap_err()
            .code,
        "OUTPUT_SIZE_LIMIT"
    );
}

#[test]
fn in_memory_image_content_survives_session_drop_in_a_readonly_capture() {
    let mut session = fixture(&texture_path());
    let image = zipzi_sdk::prepare_image_load(&texture_path()).unwrap();
    let token = session.image_load_token(ASSET).unwrap();
    session
        .replace_prepared_image_content(&token, &image)
        .unwrap();
    let snapshot = session.read_snapshot();
    let input = ObservationInput::capture_from_snapshot(&snapshot, &PreviewValues::new()).unwrap();
    drop(snapshot);
    drop(session);
    drop(image);
    let scene = resolve(input);
    let mut observer = Observer::new(ObserverConfig {
        width: 64,
        height: 64,
    })
    .unwrap();
    let frame = observer.render(&scene, view()).unwrap();
    assert_eq!(frame.textures.len(), 1);
    assert!(frame.rgba.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
}

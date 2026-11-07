use zipzi_core::{Canvas, Parameter, PreviewValues, Vec2};
use zipzi_observe::{
    ModelRoi, ObservationInput, Observer, ObserverConfig, RenderRequest, ResolvedObservation,
};
use zipzi_sdk::{
    rectangle_mesh, AuthoringSession, Commands, ConversionControl, ResourceStore, ResourceView,
    Snapshot, SourcedSnapshot, Status,
};

const DOCUMENT: &str = "00000000-0000-4000-8000-000000000001";
const ASSET: &str = "00000000-0000-4000-8000-000000000002";
const MESH: &str = "00000000-0000-4000-8000-000000000003";
const PARAMETER: &str = "00000000-0000-4000-8000-000000000004";

fn edit(
    session: &mut AuthoringSession,
    description: &str,
    run: impl FnOnce(&mut Commands<'_>) -> Result<(), Status>,
) {
    let mut draft = session.begin_draft();
    draft.batch(draft.source(), run).unwrap();
    session
        .publish(draft.prepare(description).unwrap())
        .unwrap();
}
fn motion_preview(session: &AuthoringSession) -> zipzi_animation::MotionPreview {
    let input = session.read_snapshot();
    let mut preview = zipzi_animation::MotionPreview::new(input.snapshot.document());
    preview.bind_source(input.source);
    preview
}
fn fixture(path: &std::path::Path) -> AuthoringSession {
    let state = zipzi_core::state::DocumentState::new(
        DOCUMENT,
        Canvas::new(100., 100., Vec2::new(50., 50.), 10.).into(),
    )
    .unwrap();
    let mut session = AuthoringSession::new(Snapshot::new(state, ResourceView::default()).unwrap());
    let image = ResourceStore::default().import_image(path).unwrap();
    edit(&mut session, "fixture", |edit| {
        edit.create_empty_asset(ASSET.into(), "texture".into())?;
        edit.replace_image(ASSET, image)?;
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
            runtime_id: "Shift".into(),
            name: "Shift".into(),
            ..Default::default()
        })
    });
    session
}

fn texture_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png")
}

fn capture(snapshot: &SourcedSnapshot) -> ResolvedObservation {
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
fn fit_budget_gpu_cache_uses_actual_render_content() {
    use zipzi_observe::{planning::*, ObservationCaptureLimits};
    let session = fixture(&texture_path());
    let snapshot = session.read_snapshot();
    let capture = |budget: Option<u64>| {
        let policy = budget.map_or_else(Default::default, |max_rgba_bytes| {
            ObservationTexturePolicy::FitBudget { max_rgba_bytes }
        });
        let ObservationTexturePlanning::Ready { plan } = plan_textures(
            texture_sources(snapshot.snapshot.document()).unwrap(),
            policy,
            DEFAULT_OBSERVATION_TEXTURE_RGBA_BYTES,
        )
        .unwrap() else {
            panic!("fixture must fit")
        };
        let input =
            ObservationInput::capture_from_snapshot(&snapshot, &Default::default()).unwrap();
        ResolvedObservation::capture_many_with_plan(
            vec![input],
            &plan,
            ObservationCaptureLimits::default(),
            false,
            &Default::default(),
        )
        .unwrap()
        .remove(0)
    };
    let original = capture(None);
    let fit = capture(Some(4));
    let equivalent_fit = capture(Some(5));
    let equivalent_original = capture(Some(16));
    let mut observer = Observer::new(ObserverConfig {
        width: 64,
        height: 64,
    })
    .unwrap();
    let a = observer.render(&original, view()).unwrap();
    let b = observer.render(&fit, view()).unwrap();
    let c = observer.render(&equivalent_fit, view()).unwrap();
    let d = observer.render(&equivalent_original, view()).unwrap();
    assert_ne!(a.input_sha256, b.input_sha256);
    assert_eq!(b.input_sha256, c.input_sha256);
    assert_eq!(b.rgba, c.rgba);
    assert_eq!(a.input_sha256, d.input_sha256);
    assert_eq!(a.rgba, d.rgba);
}

#[test]
fn candidate_animation_checks_draft_identity_sequence_and_content() {
    let session = fixture(&texture_path());
    let mut left = session.begin_draft();
    let a = left.read_snapshot().unwrap();
    let mut preview = zipzi_animation::MotionPreview::new(a.snapshot.document());
    preview.bind_source(a.source);
    ObservationInput::capture_motion_from_snapshot(&a, &preview).unwrap();
    let right = session.begin_draft();
    let b = right.read_snapshot().unwrap();
    // Same ContentId and base Version still have different candidate identities.
    assert_eq!(a.snapshot.content_id(), b.snapshot.content_id());
    assert_ne!(a.source, b.source);
    assert_eq!(
        ObservationInput::capture_motion_from_snapshot(&b, &preview)
            .unwrap_err()
            .code,
        "STALE_ANIMATION_PREVIEW"
    );
    left.batch(left.source(), |_| Ok(())).unwrap();
    let later = left.read_snapshot().unwrap();
    assert_eq!(a.snapshot.content_id(), later.snapshot.content_id());
    assert_ne!(a.source, later.source);
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
    edit(&mut session, "move after snapshot", |edit| {
        edit.set_vertex_positions(MESH, &[(0, Vec2::new(50., 40.))])
    });
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
    let mut preview = motion_preview(&session);
    let scene =
        resolve(ObservationInput::capture_motion_from_snapshot(&snapshot, &preview).unwrap());
    let second_preview = motion_preview(&session);
    let second = resolve(
        ObservationInput::capture_motion_from_snapshot(&snapshot, &second_preview).unwrap(),
    );
    assert_ne!(scene.capture_id(), second.capture_id());
    assert_eq!(scene.scene_digest(), second.scene_digest());
    preview.reset();
    let reset =
        resolve(ObservationInput::capture_motion_from_snapshot(&snapshot, &preview).unwrap());
    assert_eq!(scene.scene_digest(), reset.scene_digest());
    edit(&mut session, "move", |edit| {
        edit.set_vertex_positions(MESH, &[(0, Vec2::new(50., 40.))])
    });
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
    edit(&mut session, "move", |edit| {
        edit.set_vertex_positions(
            MESH,
            &[
                (0, Vec2::new(50., 40.)),
                (1, Vec2::new(70., 40.)),
                (2, Vec2::new(70., 60.)),
                (3, Vec2::new(50., 60.)),
            ],
        )
    });
    let second = observer
        .render(&capture(&session.read_snapshot()), view())
        .unwrap();
    assert_ne!(first.rgba, second.rgba);
    assert_ne!(first.input_sha256, second.input_sha256);
    assert_eq!(first.textures, second.textures);
    assert_eq!(first.view, second.view);
    let replacement = ResourceStore::default()
        .import_image(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/texture_00.png"),
        )
        .unwrap();
    edit(&mut session, "texture", |edit| {
        edit.replace_image(ASSET, replacement)
    });
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
                    // Host policy is configurable in Runtime. The low-level
                    // observer rejects the actual device bound before allocation.
                    width: u32::MAX,
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
    let image = ResourceStore::default()
        .import_image(&texture_path())
        .unwrap();
    edit(&mut session, "image", |edit| {
        edit.replace_image(ASSET, image.clone())
    });
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

#[test]
fn archive_observation_keeps_pixels_after_session_and_all_files_are_removed() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("source.png");
    std::fs::copy(texture_path(), &source).unwrap();
    let mut session = fixture(&source);
    let io =
        zipzi_project::ProjectIo::new(directory.path().join("cache"), ResourceStore::default());
    let project = directory.path().join("角色 observation.zipzi");
    session.save_as(&io, &project, None, || true).unwrap();
    let (session, _) = AuthoringSession::open(&io, &project, || true).unwrap();
    let input =
        ObservationInput::capture_from_snapshot(&session.read_snapshot(), &PreviewValues::new())
            .unwrap();
    drop(session);
    drop(io);
    drop(directory);
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

#[test]
fn static_project_preview_has_pixels_preserves_pose_and_observes_cancellation() {
    let session = fixture(&texture_path());
    let mut preview = zipzi_core::preview::PreviewState::default();
    preview
        .replace(
            session.snapshot().document(),
            session.version().generation,
            [(PARAMETER.into(), 0.75)].into(),
        )
        .unwrap();
    let snapshot = session.read_snapshot();
    let version = session.version();
    let image = zipzi_observe::render_project_preview(&snapshot, &ConversionControl::default())
        .unwrap()
        .unwrap();
    assert_eq!(
        (image.descriptor().width, image.descriptor().height),
        (1024, 1024)
    );
    let mut png = png::Decoder::new(std::io::Cursor::new(image.png()))
        .read_info()
        .unwrap();
    let mut pixels = vec![0; png.output_buffer_size()];
    png.next_frame(&mut pixels).unwrap();
    assert!(pixels.chunks_exact(4).any(|p| p[3] > 0));
    assert_eq!(session.version(), version);
    assert_eq!(preview.values().get(PARAMETER), Some(&0.75));
    let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    assert_eq!(
        zipzi_observe::render_project_preview(
            &snapshot,
            &ConversionControl::from_cancel_flag(flag)
        )
        .unwrap_err()
        .code,
        "CANCELLED"
    );
}

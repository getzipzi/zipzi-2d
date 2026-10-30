use super::*;
use zipzi_core::{Canvas, Vec2};
use zipzi_sdk::{AuthoringSession, ConversionControl};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}

fn fixture() -> (AuthoringSession, zipzi_project::PreparedImageContent) {
    let pixels: Arc<[u8]> = (0..35)
        .flat_map(|i| [255, (i * 3) as u8, 19, if i % 3 == 0 { 0 } else { 255 }])
        .collect::<Vec<_>>()
        .into();
    let image = zipzi_project::PreparedImageContent::from_rgba(
        7,
        5,
        pixels,
        &zipzi_project::ImageMemoryBudget::default(),
    )
    .unwrap();
    let mut session =
        AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::new(50., 50.), 10.)).unwrap();
    session
        .edit("fixture", None, |edit| {
            edit.create_prepared_image(id(2), "texture".into(), &image)?;
            edit.create_mesh(
                zipzi_sdk::rectangle_mesh(
                    &id(3),
                    "mesh",
                    &id(2),
                    Vec2::new(40., 40.),
                    Vec2::new(60., 60.),
                )
                .unwrap(),
            )
        })
        .unwrap();
    (session, image)
}

fn plan(snapshot: &AuthoringSnapshot, budget: Option<u64>) -> planning::ObservationTexturePlan {
    match planning::plan_textures(
        planning::texture_sources(snapshot.document()).unwrap(),
        budget.map_or_else(Default::default, |max_rgba_bytes| {
            planning::ObservationTexturePolicy::FitBudget { max_rgba_bytes }
        }),
        planning::DEFAULT_OBSERVATION_TEXTURE_RGBA_BYTES,
    )
    .unwrap()
    {
        planning::ObservationTexturePlanning::Ready { plan } => plan,
        _ => panic!("fixture must fit"),
    }
}

fn resolve(
    snapshot: &AuthoringSnapshot,
    plan: &planning::ObservationTexturePlan,
    alpha: bool,
) -> ResolvedObservation {
    let input =
        ObservationInput::capture_from_snapshot_with_quality(snapshot, &Default::default(), true)
            .unwrap();
    ResolvedObservation::capture_many_with_plan(
        vec![input],
        plan,
        ObservationCaptureLimits::default(),
        alpha,
        &ConversionControl::default(),
    )
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
            x1: 70.,
            y1: 70.,
        },
        padding_model: 0.,
        topology: None,
        quality: true,
    }
}

#[test]
fn original_reuses_source_arc_and_a_batch_shares_all_prepared_textures_and_report() {
    let (session, image) = fixture();
    let snapshot = session.read_snapshot();
    let inputs = (0..2)
        .map(|_| {
            ObservationInput::capture_from_snapshot_with_quality(
                &snapshot,
                &Default::default(),
                true,
            )
            .unwrap()
        })
        .collect();
    let captures = ResolvedObservation::capture_many_with_plan(
        inputs,
        &plan(&snapshot, None),
        ObservationCaptureLimits::default(),
        true,
        &ConversionControl::default(),
    )
    .unwrap();
    assert!(Arc::ptr_eq(&captures[0].textures, &captures[1].textures));
    assert!(Arc::ptr_eq(
        &captures[0].texture_report,
        &captures[1].texture_report
    ));
    assert!(Arc::ptr_eq(
        &captures[0].textures[0].data.rgba,
        &image.texture().rgba
    ));
    assert_eq!(captures[0].texture_report().resized_count, 0);
}

#[test]
fn fit_budget_preserves_source_digest_and_alpha_coverage_but_changes_render_digest() {
    let (session, _) = fixture();
    let snapshot = session.read_snapshot();
    let original = resolve(&snapshot, &plan(&snapshot, None), true);
    let fit = resolve(&snapshot, &plan(&snapshot, Some(4)), true);
    assert_eq!(original.scene_digest(), fit.scene_digest());
    assert_eq!(
        serde_json::to_value(original.quality(&view()).unwrap()).unwrap(),
        serde_json::to_value(fit.quality(&view()).unwrap()).unwrap()
    );
    assert_ne!(
        original.render_digest(view()).unwrap(),
        fit.render_digest(view()).unwrap()
    );
    let report = fit.texture_report();
    assert_eq!(
        (
            report.source_rgba_bytes,
            report.render_rgba_bytes,
            report.resized_count
        ),
        (140, 4, 1)
    );
    assert_eq!(
        report.textures[0].render_size,
        ObservationTextureSize {
            width: 1,
            height: 1
        }
    );
    assert_eq!(
        report.textures[0].source_content_identity,
        original.texture_report().textures[0].source_content_identity
    );
    assert_ne!(
        report.textures[0].render_rgba_hash,
        original.texture_report().textures[0].render_rgba_hash
    );
    assert_eq!(
        AREA_FILTER_VERSION,
        zipzi_render::texture::STRAIGHT_RGBA_AREA_FILTER_VERSION
    );
}

#[test]
fn equivalent_actual_inputs_have_equal_render_digests_across_policies() {
    let (session, _) = fixture();
    let snapshot = session.read_snapshot();
    let original = resolve(&snapshot, &plan(&snapshot, None), true);
    let fit_original = resolve(&snapshot, &plan(&snapshot, Some(140)), true);
    assert_eq!(
        original.render_digest(view()).unwrap(),
        fit_original.render_digest(view()).unwrap()
    );
    let fit_four = resolve(&snapshot, &plan(&snapshot, Some(4)), true);
    let fit_five = resolve(&snapshot, &plan(&snapshot, Some(5)), true);
    assert_eq!(
        fit_four.render_digest(view()).unwrap(),
        fit_five.render_digest(view()).unwrap()
    );
}

#[test]
fn geometry_quality_does_not_require_or_retain_source_alpha_samples() {
    let (session, _) = fixture();
    let snapshot = session.read_snapshot();
    let capture = resolve(&snapshot, &plan(&snapshot, Some(4)), false);
    assert!(capture.textures[0].source_alpha.is_none());
    let request = RenderRequest {
        quality: false,
        topology: Some(zipzi_sdk::TopologyOverlay {
            strain: true,
            ..Default::default()
        }),
        ..view()
    };
    assert!(!capture.quality(&request).unwrap().is_empty());
}

#[test]
fn mismatched_plan_and_workspace_fail_before_source_acquisition() {
    let (session, _) = fixture();
    let snapshot = session.read_snapshot();
    let mut input =
        ObservationInput::capture_from_snapshot(&snapshot, &Default::default()).unwrap();
    input.memory_images.clear(); // Would cause missing-file failure if acquisition ran.
    let mut wrong = plan(&snapshot, None);
    wrong.textures[0].render_size.width += 1;
    assert_eq!(
        ResolvedObservation::capture_many_with_plan(
            vec![input.clone()],
            &wrong,
            ObservationCaptureLimits::default(),
            false,
            &Default::default()
        )
        .unwrap_err()
        .code,
        "TEXTURE_PLAN_MISMATCH"
    );
    let mut limits = ObservationCaptureLimits::default();
    limits.png.max_workspace_bytes = 1;
    assert_eq!(
        ResolvedObservation::capture_many_with_plan(
            vec![input],
            &plan(&snapshot, None),
            limits,
            false,
            &Default::default()
        )
        .unwrap_err()
        .code,
        "DECODE_WORKSPACE_LIMIT"
    );
}

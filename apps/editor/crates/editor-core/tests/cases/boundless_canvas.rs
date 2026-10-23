use super::support;
use editor_core::*;
use kasane_core::{ModelSetup, Vec2};
use kasane_sdk::{AuthoringSession, prepare_png_asset, rectangle_mesh};

fn fixture() -> anyhow::Result<(Editor, tempfile::TempDir)> {
    fixture_with_source(None)
}

fn fixture_with_source(
    source_canvas: Option<kasane_core::Canvas>,
) -> anyhow::Result<(Editor, tempfile::TempDir)> {
    let dir = tempfile::tempdir()?;
    let texture = dir.path().join("texture.png");
    std::fs::write(&texture, include_bytes!("../fixtures/accent.png"))?;
    let id = |n| uuid::Uuid::from_u128(n).to_string();
    // Source coordinates must remain independent of the origin and SDK units.
    let canvas = ModelSetup::from(kasane_core::Canvas::new(
        100.,
        100.,
        Vec2::new(73., -42.),
        1.,
    ));
    let mut session = AuthoringSession::new(
        &id(1),
        kasane_core::ModelSetup {
            source_canvas,
            ..canvas
        },
    )?;
    session.edit("Artwork", None, |edit| {
        edit.create_asset(prepare_png_asset(&id(2), "Artwork", &texture)?)?;
        edit.create_mesh(rectangle_mesh(
            &id(3),
            "Offset artwork",
            &id(2),
            Vec2::new(1000., -600.),
            Vec2::new(1200., -500.),
        )?)
    })?;
    let saved = session.save_project(&dir.path().join("boundless.kasane"), None)?;
    let document = Document::open(&saved.manifest)?;
    assert_eq!(document.source_canvas(), source_canvas);
    Ok((support::editor_from_document(document), dir))
}

fn metrics(scale_factor: f32) -> ViewportMetrics {
    ViewportMetrics {
        width: 800.,
        height: 600.,
        scale_factor,
    }
}

fn assert_fitted(frame: &CanvasFrame) {
    let t = frame.transform;
    let center = t.to_viewport(ModelPoint { x: 1100., y: -550. });
    assert!((center.x - 400.).abs() < 0.001, "{center:?}");
    assert!((center.y - 300.).abs() < 0.001, "{center:?}");
    // Fit the tight geometry even when the document origin is far away.
    let a = t.to_viewport(ModelPoint { x: 1000., y: -600. });
    let b = t.to_viewport(ModelPoint { x: 1200., y: -500. });
    assert!(
        (a.x - 40.).abs() < 0.001 && (b.x - 760.).abs() < 0.001,
        "{a:?} {b:?}"
    );
    assert!(a.y >= 30. && b.y <= 570.);
    assert!((t.scale - 3.6).abs() < 0.0001);
}

#[test]
fn first_preview_fits_offset_content_and_picking_uses_the_same_transform() -> anyhow::Result<()> {
    for scale in [1., 2.] {
        let (mut editor, _dir) = fixture()?;
        editor.set_canvas_metrics(metrics(scale));
        let preview = editor.preview_if_needed()?.unwrap();
        let frame = preview.canvas.unwrap();
        assert_fitted(&frame);
        let bounds = frame
            .geometry
            .source
            .viewport_frame
            .model_bounds(false)
            .unwrap();
        assert!(
            bounds == [1000., -600., 1200., -500.],
            "model bounds must stay tight: {bounds:?}"
        );
        assert!(editor.present_frame(frame.clone()));
        let hits = editor.pick_candidates(
            frame
                .transform
                .to_viewport(ModelPoint { x: 1100., y: -550. }),
            Some(frame.version),
        );
        assert!(!hits.is_empty());
        assert!(editor.preview_if_needed()?.is_none());
        let crosshair = editor.overlay().origin_crosshair.unwrap();
        let origin = frame.transform.to_viewport(ModelPoint::default());
        assert_eq!(crosshair, origin);
    }
    Ok(())
}

#[test]
fn fit_command_before_preview_and_after_manual_navigation_uses_content() -> anyhow::Result<()> {
    let (mut editor, _dir) = fixture()?;
    editor.set_canvas_metrics(metrics(2.));
    // Exercise the actual command route without a preexisting geometry cache.
    editor.dispatch(editor.command(CommandId::FitView))?;
    assert_fitted(&editor.preview()?.canvas.unwrap());
    editor.pan_by(ViewportPoint { x: 900., y: 900. });
    editor.set_zoom_at(0.1, None);
    editor.dispatch(editor.command(CommandId::FitView))?;
    assert_fitted(&editor.preview()?.canvas.unwrap());
    let fitted = editor.view();
    editor.set_canvas_metrics(ViewportMetrics {
        width: 400.,
        ..metrics(2.)
    });
    editor.preview()?;
    assert_eq!(
        editor.view(),
        fitted,
        "Explicit Fit disables automatic layout fitting"
    );
    Ok(())
}

#[test]
fn content_fit_preserves_other_viewports_and_pointer_zoom_anchor() -> anyhow::Result<()> {
    let (mut editor, _dir) = fixture()?;
    editor.set_canvas_metrics(metrics(2.));
    let first = editor.preview()?.canvas.unwrap();
    let first_view = editor.view();
    let second = ViewportId::new();
    editor.register_viewport(second, ViewportKind::Modeling)?;
    editor.set_viewport_metrics(
        second,
        ViewportMetrics {
            width: 300.,
            height: 200.,
            scale_factor: 1.,
        },
    );
    let binding = editor.viewport_binding(second).unwrap();
    editor.fit_viewport(binding)?;
    assert_ne!(editor.viewport_view(binding).unwrap(), first_view);
    assert_eq!(editor.view(), first_view);
    let focus = ViewportPoint { x: 173., y: 271. };
    let source = first.transform.to_model(focus);
    editor.set_zoom_at(first_view.zoom * 1.5, Some(focus));
    let zoomed = editor
        .preview()?
        .canvas
        .unwrap()
        .transform
        .to_viewport(source);
    assert!((zoomed.x - focus.x).abs() < 0.001 && (zoomed.y - focus.y).abs() < 0.001);
    Ok(())
}

#[test]
fn invalidated_geometry_is_fitted_before_the_next_frame() -> anyhow::Result<()> {
    let (mut editor, _dir) = fixture()?;
    editor.set_canvas_metrics(metrics(1.));
    assert_fitted(&editor.preview()?.canvas.unwrap());
    let mesh = ObjectRef::new(ObjectKind::Mesh, uuid::Uuid::from_u128(3).to_string());
    editor.execute(EditorCommand::SetObjectVisible {
        object: mesh.clone(),
        visible: false,
    })?;
    editor.preview_if_needed()?.unwrap();
    assert_eq!(editor.view(), ViewState::default());
    editor.execute(EditorCommand::SetObjectVisible {
        object: mesh,
        visible: true,
    })?;
    assert_fitted(&editor.preview_if_needed()?.unwrap().canvas.unwrap());
    Ok(())
}

#[test]
fn animation_can_fit_without_a_modeling_preview() -> anyhow::Result<()> {
    let (mut editor, _dir) = fixture()?;
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_canvas_metrics(metrics(2.));
    let request = editor.animation_request().unwrap();
    let evaluated = AnimationEvaluator::default().evaluate(&request, |_, _| true)?;
    let binding = editor.primary_viewport_binding().unwrap();
    let (_, transform) = editor
        .animation_viewport_request(binding, evaluated.identity)
        .unwrap();
    let center = transform.to_viewport(ModelPoint { x: 1100., y: -550. });
    assert!((center.x - 400.).abs() < 0.001 && (center.y - 300.).abs() < 0.001);
    let automatic = editor.view();
    editor.pan_by(ViewportPoint { x: 500., y: 500. });
    editor.dispatch(editor.command(CommandId::FitView))?;
    assert_eq!(editor.view(), automatic);
    Ok(())
}

#[test]
fn new_empty_canvas_round_trips_and_fits_to_actual_pixels() -> anyhow::Result<()> {
    let mut editor = support::new_editor()?;
    assert_eq!(
        editor.document().unwrap().model_setup(),
        kasane_core::ModelSetup::default()
    );
    assert!(editor.document().unwrap().source_canvas().is_none());
    editor.set_canvas_metrics(metrics(2.));
    assert!(editor.preview()?.canvas.is_some());
    editor.pan_by(ViewportPoint { x: 100., y: -100. });
    editor.dispatch(editor.command(CommandId::FitView))?;
    assert_eq!(editor.view(), ViewState::default());
    let output = tempfile::tempdir()?;
    let save = editor.prepare_save(
        editor.active_id().unwrap(),
        &output.path().join("empty"),
        false,
    )?;
    editor.finish_save(&mut save.run())?;
    assert!(!editor.document().unwrap().modified());
    let reopened = Document::open(editor.document().unwrap().path().unwrap())?;
    assert_eq!(reopened.model_setup(), kasane_core::ModelSetup::default());
    Ok(())
}

#[test]
fn imported_rectangle_does_not_change_fitting_or_picking() -> anyhow::Result<()> {
    let mut views = Vec::new();
    for source in [
        None,
        Some(kasane_core::Canvas::new(
            8000.,
            16000.,
            Vec2::new(4000., 8000.),
            100.,
        )),
    ] {
        let (mut editor, _dir) = fixture_with_source(source)?;
        assert_eq!(editor.document().unwrap().source_canvas(), source);
        editor.set_canvas_metrics(metrics(2.));
        let frame = editor.preview()?.canvas.unwrap();
        assert_fitted(&frame);
        let view = editor.view();
        views.push(view);
        assert!(editor.present_frame(frame.clone()));
        assert!(
            !editor
                .pick_candidates(
                    frame
                        .transform
                        .to_viewport(ModelPoint { x: 1100., y: -550. }),
                    Some(frame.version)
                )
                .is_empty()
        );
    }
    assert_eq!(views[0], views[1]);
    Ok(())
}

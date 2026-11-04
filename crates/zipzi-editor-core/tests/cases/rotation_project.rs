use zipzi_editor_core::*;

#[test]
fn imported_project_bones_are_visible_and_pickable() -> anyhow::Result<()> {
    let Some(path) = std::env::var_os("ZIPZI_ROTATION_PROJECT") else {
        eprintln!("SKIP: set ZIPZI_ROTATION_PROJECT to inspect a local project");
        return Ok(());
    };
    let document = Document::open(std::path::Path::new(&path))?;
    assert!(!document.modified());
    let mut editor = super::support::editor_from_document(document);
    editor.reset_view();
    editor.set_canvas_metrics(ViewportMetrics {
        width: 1200.,
        height: 900.,
        scale_factor: 2.,
    });
    editor.set_zoom_at(0.1, None);
    let frame = super::support::present_canvas(&mut editor)?;
    let controls: Vec<_> = frame
        .geometry
        .controls
        .iter()
        .filter(|c| c.kind == ControlKind::Rotation && c.enabled && c.visible)
        .collect();
    assert!(!controls.is_empty());
    assert_eq!(editor.overlay().rotation_bones.len(), controls.len());
    let mut lengths = Vec::new();
    for control in &controls {
        let g = frame
            .geometry
            .deformer_gizmo(control, frame.transform)
            .unwrap();
        let tip = g.handles[0].1;
        let evaluated = frame
            .geometry
            .source
            .viewport_frame
            .deformers
            .iter()
            .find(|d| d.id == control.object.id)
            .unwrap();
        let axis = evaluated.rotation_axis.unwrap();
        let display = control.rotation_axis.unwrap();
        assert!((display.x + axis.y).abs() < 1e-5);
        assert!((display.y - axis.x).abs() < 1e-5);
        let length = (tip.x - g.center.x).hypot(tip.y - g.center.y);
        assert!(
            length.is_finite() && length > 0.,
            "{}: bone has invalid length {length} px",
            control.object.id
        );
        lengths.push(length);
        let body = ViewportPoint {
            x: (g.outline[1].0.x + g.outline[3].0.x) / 2.,
            y: (g.outline[1].0.y + g.outline[3].0.y) / 2.,
        };
        assert!(
            editor
                .pick_candidates(body, Some(frame.version))
                .iter()
                .any(|c| c.object == control.object),
            "{} missing from hitTest",
            control.object.id
        );
    }
    lengths.sort_by(f32::total_cmp);
    println!(
        "{} visible Rotation bones at 10% zoom; length {:.3}–{:.3} px; all participate in hitTest",
        controls.len(),
        lengths[0],
        lengths[lengths.len() - 1]
    );
    Ok(())
}

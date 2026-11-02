use super::*;
use zipzi_editor_core::{CanvasInput, EditorCommand, PropertyValue};

#[gpui_kit::test]
fn new_warp_density_matches_properties_and_combined_canvas_controls(cx: &mut TestAppContext) {
    let mut document = zipzi_editor_core::Document::new_blank().unwrap();
    let plan = document
        .plan_scene_object_creation(zipzi_editor_core::SceneObjectType::Warp, "Warp".into(), &[])
        .unwrap();
    let object = document
        .execute_scene_object_creation(&plan)
        .unwrap()
        .suggested_selection[0]
        .clone();
    document.navigate(object.clone()).unwrap();
    let h = setup_with_document(cx, document, tempfile::tempdir().unwrap());
    show(&h, "property-input-bezier_rows", cx);
    h.view.update(cx, |view, cx| {
        view.editor
            .execute(EditorCommand::SelectObject(object.clone()))
            .unwrap();
        let values = view
            .editor
            .document()
            .unwrap()
            .property_values(&object)
            .unwrap();
        for (field, expected) in [
            (schema::deformer::COLUMNS.key, 5),
            (schema::deformer::ROWS.key, 5),
            (schema::deformer::BEZIER_COLUMNS.key, 2),
            (schema::deformer::BEZIER_ROWS.key, 2),
        ] {
            assert_eq!(values[&field], PropertyValue::Integer(expected));
        }
        view.redraw(cx);
    });
    let history = h.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().history_lengths()
    });
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            let frame = view.editor.preview().unwrap().canvas.unwrap();
            assert!(view.editor.present_frame(frame.clone()));
            view.viewport_frames
                .entry(view.viewport_slots[0])
                .or_default()
                .modeling = Some(crate::preview::Presented {
                image: crate::preview::test_image(),
                canvas: frame,
            });
            cx.notify();
        });
        window.render_frame(cx);
        use zipzi_editor_core::OverlayVertexKind as K;
        assert!(window.try_find("deformer-warp-bezier").is_none());
        assert!(window.try_find("deformer-warp-fine").is_none());
        let overlay = h.view.read(cx).editor.overlay();
        assert_eq!(
            overlay
                .vertices
                .iter()
                .filter(|v| v.kind == K::Warp)
                .count(),
            9
        );
        assert_eq!(
            overlay
                .vertices
                .iter()
                .filter(|v| v.kind == K::WarpGrid)
                .count(),
            36
        );
        assert!(!overlay.bezier_tangents.is_empty());
        assert!(!overlay.bezier_lines.is_empty());
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert_eq!(view.editor.document().unwrap().history_lengths(), history);
    });
}

#[gpui_kit::test]
fn warp_density_is_inline_and_removed_actions_are_absent(cx: &mut TestAppContext) {
    let (document, assets, _) = warp_grid_document();
    let h = setup_with_document(cx, document, assets);
    show(&h, "property-input-bezier_rows", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let panel = window.find("object-properties").bounds();
        for id in ["columns", "rows", "bezier_columns", "bezier_rows"] {
            let bounds = window.find(format!("property-input-{id}")).bounds();
            assert!(panel.contains(&bounds.center()) && bounds.right() <= panel.right());
            assert!(
                bounds.size.width >= gpui_kit::px(30.),
                "{id} must show its value"
            );
        }
        for id in [
            "property-warp-transform",
            "property-warp-frame",
            "warp-grid-dialog",
            "warp-grid-interpolation",
            "property-warp-keep-children",
            "deformer-keep-children",
        ] {
            assert!(window.try_find(id).is_none());
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn inline_density_commits_and_undoes_while_bezier_density_keeps_grid_points(
    cx: &mut TestAppContext,
) {
    let (document, assets, object) = warp_grid_document();
    let h = setup_with_document(cx, document, assets);
    for (field, value, key) in [
        ("columns", "4", schema::deformer::COLUMNS.key),
        ("bezier_columns", "3", schema::deformer::BEZIER_COLUMNS.key),
    ] {
        let before = h.view.read_with(cx, |view, _| {
            view.editor
                .document()
                .unwrap()
                .object_index()
                .get(&object)
                .unwrap()
                .properties["warp"]["points"]
                .clone()
        });
        fill(&h, &format!("property-input-{field}"), value, cx);
        cx.update_window(h.window, |_, window, cx| {
            window.press("enter", cx);
            window.render_frame(cx);
        })
        .unwrap();
        h.view.read_with(cx, |view, _| {
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            let doc = view.editor.document().unwrap();
            assert_eq!(
                doc.property_values(&object).unwrap()[&key],
                PropertyValue::Integer(value.parse().unwrap())
            );
            if field == "bezier_columns" {
                assert_eq!(
                    doc.object_index().get(&object).unwrap().properties["warp"]["points"],
                    before
                );
            }
            assert!(!doc.has_pending_edit());
        });
    }
    h.view.update(cx, |view, cx| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
        view.editor.execute(EditorCommand::Undo).unwrap();
        view.redraw(cx);
    });
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .property_values(&object)
                .unwrap()[&schema::deformer::BEZIER_COLUMNS.key],
            PropertyValue::Integer(1)
        )
    });
}

#[gpui_kit::test]
fn inline_density_escape_and_invalid_values_leave_the_document_unchanged(cx: &mut TestAppContext) {
    let (document, assets, object) = warp_grid_document();
    let h = setup_with_document(cx, document, assets);
    for value in ["6", "0", "65"] {
        fill(&h, "property-input-columns", value, cx);
        cx.update_window(h.window, |_, window, cx| {
            window.press("escape", cx);
            window.render_frame(cx);
        })
        .unwrap();
        h.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            assert_eq!(
                doc.property_values(&object).unwrap()[&schema::deformer::COLUMNS.key],
                PropertyValue::Integer(1)
            );
            assert_eq!(doc.history_lengths(), (0, 0));
            assert!(!doc.has_pending_edit());
        });
    }
}

#[gpui_kit::test]
fn deformer_properties_reset_reform_auto_and_mode_controls_write_through_the_shared_plan(
    cx: &mut TestAppContext,
) {
    let (mut document, assets, object) = warp_grid_document();
    document
        .set_object_property(&object, "warp.rows", serde_json::json!(4))
        .unwrap();
    document
        .set_object_property(&object, "warp.columns", serde_json::json!(4))
        .unwrap();
    let saved = document.save(&assets.path().join("dense")).unwrap();
    let mut document = zipzi_editor_core::Document::open(&saved.manifest).unwrap();
    document.navigate(object.clone()).unwrap();
    let h = setup_with_document(cx, document, assets);
    show(&h, "property-warp-reset", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-warp-reset", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let before = h.view.update(cx, |view, cx| {
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        view.editor
            .execute(EditorCommand::SelectObject(object.clone()))
            .unwrap();
        let frame = document_fixture::present_canvas(&mut view.editor).unwrap();
        let control = frame
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap();
        let point = frame.transform.to_viewport(control.points[12]);
        let before = view
            .editor
            .document()
            .unwrap()
            .object_index()
            .get(&object)
            .unwrap()
            .properties["warp"]["points"]
            .clone();
        document_fixture::pointer_input(
            &mut view.editor,
            CanvasInput::Down {
                point,
                shift: false,
                alt: false,
                primary: false,
            },
        )
        .unwrap();
        document_fixture::pointer_input(
            &mut view.editor,
            CanvasInput::Up {
                point: zipzi_editor_core::ViewportPoint {
                    x: point.x + 10.,
                    y: point.y,
                },
            },
        )
        .unwrap();
        view.redraw(cx);
        before
    });
    let detail = h.view.read_with(cx, |view, _| {
        view.editor
            .document()
            .unwrap()
            .object_index()
            .get(&object)
            .unwrap()
            .properties["warp"]["points"]
            .clone()
    });
    assert_ne!(detail, before);
    show(&h, "property-warp-reform", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-warp-reform", cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .object_index()
                .get(&object)
                .unwrap()
                .properties["warp"]["points"],
            before
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (3, 0));
    });
    h.view.update(cx, |view, cx| {
        view.editor.execute(EditorCommand::Undo).unwrap();
        view.redraw(cx);
    });
    show(&h, "property-toggle-auto_reform", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-toggle-auto_reform", cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.property_values(&object).unwrap()[&schema::deformer::AUTO_REFORM.key],
            PropertyValue::Bool(true)
        );
        assert_eq!(
            doc.object_index().get(&object).unwrap().properties["warp"]["points"],
            detail
        );
    });
    show(&h, "property-select-bezier_edit_mode", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-select-bezier_edit_mode", cx);
        window.render_frame(cx);
        window.press("down", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .property_values(&object)
                .unwrap()[&schema::deformer::EDIT_MODE.key],
            PropertyValue::Text("smooth_local".into())
        );
    });
}

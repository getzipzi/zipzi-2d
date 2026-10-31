use super::*;
use zipzi_editor_core::{CanvasInput, EditorCommand, PropertyValue};

#[gpui_kit::test]
fn deformer_properties_keep_children_mode_applies_numeric_transform_without_model_history_for_the_toggle(
    cx: &mut TestAppContext,
) {
    let (mut document, assets, object) = warp_grid_document();
    // The general scene fixture deliberately stores canvas-sized mesh values
    // below a normalized Warp. Use realistic child coordinates for this gate.
    let saved = document.save(&assets.path().join("preservation")).unwrap();
    let mut sdk = zipzi_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::Canvas::new(256., 256., Default::default(), 1.),
    )
    .unwrap();
    sdk.open_project(&saved.manifest, None).unwrap();
    let meshes = sdk
        .document()
        .mesh_order()
        .iter()
        .filter_map(|id| sdk.document().get_mesh(id))
        .filter(|m| m.deformer_id == object.id)
        .cloned()
        .collect::<Vec<_>>();
    sdk.edit("Normalize child fixture", None, |edit| {
        for mut mesh in meshes {
            for p in &mut mesh.base_positions {
                p.x /= 256.;
                p.y /= 256.;
            }
            edit.replace_mesh(mesh)?;
        }
        Ok(())
    })
    .unwrap();
    sdk.save_project(&saved.manifest, None).unwrap();
    let mut document = zipzi_editor_core::Document::open(&saved.manifest).unwrap();
    document.navigate(object.clone()).unwrap();
    let h = setup_with_document(cx, document, assets);
    let before = h.view.update(cx, |view, cx| {
        view.editor
            .execute(EditorCommand::SelectObject(object.clone()))
            .unwrap();
        let frame = document_fixture::present_canvas(&mut view.editor).unwrap();
        view.redraw(cx);
        frame
            .geometry
            .meshes
            .iter()
            .flat_map(|m| m.vertices.iter().map(|(_, p)| *p))
            .collect::<Vec<_>>()
    });
    show(&h, "property-warp-keep-children", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-warp-keep-children", cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert!(view.editor.warp_keep_children());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
    show(&h, "property-warp-transform", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-warp-transform", cx);
        window.render_frame(cx);
    })
    .unwrap();
    warp_grid_input(&h, "warp-transform-x", "8", cx);
    warp_grid_input(&h, "warp-transform-angle", "15", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("warp-transform-apply", cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.update(cx, |view, _| {
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        let frame = document_fixture::present_canvas(&mut view.editor).unwrap();
        let after = frame
            .geometry
            .meshes
            .iter()
            .flat_map(|m| m.vertices.iter().map(|(_, p)| *p))
            .collect::<Vec<_>>();
        for (a, b) in before.iter().zip(after) {
            assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
        }
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
}

#[gpui_kit::test]
fn deformer_properties_bezier_density_preview_cancels_without_changing_baked_points(
    cx: &mut TestAppContext,
) {
    let (document, assets, object) = warp_grid_document();
    let h = setup_with_document(cx, document, assets);
    let original = h.view.read_with(cx, |view, _| {
        view.editor
            .document()
            .unwrap()
            .object_index()
            .get(&object)
            .unwrap()
            .properties["warp"]["points"]
            .clone()
    });
    for cancel in [true, false] {
        show(&h, "property-warp-bezier-grid", cx);
        cx.update_window(h.window, |_, window, cx| {
            window.click("property-warp-bezier-grid", cx);
            window.render_frame(cx);
            assert!(window.try_find("warp-grid-interpolation").is_none());
        })
        .unwrap();
        warp_grid_input(&h, "warp-grid-columns", "3", cx);
        warp_grid_input(&h, "warp-grid-rows", "2", cx);
        cx.update_window(h.window, |_, window, _| {
            assert_eq!(window.find("warp-grid-columns").value(), Some("3"));
            assert_eq!(window.find("warp-grid-rows").value(), Some("2"));
        })
        .unwrap();
        h.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            let values = doc.property_values(&object).unwrap();
            assert_eq!(
                values[&schema::deformer::BEZIER_COLUMNS.key],
                PropertyValue::Integer(1)
            );
            assert_eq!(
                doc.object_index().get(&object).unwrap().properties["warp_authoring"]["bezier_grid"]
                    ["columns"],
                3
            );
            assert_eq!(
                doc.object_index().get(&object).unwrap().properties["warp"]["points"],
                original
            );
            assert_eq!(doc.history_lengths(), (0, 0));
        });
        cx.update_window(h.window, |_, window, cx| {
            assert!(window.try_find("warp-bezier-metrics").is_some());
            if cancel {
                window.press("escape", cx);
            } else {
                window.click("warp-grid-apply", cx);
            }
            window.render_frame(cx);
        })
        .unwrap();
        h.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            let values = doc.property_values(&object).unwrap();
            assert_eq!(
                values[&schema::deformer::BEZIER_COLUMNS.key],
                PropertyValue::Integer(if cancel { 1 } else { 3 })
            );
            assert_eq!(
                values[&schema::deformer::COLUMNS.key],
                PropertyValue::Integer(1)
            );
            assert_eq!(
                doc.object_index().get(&object).unwrap().properties["warp"]["points"],
                original
            );
            assert_eq!(doc.history_lengths(), (usize::from(!cancel), 0));
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
        view.editor.execute(EditorCommand::ToggleWarpFine).unwrap();
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

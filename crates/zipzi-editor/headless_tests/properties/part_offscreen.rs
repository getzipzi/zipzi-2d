use super::*;
use zipzi_editor_core::{EditorCommand, ObjectKind, PropertyValue};

fn fixture(cx: &mut TestAppContext) -> (super::super::Harness, zipzi_editor_core::ObjectRef) {
    let (mut document, assets) = document_fixture::sample_document_with_parts().unwrap();
    let part = document_fixture::object(ObjectKind::Part, 101);
    document.navigate(part.clone()).unwrap();
    (setup_with_document(cx, document, assets), part)
}

fn click(h: &super::super::Harness, id: &str, cx: &mut TestAppContext) {
    show(h, id, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(id.to_owned(), cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn part_offscreen_checkbox_reveals_settings_and_undo_preserves_the_part(cx: &mut TestAppContext) {
    let (h, part) = fixture(cx);
    click(&h, "property-toggle-offscreen", cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.context().inspected.as_ref(), Some(&part));
        let node = doc
            .part_tree()
            .into_iter()
            .find(|p| p.id == part.id)
            .unwrap();
        assert!(node.offscreen);
        assert!(
            node.children
                .iter()
                .all(|n| n.kind != zipzi_editor_core::TreeNodeKind::Offscreen)
        );
        assert_eq!(doc.history_lengths(), (1, 0));
    });
    show(&h, "property-input-offscreen_opacity", cx);
    fill(&h, "property-input-offscreen_opacity", "45", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.property_values(&part).unwrap()[&schema::part::OPACITY.key],
            PropertyValue::Float(0.45)
        );
        assert_eq!(doc.history_lengths(), (2, 0));
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
    });
    click(&h, "property-toggle-offscreen", cx);
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .property_values(&part)
                .unwrap()[&schema::part::OFFSCREEN.key],
            PropertyValue::Bool(false)
        );
    });
    h.view.update(cx, |view, cx| {
        view.editor.execute(EditorCommand::Undo).unwrap();
        view.redraw(cx);
    });
    cx.run_until_parked();
    show(&h, "property-input-offscreen_opacity", cx);
    h.view.read_with(cx, |view, _| {
        let values = view
            .editor
            .document()
            .unwrap()
            .property_values(&part)
            .unwrap();
        assert_eq!(
            values[&schema::part::OFFSCREEN.key],
            PropertyValue::Bool(true)
        );
        assert_eq!(
            values[&schema::part::OPACITY.key],
            PropertyValue::Float(0.45)
        );
    });
}

#[gpui_kit::test]
fn part_offscreen_checkbox_is_disabled_on_a_locked_part(cx: &mut TestAppContext) {
    let (h, part) = fixture(cx);
    h.view.update(cx, |view, cx| {
        view.editor
            .execute(EditorCommand::SetObjectsLocked {
                objects: vec![part.clone()],
                locked: true,
            })
            .unwrap();
        view.redraw(cx);
    });
    cx.run_until_parked();
    show(&h, "property-toggle-offscreen", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-toggle-offscreen", cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .property_values(&part)
                .unwrap()[&schema::part::OFFSCREEN.key],
            PropertyValue::Bool(false)
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
}

#[gpui_kit::test]
fn part_offscreen_removal_retires_focused_inputs_and_delayed_events(cx: &mut TestAppContext) {
    use gpui_kit::component::input::InputEvent;
    for external in [false, true] {
        let (h, part) = fixture(cx);
        click(&h, "property-toggle-offscreen", cx);
        show(&h, "property-input-offscreen_opacity", cx);
        fill(&h, "property-input-offscreen_opacity", "45", cx);
        let old_input = h.view.read_with(cx, |view, cx| {
            view.dock
                .as_ref()
                .unwrap()
                .properties
                .read(cx)
                .field_input(schema::part::OPACITY.key)
        });
        if external {
            h.view.update(cx, |view, cx| {
                view.editor
                    .execute(EditorCommand::SetObjectProperty {
                        object: part.clone(),
                        field: "offscreen.present".into(),
                        value: serde_json::json!(false),
                    })
                    .unwrap();
                view.redraw(cx);
            });
            cx.run_until_parked();
        } else {
            click(&h, "property-toggle-offscreen", cx);
        }
        let history = h.view.read_with(cx, |view, _| {
            view.editor.document().unwrap().history_lengths()
        });
        cx.update_window(h.window, |_, window, cx| {
            window.render_frame(cx);
            window.input("9", cx);
            old_input.update(cx, |input, cx| {
                input.set_value("99", window, cx);
                cx.emit(InputEvent::Blur);
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        h.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            assert_eq!(doc.history_lengths(), history);
            assert_eq!(
                doc.property_values(&part).unwrap()[&schema::part::OFFSCREEN.key],
                PropertyValue::Bool(false)
            );
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        });
        // Recreating the form must not replay the discarded 45/99 draft.
        click(&h, "property-toggle-offscreen", cx);
        h.view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor
                    .document()
                    .unwrap()
                    .property_values(&part)
                    .unwrap()[&schema::part::OPACITY.key],
                PropertyValue::Float(1.)
            );
        });
    }
}

#[gpui_kit::test]
fn part_offscreen_disabled_composite_is_recovered_by_the_part_enabled_checkbox(
    cx: &mut TestAppContext,
) {
    let (mut document, assets) = document_fixture::sample_document_with_parts().unwrap();
    let part = document_fixture::object(ObjectKind::Part, 101);
    document
        .set_object_property(&part, schema::part::OFFSCREEN.path, serde_json::json!(true))
        .unwrap();
    document
        .set_object_property(&part, schema::part::OPACITY.path, serde_json::json!(0.4))
        .unwrap();
    document
        .set_object_property(
            &part,
            schema::part::COMPOSITE_VISIBLE.path,
            serde_json::json!(false),
        )
        .unwrap();
    document.navigate(part.clone()).unwrap();
    let history = document.history_lengths();
    let h = setup_with_document(cx, document, assets);
    show(&h, "property-toggle-enabled", cx);
    cx.update_window(h.window, |_, window, _| {
        assert_eq!(
            window.find("property-toggle-enabled").checked(),
            Some(false)
        );
    })
    .unwrap();
    click(&h, "property-toggle-enabled", cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        let values = doc.property_values(&part).unwrap();
        assert_eq!(
            values[&schema::part::ENABLED.key],
            PropertyValue::Bool(true)
        );
        assert_eq!(
            values[&schema::part::COMPOSITE_VISIBLE.key],
            PropertyValue::Bool(true)
        );
        assert_eq!(
            values[&schema::part::OPACITY.key],
            PropertyValue::Float(0.4)
        );
        assert_eq!(doc.history_lengths().0, history.0 + 1);
    });
    h.view.update(cx, |view, cx| {
        view.editor.execute(EditorCommand::Undo).unwrap();
        view.redraw(cx);
    });
    cx.run_until_parked();
    show(&h, "property-toggle-enabled", cx);
    cx.update_window(h.window, |_, window, _| {
        assert_eq!(
            window.find("property-toggle-enabled").checked(),
            Some(false)
        );
    })
    .unwrap();
}

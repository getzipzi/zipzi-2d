use super::{document_fixture, setup, setup_with_document};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

fn part_properties_document() -> (editor_core::Document, tempfile::TempDir) {
    let (mut document, assets) = document_fixture::sample_document_with_parts().unwrap();
    document
        .navigate(document_fixture::object(editor_core::ObjectKind::Part, 101))
        .unwrap();
    (document, assets)
}

fn part_draw_order(document: &editor_core::Document) -> f32 {
    document
        .object_index()
        .get(&document_fixture::object(
            editor_core::ObjectKind::Part,
            101,
        ))
        .unwrap()
        .properties["draw_order"]
        .as_f64()
        .unwrap() as f32
}

#[gpui_kit::test]
fn mesh_name_and_runtime_id_edit_without_changing_object_identity(cx: &mut TestAppContext) {
    use editor_core::{PropertyId, PropertyValue};
    let harness = setup(cx);
    let object = harness.view.read_with(cx, |view, _| {
        view.editor.active_inspection().unwrap().object().unwrap()
    });
    fill(&harness, "property-input-name", "Edited ArtMesh", cx);
    fill(&harness, "property-input-runtime_id", "MeshNew", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        let values = doc.property_values(&object).unwrap();
        assert_eq!(
            values[&PropertyId::Name],
            PropertyValue::Text("Edited ArtMesh".into())
        );
        assert_eq!(
            values[&PropertyId::RuntimeId],
            PropertyValue::Text("MeshNew".into())
        );
        assert_eq!(doc.context().inspected.as_ref(), Some(&object));
        assert_eq!(doc.history_lengths(), (2, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        assert_eq!(
            window.find("property-title-name").label(),
            Some("Edited ArtMesh")
        );
        assert_eq!(window.find("property-title-type").label(), Some("ArtMesh"));
        assert!(window.try_find("object-property-id").is_none());
        harness.view.update(cx, |view, cx| {
            view.run_command(editor_core::CommandId::Deselect, window, cx);
        });
        window.render_frame(cx);
        assert_eq!(window.find("property-title-name").label(), Some("Document"));
        assert_eq!(window.find("property-title-type").label(), Some("Document"));
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().context().inspected, None);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
    });
}

#[gpui_kit::test]
fn part_properties_edit_text_and_toggles_with_document_history(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_parts().unwrap();
    let part = document_fixture::object(editor_core::ObjectKind::Part, 101);
    document.navigate(part.clone()).unwrap();
    let harness = setup_with_document(cx, document, assets);
    fill(&harness, "property-input-name", "Edited Part", cx);
    cx.update_window(harness.window, |_, window, cx| {
        let panel = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .properties
            .clone();
        let input = panel.read(cx).field_input(editor_core::PropertyId::Name);
        assert_eq!(input.read(cx).value().as_ref(), "Edited Part");
        assert!(gpui_kit::Focusable::focus_handle(input.read(cx), cx).is_focused(window));
        let forms = &harness.view.read(cx).property_forms.entries;
        assert!(
            forms.values().any(|form| form
                .fields
                .get(&editor_core::PropertyId::Name)
                .is_some_and(|f| f.raw == "Edited Part")),
            "drafts: {forms:?}"
        );
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.property_values(&part).unwrap()[&editor_core::PropertyId::Name],
            editor_core::PropertyValue::Text("Edited Part".into())
        );
        assert_eq!(doc.history_lengths(), (1, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.click("property-toggle-enabled", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.property_values(&part).unwrap()[&editor_core::PropertyId::Enabled],
            editor_core::PropertyValue::Bool(false)
        );
        assert_eq!(doc.history_lengths(), (2, 0));
    });
    cx.update_window(harness.window, |_, _, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor
                .execute(editor_core::EditorCommand::Undo)
                .unwrap();
            view.redraw(cx);
        });
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .property_values(&part)
                .unwrap()[&editor_core::PropertyId::Enabled],
            editor_core::PropertyValue::Bool(true)
        );
    });
}

fn fill(harness: &super::Harness, field: &str, text: &str, cx: &mut TestAppContext) {
    cx.update_window(harness.window, |_, window, cx| {
        window.activate_window();
        for _ in 0..60 {
            let viewport = window.find("object-properties").bounds();
            let bounds = window
                .try_find(field.to_owned())
                .map(|element| element.bounds());
            if bounds.is_some_and(|bounds| {
                bounds.top() >= viewport.top() && bounds.bottom() <= viewport.bottom()
            }) {
                break;
            }
            let delta = if bounds.is_some_and(|bounds| bounds.top() < viewport.top()) {
                32.
            } else {
                -32.
            };
            window.scroll(
                "object-properties",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
                    gpui_kit::px(0.),
                    gpui_kit::px(delta),
                )),
                cx,
            );
            window.render_frame(cx);
        }
        window.click(field.to_owned(), cx);
        window.press("cmd-a", cx);
        window.input(text, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn parameter_fields_submit_individually_and_invalid_blur_restores_committed_value(
    cx: &mut TestAppContext,
) {
    use editor_core::{ObjectKind, ObjectRef, PropertyId};
    let (mut document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let parameter = ObjectRef::new(ObjectKind::Parameter, &document.parameters()[0].id);
    document.navigate(parameter.clone()).unwrap();
    let harness = setup_with_document(cx, document, assets);
    fill(&harness, "property-input-minimum", "10", cx);
    fill(&harness, "property-input-maximum", "20", cx);
    harness.view.read_with(cx, |view, _| {
        let form = view
            .property_forms
            .entries
            .values()
            .find(|form| form.fields.contains_key(&PropertyId::Minimum))
            .unwrap();
        assert_eq!(form.fields[&PropertyId::Minimum].raw, "0");
        assert!(!form.fields[&PropertyId::Minimum].dirty());
        assert!(view.edit_error.is_some());
    });
    fill(&harness, "property-input-default_value", "15", cx);
    fill(&harness, "property-input-minimum", "10", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
        assert!(window.try_find("property-apply-range").is_none());
        assert!(window.try_find("property-revert-range").is_none());
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        let p = doc.parameter_draft(&parameter.id).unwrap();
        assert_eq!((p.minimum, p.maximum, p.default_value), (10., 20., 15.));
        assert_eq!(doc.history_lengths(), (3, 0));
        assert!(
            !view
                .property_forms
                .dirty_document(view.editor.active_id().unwrap())
        );
    });
}

#[gpui_kit::test]
fn input_failure_does_not_resize_header_or_move_the_scroll_anchor(cx: &mut TestAppContext) {
    use editor_core::{ObjectKind, ObjectRef, PropertyId};
    let (mut document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    document
        .navigate(ObjectRef::new(
            ObjectKind::Parameter,
            &document.parameters()[0].id,
        ))
        .unwrap();
    let harness = setup_with_document(cx, document, assets);
    let before = cx
        .update_window(harness.window, |_, window, cx| {
            scroll_bottom_in_window(window, cx);
            (
                window.find("property-identity").bounds(),
                window.find("object-property-default_value").bounds(),
            )
        })
        .unwrap();
    fill(&harness, "property-input-default_value", "invalid", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
        assert_eq!(window.find("property-identity").bounds(), before.0);
        assert_eq!(
            window.find("object-property-default_value").bounds(),
            before.1
        );
        let panel = &harness.view.read(cx).dock.as_ref().unwrap().properties;
        assert_eq!(
            panel
                .read(cx)
                .field_input(PropertyId::DefaultValue)
                .read(cx)
                .value()
                .as_ref(),
            "0"
        );
    })
    .unwrap();
}
fn scroll_bottom_in_window(window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
    for _ in 0..3 {
        window.scroll(
            "object-properties",
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
                gpui_kit::px(0.),
                gpui_kit::px(-100_000.),
            )),
            cx,
        );
        window.render_frame(cx);
    }
}

#[gpui_kit::test]
fn properties_external_conflict_restores_latest_value_without_overwriting_it(
    cx: &mut TestAppContext,
) {
    use editor_core::{EditorCommand, ObjectKind, PropertyId, PropertyValue};
    let (mut document, assets) = document_fixture::sample_document_with_parts().unwrap();
    let part = document_fixture::object(ObjectKind::Part, 101);
    document.navigate(part.clone()).unwrap();
    let harness = setup_with_document(cx, document, assets);
    fill(&harness, "property-input-name", "Local draft", cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor
                .execute(EditorCommand::SetObjectProperty {
                    object: part.clone(),
                    field: "name".into(),
                    value: serde_json::json!("External name"),
                })
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
        window.press("enter", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.property_values(&part).unwrap()[&PropertyId::Name],
            PropertyValue::Text("External name".into())
        );
        assert_eq!(doc.history_lengths(), (1, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("property-error-name").is_none());
        assert!(window.try_find("property-revert-name").is_none());
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.edit_error.is_some());
        let target = crate::ui::property_form::PropertyTarget {
            document: view.editor.active_id().unwrap(),
            context: editor_core::EditingContext::Modeling,
            object: part,
        };
        let form = &view.property_forms.entries[&target];
        assert_eq!(form.fields[&PropertyId::Name].raw, "External name");
        assert!(!form.dirty());
    });
}

#[gpui_kit::test]
fn properties_draw_order_scrub_previews_then_commits_once_and_escape_cancels(
    cx: &mut TestAppContext,
) {
    use super::slide_number_input::{press, release, slide};
    use gpui_kit::{point, px};
    let (document, assets) = part_properties_document();
    let harness = setup_with_document(cx, document, assets);
    let origin = cx
        .update_window(harness.window, |_, window, cx| {
            window.activate_window();
            window.render_frame(cx);
            let origin = window
                .within("property-input-draw_order")
                .find("slide-number-input")
                .bounds()
                .center();
            press(window, origin, cx);
            origin
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin + point(px(80.), px(0.)), false, cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(part_draw_order(doc), 10.);
        assert_eq!(doc.history_lengths(), (0, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        release(window, origin + point(px(80.), px(0.)), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(part_draw_order(view.editor.document().unwrap()), 10.);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        press(window, origin, cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin + point(px(80.), px(0.)), false, cx);
        window.press("escape", cx);
        release(window, origin + point(px(80.), px(0.)), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(part_draw_order(view.editor.document().unwrap()), 10.);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
}

#[gpui_kit::test]
fn properties_save_reverts_invalid_input_and_saves_the_visible_committed_value(
    cx: &mut TestAppContext,
) {
    let (mut document, assets) = part_properties_document();
    document.save(&assets.path().join("saved.kasane")).unwrap();
    let harness = setup_with_document(cx, document, assets);
    fill(&harness, "property-input-draw_order", "512", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
    })
    .unwrap();
    fill(&harness, "property-input-draw_order", "-", cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.save(false, window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(part_draw_order(view.editor.document().unwrap()), 512.);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
        assert!(
            !view
                .property_forms
                .dirty_document(view.editor.active_id().unwrap())
        );
        let saved =
            editor_core::Document::open(view.editor.document().unwrap().path().unwrap()).unwrap();
        assert_eq!(part_draw_order(&saved), 512.);
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let input = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .properties
            .read(cx)
            .field_input(editor_core::PropertyId::DrawOrder);
        assert_eq!(input.read(cx).value().as_ref(), "512");
    })
    .unwrap();
}

#[gpui_kit::test]
fn properties_optional_fade_distinguishes_default_from_explicit_zero(cx: &mut TestAppContext) {
    use editor_core::{ObjectKind, PropertyId, PropertyValue};
    let (mut document, assets) = document_fixture::animation_document().unwrap();
    let motion = document
        .object_index()
        .objects
        .iter()
        .find(|info| info.reference.kind == ObjectKind::Motion)
        .unwrap()
        .reference
        .clone();
    document.navigate(motion.clone()).unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("property-custom-fade_in", cx);
        window.render_frame(cx);
        assert!(window.try_find("property-input-fade_in").is_none());
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .property_values(&motion)
                .unwrap()[&PropertyId::FadeIn],
            PropertyValue::OptionalFloat(None)
        );
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.click("property-custom-fade_in", cx);
        window.render_frame(cx);
        assert!(window.try_find("property-input-fade_in").is_some());
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .property_values(&motion)
                .unwrap()[&PropertyId::FadeIn],
            PropertyValue::OptionalFloat(Some(0.))
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
    });
}

#[gpui_kit::test]
fn properties_hiding_panel_cancels_drag_and_late_release_cannot_commit(cx: &mut TestAppContext) {
    use super::slide_number_input::{press, release, slide};
    use gpui_kit::{point, px};
    let (document, assets) = part_properties_document();
    let harness = setup_with_document(cx, document, assets);
    let origin = cx
        .update_window(harness.window, |_, window, cx| {
            window.activate_window();
            let origin = window
                .within("property-input-draw_order")
                .find("slide-number-input")
                .bounds()
                .center();
            press(window, origin, cx);
            origin
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin + point(px(80.), px(0.)), false, cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(part_draw_order(view.editor.document().unwrap()), 10.)
    });
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.dock
                .as_ref()
                .unwrap()
                .toggle(editor_core::PanelKind::Properties, window, cx);
            cx.notify();
        });
        window.render_frame(cx);
        release(window, origin + point(px(80.), px(0.)), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(part_draw_order(view.editor.document().unwrap()), 0.);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        assert!(
            !view
                .property_forms
                .dirty_document(view.editor.active_id().unwrap())
        );
    });
}

#[gpui_kit::test]
fn invalid_input_does_not_create_a_phantom_unsaved_document_on_close(cx: &mut TestAppContext) {
    let (document, assets) = part_properties_document();
    let harness = setup_with_document(cx, document, assets);
    fill(&harness, "property-input-draw_order", "-", cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.close_active(window, cx);
            assert!(!view.modal);
            assert!(view.editor.active_id().is_none());
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn properties_ime_composition_does_not_submit_or_save_partial_text(cx: &mut TestAppContext) {
    use editor_core::{ObjectKind, PropertyId, PropertyValue};
    use gpui_kit::EntityInputHandler;
    let (mut document, assets) = document_fixture::sample_document_with_parts().unwrap();
    let part = document_fixture::object(ObjectKind::Part, 101);
    document.navigate(part.clone()).unwrap();
    let harness = setup_with_document(cx, document, assets);
    let input = cx
        .update_window(harness.window, |_, window, cx| {
            window.activate_window();
            window.click("property-input-name", cx);
            let panel = harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .properties
                .clone();
            let input = panel.read(cx).field_input(PropertyId::Name);
            input.update(cx, |input, cx| {
                input.replace_and_mark_text_in_range(Some(0..4), "名称", None, window, cx);
            });
            input
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
        harness.view.update(cx, |view, cx| {
            view.save(false, window, cx);
            assert!(view.edit_error.is_some());
            assert!(!view.modal);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
            view.close_active(window, cx);
            assert!(view.editor.active_id().is_some());
            assert!(!view.modal);
            view.close(window, cx);
            assert!(view.editor.active_id().is_some());
            assert!(!view.editor.exiting());
        });
        input.update(cx, |input, cx| input.unmark_text(window, cx));
        window.press("enter", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.property_values(&part).unwrap()[&PropertyId::Name],
            PropertyValue::Text("名称".into())
        );
        assert_eq!(doc.history_lengths(), (1, 0));
        assert!(view.edit_error.is_none());
    });
}

#[gpui_kit::test]
fn compact_properties_align_and_wrap_without_overflow_in_narrow_panels(cx: &mut TestAppContext) {
    use gpui_kit::{prelude::*, *};

    struct PropertyHarness(Pixels);
    impl Render for PropertyHarness {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            crate::ui::compact::property_row(
                "draw_order",
                crate::ui::compact::property_value(
                    "a-long-identifier-01234567-89ab-cdef-0123-456789abcdef",
                ),
                cx,
            )
            .id("property-row")
            .test_support()
            .w(self.0)
        }
    }

    cx.update(gpui_kit::init);
    let (window, harness) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|_| PropertyHarness(px(360.)))
        })
        .unwrap()
    });
    for width in [360., 180., 100.] {
        cx.update_window(window, |_, window, cx| {
            harness.update(cx, |harness, cx| {
                harness.0 = px(width);
                cx.notify();
            });
            window.render_frame(cx);
            let row = window.find("property-row").bounds();
            let label = window.find("property-label-draw_order").bounds();
            let value = window.find("property-value").bounds();
            for bounds in [label, value] {
                assert!(bounds.left() >= row.left());
                assert!(bounds.right() <= row.right());
                assert!(bounds.bottom() <= row.bottom());
            }
            if width == 360. {
                assert!(value.left() >= label.right());
                assert!(value.top() < label.bottom());
            } else {
                assert!(value.top() >= label.bottom());
            }
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn switching_objects_settles_only_the_previous_objects_input(cx: &mut TestAppContext) {
    use editor_core::{EditorCommand, ObjectKind, PropertyId, PropertyValue};
    let harness = setup(cx);
    let (first, second) = harness.view.read_with(cx, |view, _| {
        let first = view.editor.active_inspection().unwrap().object().unwrap();
        let second = view
            .editor
            .document()
            .unwrap()
            .object_index()
            .objects
            .iter()
            .find(|info| info.reference.kind == ObjectKind::Mesh && info.reference != first)
            .unwrap()
            .reference
            .clone();
        (first, second)
    });
    fill(
        &harness,
        "property-input-name",
        "Changed before switching",
        cx,
    );
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor
                .execute(EditorCommand::Navigate(second.clone()))
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.property_values(&first).unwrap()[&PropertyId::Name],
            PropertyValue::Text("Changed before switching".into())
        );
        assert_ne!(
            doc.property_values(&second).unwrap()[&PropertyId::Name],
            PropertyValue::Text("Changed before switching".into())
        );
        assert_eq!(doc.history_lengths(), (1, 0));
    });
    let runtime_id = harness.view.read_with(cx, |view, _| {
        view.editor
            .document()
            .unwrap()
            .property_values(&second)
            .unwrap()[&PropertyId::RuntimeId]
            .clone()
    });
    fill(&harness, "property-input-runtime_id", "", cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor.execute(EditorCommand::Navigate(first)).unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .property_values(&second)
                .unwrap()[&PropertyId::RuntimeId],
            runtime_id
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
        assert!(
            !view
                .property_forms
                .dirty_document(view.editor.active_id().unwrap())
        );
    });
}

#[gpui_kit::test]
fn switching_objects_during_ime_composition_does_not_publish_partial_text(cx: &mut TestAppContext) {
    use editor_core::{EditorCommand, ObjectKind, PropertyId};
    use gpui_kit::EntityInputHandler;
    let harness = setup(cx);
    let (first, second, name) = harness.view.read_with(cx, |view, _| {
        let first = view.editor.active_inspection().unwrap().object().unwrap();
        let doc = view.editor.document().unwrap();
        let name = doc.property_values(&first).unwrap()[&PropertyId::Name].clone();
        let second = doc
            .object_index()
            .objects
            .iter()
            .find(|info| info.reference.kind == ObjectKind::Mesh && info.reference != first)
            .unwrap()
            .reference
            .clone();
        (first, second, name)
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.activate_window();
        window.click("property-input-name", cx);
        let input = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .properties
            .read(cx)
            .field_input(PropertyId::Name);
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(Some(0..7), "预编辑", None, window, cx)
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor
                .execute(EditorCommand::Navigate(second))
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .property_values(&first)
                .unwrap()[&PropertyId::Name],
            name
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        assert!(
            !view
                .property_forms
                .dirty_document(view.editor.active_id().unwrap())
        );
    });
}

#[gpui_kit::test]
fn closing_an_inactive_document_does_not_commit_the_active_property_drag(cx: &mut TestAppContext) {
    use super::slide_number_input::{press, release, slide};
    use gpui_kit::{point, px};
    let (document, assets) = part_properties_document();
    let harness = setup_with_document(cx, document, assets);
    let inactive = cx
        .update_window(harness.window, |_, _, cx| {
            harness.view.update(cx, |view, cx| {
                let active = view.editor.active_id().unwrap();
                let inactive = view.editor.new_document().unwrap();
                view.editor.activate(active).unwrap();
                view.redraw(cx);
                inactive
            })
        })
        .unwrap();
    let origin = cx
        .update_window(harness.window, |_, window, cx| {
            window.activate_window();
            window.render_frame(cx);
            let origin = window
                .within("property-input-draw_order")
                .find("slide-number-input")
                .bounds()
                .center();
            press(window, origin, cx);
            origin
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin + point(px(80.), px(0.)), false, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.close_document(inactive, window, cx);
            assert_eq!(part_draw_order(view.editor.document().unwrap()), 10.);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        });
        release(window, origin + point(px(80.), px(0.)), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
        assert_eq!(view.editor.document_count(), 1);
    });
}

#[gpui_kit::test]
fn form_error_routes_to_status_bar_and_clears_after_timeout(cx: &mut TestAppContext) {
    use editor_core::{ObjectKind, ObjectRef, PropertyId};
    let (mut document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let parameter = ObjectRef::new(ObjectKind::Parameter, &document.parameters()[0].id);
    document.navigate(parameter).unwrap();
    let harness = setup_with_document(cx, document, assets);

    harness.view.read_with(cx, |view, _| {
        assert!(view.edit_error.is_none());
    });

    // Enter an invalid value that violates parameter range (min 999 > max 100)
    fill(&harness, "property-input-minimum", "999", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();

    // Form does NOT cache the error, input restores baseline committed value, and edit_error is set
    harness.view.read_with(cx, |view, _| {
        let form = view
            .property_forms
            .entries
            .values()
            .find(|form| form.fields.contains_key(&PropertyId::Minimum))
            .unwrap();
        assert_eq!(form.fields[&PropertyId::Minimum].raw, "0");
        assert!(!form.fields[&PropertyId::Minimum].dirty());
        assert!(view.edit_error.is_some());
    });

    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("app-status-message").is_some());
    })
    .unwrap();

    // Advance clock past the status message timeout
    cx.background_executor
        .advance_clock(crate::EditorView::STATUS_MESSAGE_TIMEOUT);
    cx.run_until_parked();

    // Status message / edit error is cleared
    harness.view.read_with(cx, |view, _| {
        assert!(view.edit_error.is_none());
        assert!(view.message.is_empty());
    });

    // Test set_status_message
    cx.update_window(harness.window, |_, _, cx| {
        harness.view.update(cx, |view, cx| {
            view.set_status_message("Temporary notification", cx);
        });
    })
    .unwrap();

    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.message, "Temporary notification");
        assert!(view.edit_error.is_none());
    });

    cx.background_executor
        .advance_clock(crate::EditorView::STATUS_MESSAGE_TIMEOUT);
    cx.run_until_parked();

    harness.view.read_with(cx, |view, _| {
        assert!(view.message.is_empty());
        assert!(view.edit_error.is_none());
    });
}

#[gpui_kit::test]
fn document_properties_exclude_import_export_settings(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parts().unwrap();
    let source_canvas = document.source_canvas();
    assert!(source_canvas.is_some());
    let export_settings = document.export_settings();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(editor_core::CommandId::Deselect, window, cx);
        });
        window.render_frame(cx);
        assert_eq!(window.find("property-title-type").label(), Some("Document"));
        window.click("property-group-$details", cx);
        window.render_frame(cx);
        for field in [
            "export_settings.pixels_per_unit",
            "export_settings.padding_fraction",
            "export_settings.padding_pixels",
            "export_settings.minimum_extent",
            "export_settings.bounds",
            "source_canvas.width",
            "source_canvas.height",
            "source_canvas.origin.x",
            "source_canvas.origin.y",
            "source_canvas.pixels_per_unit",
            "source_canvas.flag",
        ] {
            assert!(window.try_find(format!("property-input-{field}")).is_none());
            assert!(
                window
                    .try_find(format!("object-property-{field}"))
                    .is_none()
            );
        }
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let document = view.editor.document().unwrap();
        assert_eq!(document.source_canvas(), source_canvas);
        assert_eq!(document.export_settings(), export_settings);
        assert_eq!(document.history_lengths(), (0, 0));
    });
}

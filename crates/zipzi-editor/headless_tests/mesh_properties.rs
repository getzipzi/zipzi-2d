use super::{
    Harness, document_fixture,
    properties::{fill, select_all, show},
    setup_with_document,
};
use gpui_kit::component::WindowExt;
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};
use zipzi_editor_core::{EditorCommand, ObjectKind, ObjectRef, PropertyValue, property::mesh};

fn command(h: &Harness, command: EditorCommand, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.editor.execute(command).unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
}

fn choose(h: &Harness, field: &str, query: &str, cx: &mut TestAppContext) {
    show(h, field, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(field.to_owned(), cx);
        window.render_frame(cx);
        window.input(query, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(150));
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn mesh_properties_part_masks_and_blends(cx: &mut TestAppContext) {
    let (mut doc, assets) = document_fixture::sample_document_with_scene_objects().unwrap();
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    let mask_source = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[1].id);
    doc.set_object_property(
        &mask_source,
        "runtime_id",
        serde_json::json!("MaskSearchId"),
    )
    .unwrap();
    doc.navigate(object.clone()).unwrap();
    let h = setup_with_document(cx, doc, assets);
    choose(&h, "property-select-part_id", "Eyes", cx);
    cx.update_window(h.window, |_, window, _| {
        assert_eq!(window.find("property-select-part_id").value(), Some("Eyes"));
    })
    .unwrap();
    choose(&h, "property-select-masks", "MaskSearchId", cx);
    h.view.read_with(cx, |view, _| {
        let values = view
            .editor
            .document()
            .unwrap()
            .property_values(&object)
            .unwrap();
        assert_eq!(
            values[&mesh::PART.key],
            PropertyValue::Text(uuid::Uuid::from_u128(103).to_string())
        );
        assert!(
            matches!(&values[&mesh::MASKS.key], PropertyValue::References(ids) if ids.len() == 1)
        );
    });
    show(&h, "property-remove-masks-0", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-remove-masks-0", cx);
        window.render_frame(cx);
        assert!(window.try_find("property-remove-masks-0").is_none());
    })
    .unwrap();
    command(&h, EditorCommand::Undo, cx);
    h.view.read_with(cx, |view, _| {
        assert!(matches!(&view.editor.document().unwrap().property_values(&object).unwrap()[&mesh::MASKS.key], PropertyValue::References(ids) if ids.len() == 1));
    });
    for id in ["property-toggle-inverted_mask", "property-toggle-culling"] {
        show(&h, id, cx);
        cx.update_window(h.window, |_, window, cx| {
            window.click(id, cx);
            window.render_frame(cx);
        })
        .unwrap();
    }
    // Plain selects use keyboard navigation; the first step after Normal is compatible Add.
    show(&h, "property-select-color_blend", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-select-color_blend", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |view, _| {
        let values = view
            .editor
            .document()
            .unwrap()
            .property_values(&object)
            .unwrap();
        assert_eq!(
            values[&mesh::COLOR_BLEND.key],
            PropertyValue::Text("add_compatible".into())
        );
        assert_eq!(values[&mesh::INVERTED_MASK.key], PropertyValue::Bool(true));
        assert_eq!(values[&mesh::CULLING.key], PropertyValue::Bool(true));
        assert!(view.edit_error.is_none());
    });
}

#[gpui_kit::test]
fn mesh_properties_deformer_select_reparents_to_warp_and_root_with_undo(cx: &mut TestAppContext) {
    let (mut doc, assets) = document_fixture::sample_document_with_scene_objects().unwrap();
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    doc.navigate(object.clone()).unwrap();
    let history = doc.history_lengths();
    let original = doc.property_values(&object).unwrap()[&mesh::DEFORMER.key].clone();
    let warp = document_fixture::object(ObjectKind::Deformer, 502);
    let h = setup_with_document(cx, doc, assets);
    choose(&h, "property-select-deformer_id", "Warp", cx);
    cx.update_window(h.window, |_, window, cx| {
        assert!(!window.has_active_dialog(cx));
        assert_eq!(
            window.find("property-select-deformer_id").value(),
            Some("Warp")
        );
        let view = h.view.read(cx);
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.property_values(&object).unwrap()[&mesh::DEFORMER.key],
            PropertyValue::Text(warp.id.clone())
        );
        assert_eq!(doc.context().inspected.as_ref(), Some(&object));
        assert_eq!(doc.history_lengths(), (history.0 + 1, 0));
        assert!(view.edit_error.is_none());
    })
    .unwrap();
    command(&h, EditorCommand::Undo, cx);
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .property_values(&object)
                .unwrap()[&mesh::DEFORMER.key],
            original
        );
    });
    command(&h, EditorCommand::Redo, cx);
    choose(&h, "property-select-deformer_id", "No deformer", cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.property_values(&object).unwrap()[&mesh::DEFORMER.key],
            PropertyValue::Text(String::new())
        );
        assert_eq!(doc.history_lengths(), (history.0 + 2, 0));
    });
    command(&h, EditorCommand::Undo, cx);
    choose(&h, "property-select-deformer_id", "Warp", cx);
    // Re-selecting the current parent keeps redo available.
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor.document().unwrap().history_lengths(),
            (history.0 + 1, 1)
        );
    });
    command(&h, EditorCommand::Redo, cx);
    command(
        &h,
        EditorCommand::SetObjectsLocked {
            objects: vec![warp],
            locked: true,
        },
        cx,
    );
    let history = h.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().history_lengths()
    });
    choose(&h, "property-select-deformer_id", "Warp", cx);
    h.view.read_with(cx, |view, cx| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.property_values(&object).unwrap()[&mesh::DEFORMER.key],
            PropertyValue::Text(String::new())
        );
        assert_eq!(doc.history_lengths(), history);
        assert!(view.edit_error.is_some());
        let panel = view.dock.as_ref().unwrap().properties.read(cx);
        assert_eq!(
            panel
                .field_select(mesh::DEFORMER.key)
                .read(cx)
                .selected_value(),
            Some(&String::new())
        );
    });
}

#[gpui_kit::test]
fn mesh_properties_keyform_hex_percent_and_stale_drafts(cx: &mut TestAppContext) {
    let (mut doc, assets) = document_fixture::rectangle_document(true).unwrap();
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    let parameter = doc.parameters()[0].id.clone();
    doc.navigate(object.clone()).unwrap();
    let h = setup_with_document(cx, doc, assets);
    fill(&h, "property-input-opacity", "35", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    fill(&h, "property-input-multiply", "#804020", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.property_values(&object).unwrap()[&mesh::OPACITY.key],
            PropertyValue::Float(0.35)
        );
        assert_eq!(doc.history_lengths(), (2, 0));
        assert_eq!(
            doc.property_values(&object).unwrap()[&mesh::MULTIPLY.key],
            PropertyValue::Color([128. / 255., 64. / 255., 32. / 255.])
        );
        assert_eq!(
            doc.object_index().get(&object).unwrap().properties["appearance"]["opacity"],
            serde_json::json!(1.)
        );
    });
    fill(&h, "property-input-multiply", "#12", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
        // Invalid HEX stays local to the picker and never changes the document.
    });
    fill(&h, "property-input-opacity", "80", cx);
    command(
        &h,
        EditorCommand::SetPreviewParameter {
            parameter_id: parameter.clone(),
            value: 1.,
        },
        cx,
    );
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .property_values(&object)
                .unwrap()[&mesh::OPACITY.key],
            PropertyValue::Float(1.)
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
    });
    command(
        &h,
        EditorCommand::SetPreviewParameter {
            parameter_id: parameter.clone(),
            value: 0.5,
        },
        cx,
    );
    show(&h, "object-property-opacity", cx);
    cx.update_window(h.window, |_, window, _| {
        assert!(window.try_find("property-input-opacity").is_none());
    })
    .unwrap();
    command(
        &h,
        EditorCommand::SetPreviewParameter {
            parameter_id: parameter,
            value: 0.,
        },
        cx,
    );
    show(&h, "property-input-opacity", cx);
    cx.update_window(h.window, |_, _, cx| {
        let panel = h.view.read(cx).dock.as_ref().unwrap().properties.read(cx);
        assert_eq!(
            panel
                .field_input(mesh::OPACITY.key)
                .read(cx)
                .value()
                .as_ref(),
            "35"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn mesh_properties_color_popup_previews_once_and_escape_cancels(cx: &mut TestAppContext) {
    use super::slide_number_input::{press, release, slide};
    use gpui_kit::{point, px};
    let (mut doc, assets) = document_fixture::rectangle_document(true).unwrap();
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    doc.navigate(object.clone()).unwrap();
    let h = setup_with_document(cx, doc, assets);
    for cancel in [true, false] {
        show(&h, "property-color-multiply", cx);
        let (start, end) = cx
            .update_window(h.window, |_, window, cx| {
                window.activate_window();
                window.click("property-color-multiply", cx);
                window.render_frame(cx);
                let track = window.find("color-picker-sv").bounds();
                let start = track.center();
                let end = point(
                    track.left() + track.size.width * 0.1,
                    track.top() + track.size.height * 0.15,
                );
                press(window, end, cx);
                window.render_frame(cx);
                slide(window, end, false, cx);
                release(window, end, cx);
                window.render_frame(cx);
                (start, end)
            })
            .unwrap();
        h.view.update(cx, |view, _| {
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
            assert_eq!(
                view.editor
                    .document()
                    .unwrap()
                    .property_values(&object)
                    .unwrap()[&mesh::MULTIPLY.key],
                PropertyValue::Color([1.; 3])
            );
            let scene = view.editor.preview().unwrap().scene;
            assert!(scene.runtime_frame.drawables[0].multiply_color[0] < 0.6);
        });
        cx.update_window(h.window, |_, window, cx| {
            if cancel {
                window.press("escape", cx);
            } else {
                window.click("property-color-multiply", cx);
            }
            release(window, end + point(px(1.), px(0.)), cx);
            window.render_frame(cx);
            assert!(
                window.try_find("color-picker-panel").is_none(),
                "popup from {start:?}"
            );
        })
        .unwrap();
        h.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            assert_eq!(doc.history_lengths(), (usize::from(!cancel), 0));
            let value = &doc.property_values(&object).unwrap()[&mesh::MULTIPLY.key];
            if cancel {
                assert_eq!(*value, PropertyValue::Color([1.; 3]));
            } else {
                assert_ne!(*value, PropertyValue::Color([1.; 3]));
            }
        });
    }
    command(&h, EditorCommand::Undo, cx);
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .property_values(&object)
                .unwrap()[&mesh::MULTIPLY.key],
            PropertyValue::Color([1.; 3])
        )
    });
}

#[gpui_kit::test]
fn mesh_properties_many_masks_virtualize_and_keep_the_last_reference_reachable(
    cx: &mut TestAppContext,
) {
    use gpui_kit::{point, px};
    let (mut doc, assets) = document_fixture::sample_document_with_two_layers().unwrap();
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    let saved = doc.save(&assets.path().join("many-masks.zipzi")).unwrap();
    let mut session = zipzi_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::ModelSetup::default(),
    )
    .unwrap();
    session.open_project(&saved.manifest, None).unwrap();
    let template = session.mesh(&object.id).unwrap();
    let ids: Vec<_> = (0..80).map(|_| uuid::Uuid::new_v4().to_string()).collect();
    session
        .edit("Mask sources", None, |edit| {
            for (slot, id) in ids.iter().enumerate() {
                let mut source = template.clone();
                source.id = id.clone();
                source.runtime_id = format!("Mask{slot}");
                source.name = format!("Mask {slot}");
                edit.create_mesh(source)?;
            }
            Ok(())
        })
        .unwrap();
    session.save_project(&saved.manifest, None).unwrap();
    let mut doc = zipzi_editor_core::Document::open(&saved.manifest).unwrap();
    doc.set_object_property(&object, "masks", serde_json::json!(ids))
        .unwrap();
    doc.navigate(object.clone()).unwrap();
    let h = setup_with_document(cx, doc, assets);
    show(&h, "property-references-masks", cx);
    cx.update_window(h.window, |_, window, cx| {
        assert!(window.try_find("property-remove-masks-0").is_some());
        assert!(window.try_find("property-remove-masks-79").is_none());
        let bounds = window.find("property-references-masks").bounds();
        assert!(bounds.size.height <= px(100.));
        window.scroll(
            "property-references-masks",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-10_000.))),
            cx,
        );
        window.render_frame(cx);
        window.click("property-remove-masks-79", cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let values = view.editor.document().unwrap().property_values(&object).unwrap();
        assert!(matches!(&values[&mesh::MASKS.key], PropertyValue::References(current) if current.len() == 79 && !current.contains(&ids[79])));
    });
}

#[gpui_kit::test]
fn mesh_color_session_cancels_on_keyform_switch_and_save_settles_the_new_session(
    cx: &mut TestAppContext,
) {
    let (mut doc, assets) = document_fixture::rectangle_document(true).unwrap();
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    let parameter = doc.parameters()[0].id.clone();
    doc.navigate(object.clone()).unwrap();
    let h = setup_with_document(cx, doc, assets);
    show(&h, "property-color-multiply", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-color-multiply", cx);
        window.render_frame(cx);
        window.click("color-picker-hex", cx);
        select_all(window, cx);
        window.input("#804020", cx);
        window.render_frame(cx);
    })
    .unwrap();
    command(
        &h,
        EditorCommand::SetPreviewParameter {
            parameter_id: parameter,
            value: 1.,
        },
        cx,
    );
    cx.update_window(h.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
        assert!(window.try_find("color-picker-panel").is_none());
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.history_lengths(), (0, 0));
        assert_eq!(
            doc.property_values(&object).unwrap()[&mesh::MULTIPLY.key],
            PropertyValue::Color([1.; 3])
        );
    });
    fill(&h, "property-input-multiply", "#4080C0", cx);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            assert!(view.prepare_property_inputs(view.editor.active_id().unwrap(), window, cx));
        });
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.history_lengths(), (1, 0));
        assert_eq!(
            doc.property_values(&object).unwrap()[&mesh::MULTIPLY.key],
            PropertyValue::Color([64. / 255., 128. / 255., 192. / 255.])
        );
    });
}

#[gpui_kit::test]
fn mesh_color_inline_input_enter_in_one_event_batch_commits_the_live_value(
    cx: &mut TestAppContext,
) {
    let (mut doc, assets) = document_fixture::sample_document().unwrap();
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    doc.navigate(object.clone()).unwrap();
    let h = setup_with_document(cx, doc, assets);
    show(&h, "property-input-multiply", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-input-multiply", cx);
        select_all(window, cx);
        window.input("#FFB080", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.history_lengths(), (1, 0));
        assert_eq!(
            doc.property_values(&object).unwrap()[&mesh::MULTIPLY.key],
            PropertyValue::Color([1., 176. / 255., 128. / 255.])
        );
    });
}

#[gpui_kit::test]
fn mesh_color_suffix_resets_use_defaults_and_preserve_keyform_history(cx: &mut TestAppContext) {
    let (mut doc, assets) = document_fixture::rectangle_document(true).unwrap();
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    doc.navigate(object.clone()).unwrap();
    let h = setup_with_document(cx, doc, assets);
    for (field, text) in [("multiply", "#804020"), ("screen", "#204080")] {
        fill(&h, &format!("property-input-{field}"), text, cx);
        cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
            .unwrap();
    }
    for (field, key, default, authored) in [
        (
            "multiply",
            mesh::MULTIPLY.key,
            [1.; 3],
            [128. / 255., 64. / 255., 32. / 255.],
        ),
        (
            "screen",
            mesh::SCREEN.key,
            [0.; 3],
            [32. / 255., 64. / 255., 128. / 255.],
        ),
    ] {
        let before = h.view.read_with(cx, |view, _| {
            view.editor.document().unwrap().history_lengths().0
        });
        let reset = format!("property-color-{field}-reset");
        show(&h, &reset, cx);
        cx.update_window(h.window, |_, window, cx| {
            let input = window.find(format!("property-input-{field}")).bounds();
            let button = window.find(reset.clone()).bounds();
            assert!(
                input.contains(&button.center()),
                "reset belongs inside the color input"
            );
            window.click(reset.clone(), cx);
            window.render_frame(cx);
        })
        .unwrap();
        h.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            assert_eq!(
                doc.property_values(&object).unwrap()[&key],
                PropertyValue::Color(default)
            );
            assert_eq!(doc.history_lengths(), (before + 1, 0));
        });
        command(&h, EditorCommand::Undo, cx);
        h.view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor
                    .document()
                    .unwrap()
                    .property_values(&object)
                    .unwrap()[&key],
                PropertyValue::Color(authored)
            )
        });
        command(&h, EditorCommand::Redo, cx);
        cx.update_window(h.window, |_, window, cx| {
            window.click(reset.clone(), cx);
            window.render_frame(cx);
        })
        .unwrap();
        h.view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor.document().unwrap().history_lengths(),
                (before + 1, 0)
            )
        });
    }
    // Reset clears an invalid draft without adding a no-op history entry.
    let before = h.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().history_lengths()
    });
    fill(&h, "property-input-multiply", "#12", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-color-multiply-reset", cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.history_lengths(), before);
        assert_eq!(
            doc.property_values(&object).unwrap()[&mesh::MULTIPLY.key],
            PropertyValue::Color([1.; 3])
        );
        assert!(view.edit_error.is_none());
    });
}

#[gpui_kit::test]
fn mesh_color_suffix_reset_replaces_a_popup_preview_without_committing_it_first(
    cx: &mut TestAppContext,
) {
    let (mut doc, assets) = document_fixture::rectangle_document(true).unwrap();
    let object = ObjectRef::new(ObjectKind::Mesh, &doc.layers()[0].id);
    doc.navigate(object.clone()).unwrap();
    let h = setup_with_document(cx, doc, assets);
    show(&h, "property-color-multiply", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-color-multiply", cx);
        window.render_frame(cx);
        window.click("color-picker-hex", cx);
        select_all(window, cx);
        window.input("#AA3344", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(h.window, |_, window, cx| {
        window.click("property-color-multiply-reset", cx);
        window.render_frame(cx);
        assert!(window.try_find("color-picker-panel").is_none());
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.history_lengths(), (0, 0));
        assert_eq!(
            doc.property_values(&object).unwrap()[&mesh::MULTIPLY.key],
            PropertyValue::Color([1.; 3])
        );
    });
}

use super::{Harness, document_fixture, setup_with_document};
use crate::authoring::{AuthoringContext, binding::Control};
use editor_core::{ObjectKind as K, ObjectRef as R, OrdinaryBindingAction as A, PanelKind};
use gpui_kit::{AppContext as _, TestAppContext, px, size, test::TestWindowExt};
fn setup(cx: &mut TestAppContext) -> Harness {
    let (mut d, assets) = document_fixture::sample_document().unwrap();
    d.create_parameter(editor_core::NewParameter {
        name: "Drive".into(),
        runtime_id: "Drive".into(),
        minimum: 0.,
        maximum: 1.,
        default_value: 0.,
        ..Default::default()
    })
    .unwrap();
    let saved = d.save(&assets.path().join("binding-ui.kasane")).unwrap();
    let mut d = editor_core::Document::open(&saved.manifest).unwrap();
    d.navigate(R::new(K::Mesh, &d.layers()[0].id)).unwrap();
    let h = setup_with_document(cx, d, assets);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(h.window, |_, window, cx| {
        window.resize(size(px(1200.), px(1100.)));
        window.render_frame(cx);
    })
    .unwrap();
    h
}
fn click(h: &Harness, id: impl Into<gpui_kit::ElementId>, cx: &mut TestAppContext) {
    let id = id.into();
    cx.update_window(h.window, |_, window, cx| window.click(id, cx))
        .unwrap();
    cx.run_until_parked();
}
fn selected(h: &Harness, cx: &mut TestAppContext) -> R {
    h.view.read_with(cx, |v, _| {
        v.editor.active_inspection().unwrap().object().unwrap()
    })
}
fn run(h: &Harness, source: R, control: Control, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            let context = AuthoringContext::capture(&v.editor).unwrap();
            v.run_binding_control(context, source, control, window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
}
fn choose(h: &Harness, name: &str, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        window.within("binding-parameter").click("input", cx);
        window.input(name, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(160));
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
}
fn number(h: &Harness, value: &str, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        window.click("binding-value", cx);
        window.press("cmd-a", cx);
        window.input(value, cx);
    })
    .unwrap();
    cx.run_until_parked();
}
fn create(h: &Harness, cx: &mut TestAppContext) -> R {
    let mesh = selected(h, cx);
    click(h, format!("binding-create-{}", mesh.id), cx);
    let b = selected(h, cx);
    assert_eq!(b.kind, K::MeshBinding);
    b
}
fn add_axis(h: &Harness, b: &R, cx: &mut TestAppContext) -> R {
    run(h, b.clone(), Control::AddAxis, cx);
    let name = h.view.read_with(cx, |v, _| {
        v.editor.document().unwrap().parameters()[0].name.clone()
    });
    choose(h, &name, cx);
    click(h, "binding-review", cx);
    click(h, "binding-dialog-submit", cx);
    let axis = selected(h, cx);
    assert_eq!(axis.kind, K::BindingAxis);
    axis
}
#[gpui_kit::test]
fn properties_create_axis_samples_and_target_editing_form_a_workflow(cx: &mut TestAppContext) {
    let h = setup(cx);
    let b = create(&h, cx);
    let axis = add_axis(&h, &b, cx);
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let doc = v.editor.document().unwrap();
        assert_eq!(doc.history_lengths(), (2, 0));
        assert_eq!(doc.context().active_binding.as_ref(), Some(&b));
    });
    click(&h, format!("binding-insert_sample-{}", axis.id), cx);
    number(&h, "0.5", cx);
    click(&h, "binding-review", cx);
    click(&h, "binding-dialog-submit", cx);
    let sample = selected(&h, cx);
    assert_eq!(sample.kind, K::BindingSample);
    click(&h, format!("binding-move_sample-{}", sample.id), cx);
    number(&h, "0.4", cx);
    click(&h, "binding-review", cx);
    click(&h, "binding-dialog-submit", cx);
    h.view.read_with(cx, |v, _| {
        let info = v
            .editor
            .document()
            .unwrap()
            .ordinary_binding(&sample)
            .unwrap();
        assert!(
            info.axes[0]
                .samples
                .iter()
                .any(|s| s.id == sample.id && s.value == 0.4)
        );
    });
    run(&h, b.clone(), Control::Target, cx);
    assert_eq!(selected(&h, cx).kind, K::Mesh);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            v.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::Keyforms, None, window, cx)
        });
        window.render_frame(cx);
        assert!(window.try_find("binding-keyform-pad").is_some());
    })
    .unwrap();
}
#[gpui_kit::test]
fn reviewed_draft_edits_require_new_review_and_cancel_preserves_history(cx: &mut TestAppContext) {
    let h = setup(cx);
    let b = create(&h, cx);
    let axis = add_axis(&h, &b, cx);
    run(&h, axis, Control::InsertSample, cx);
    number(&h, "0.5", cx);
    click(&h, "binding-review", cx);
    number(&h, "0.4", cx);
    click(&h, "binding-dialog-submit", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (2, 0))
    });
    click(&h, "binding-review", cx);
    click(&h, "binding-dialog-submit", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (3, 0))
    });
    run(&h, b, Control::Delete, cx);
    cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (3, 0));
        assert_eq!(
            v.editor.document().unwrap().object_index().bindings.len(),
            1
        );
    });
}
#[gpui_kit::test]
fn binding_delete_reviews_owned_content_keeps_mesh_and_undo_restores_identity(
    cx: &mut TestAppContext,
) {
    let h = setup(cx);
    let b = create(&h, cx);
    add_axis(&h, &b, cx);
    run(&h, b.clone(), Control::Delete, cx);
    click(&h, "binding-dialog-submit", cx);
    assert_eq!(selected(&h, cx).kind, K::Mesh);
    h.view.read_with(cx, |v, _| {
        assert!(
            v.editor
                .document()
                .unwrap()
                .object_index()
                .bindings
                .is_empty()
        )
    });
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            v.run_command(editor_core::CommandId::Undo, window, cx)
        })
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(
            v.editor
                .document()
                .unwrap()
                .object_index()
                .binding(&b)
                .is_some()
        )
    });
}
#[gpui_kit::test]
fn structural_undo_invalidates_dialog_and_late_context_controls(cx: &mut TestAppContext) {
    let h = setup(cx);
    let b = create(&h, cx);
    let axis = add_axis(&h, &b, cx);
    run(&h, axis.clone(), Control::InsertSample, cx);
    number(&h, "0.5", cx);
    click(&h, "binding-review", cx);
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |v, cx| {
            v.editor.execute(editor_core::EditorCommand::Undo).unwrap();
            v.redraw(cx);
        })
    })
    .unwrap();
    cx.run_until_parked();
    click(&h, "binding-dialog-submit", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (1, 1))
    });
    click(&h, "binding-dialog-cancel", cx);
    run(&h, axis, Control::RemoveAxis, cx); // The removed axis cannot be reused.
    h.view.read_with(cx, |v, _| assert!(v.edit_error.is_some()));
}
#[gpui_kit::test]
fn pending_axis_does_not_hide_existing_keyform_pad(cx: &mut TestAppContext) {
    let (mut d, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let mesh = R::new(K::Mesh, d.selected_id().unwrap());
    let b = d.ordinary_binding(&mesh).unwrap().reference;
    let plan = d
        .plan_ordinary_binding(&A::AddAxis {
            binding: b.clone(),
            parameter: String::new(),
        })
        .unwrap();
    d.execute_ordinary_binding(&plan).unwrap();
    d.open_binding(b).unwrap();
    let h = setup_with_document(cx, d, assets);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            v.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::Keyforms, None, window, cx)
        });
        window.render_frame(cx);
        assert!(window.try_find("binding-keyform-pad").is_some());
        window.click("keyform-0", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error)
    });
}

#[gpui_kit::test]
fn high_dimensional_binding_uses_stored_form_list_and_preserves_binding_selection(
    cx: &mut TestAppContext,
) {
    let (mut d, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let mesh = R::new(K::Mesh, d.selected_id().unwrap());
    let b = d.ordinary_binding(&mesh).unwrap().reference;
    for name in ["Extra2", "Extra3"] {
        let parameter = d
            .create_parameter(editor_core::NewParameter {
                name: name.into(),
                runtime_id: name.into(),
                minimum: 0.,
                maximum: 1.,
                default_value: 0.,
                ..Default::default()
            })
            .unwrap();
        let plan = d
            .plan_ordinary_binding(&A::AddAxis {
                binding: b.clone(),
                parameter,
            })
            .unwrap();
        d.execute_ordinary_binding(&plan).unwrap();
    }
    d.open_binding(b.clone()).unwrap();
    let h = setup_with_document(cx, d, assets);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            v.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::Keyforms, None, window, cx)
        });
        window.render_frame(cx);
        assert!(window.try_find("binding-keyform-pad").is_none());
        window.click("stored-keyform-1", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        assert_eq!(d.context().active_binding.as_ref(), Some(&b));
        assert!(matches!(
            d.property_scope(&mesh),
            editor_core::PropertyScope::MeshKeyform { .. }
        ));
    });
}

#[gpui_kit::test]
fn pending_axis_can_be_configured_invalid_values_rejected_and_reduction_reviewed(
    cx: &mut TestAppContext,
) {
    let h = setup(cx);
    let b = create(&h, cx);
    run(&h, b.clone(), Control::AddAxis, cx);
    click(&h, "binding-review", cx);
    click(&h, "binding-dialog-submit", cx);
    let axis = selected(&h, cx);
    run(&h, axis.clone(), Control::Parameter, cx);
    choose(&h, "Drive", cx);
    click(&h, "binding-review", cx);
    click(&h, "binding-dialog-submit", cx);
    h.view.read_with(cx, |v, _| {
        let d = v.editor.document().unwrap();
        assert_eq!(d.history_lengths(), (3, 0));
        assert!(
            !d.ordinary_binding(&b).unwrap().axes[0]
                .parameter_id
                .is_empty()
        );
    });
    run(&h, axis.clone(), Control::InsertSample, cx);
    number(&h, "NaN", cx);
    click(&h, "binding-review", cx);
    click(&h, "binding-dialog-submit", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (3, 0))
    });
    click(&h, "binding-dialog-cancel", cx);
    run(&h, axis, Control::RemoveAxis, cx);
    number(&h, "0", cx);
    click(&h, "binding-review", cx);
    click(&h, "binding-dialog-submit", cx);
    h.view.read_with(cx, |v, _| {
        let d = v.editor.document().unwrap();
        assert!(d.ordinary_binding(&b).unwrap().axes.is_empty());
        assert_eq!(d.parameters().len(), 1);
        assert_eq!(d.history_lengths(), (4, 0));
    });
}

#[gpui_kit::test]
fn binding_creation_settles_inline_hex_text_but_cancels_unaccepted_popup_color(
    cx: &mut TestAppContext,
) {
    use editor_core::{PropertyValue, property::mesh};
    for popup in [false, true] {
        let h = setup(cx);
        let object = selected(&h, cx);
        if popup {
            super::properties::show(&h, "property-color-multiply", cx);
            click(&h, "property-color-multiply", cx);
            cx.update_window(h.window, |_, window, cx| {
                window.click("color-picker-hex", cx);
                window.press("cmd-a", cx);
                window.input("#4080C0", cx);
            })
            .unwrap();
            cx.run_until_parked();
        } else {
            super::properties::fill(&h, "property-input-multiply", "#4080C0", cx);
        }
        run(&h, object.clone(), Control::Create, cx);
        h.view.read_with(cx, |v, _| {
            assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
            let d = v.editor.document().unwrap();
            assert!(d.ordinary_binding(&object).is_some());
            assert_eq!(d.history_lengths(), (if popup { 1 } else { 2 }, 0));
            assert_eq!(
                d.property_values(&object).unwrap()[&mesh::MULTIPLY.key],
                PropertyValue::Color(if popup {
                    [1.; 3]
                } else {
                    [64. / 255., 128. / 255., 192. / 255.]
                })
            );
        });
    }
}

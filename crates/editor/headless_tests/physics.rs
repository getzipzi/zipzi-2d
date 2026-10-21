use super::{document_fixture, setup_with_document};
use editor_core::{CommandId, PanelKind};
use gpui_kit::{AppContext as _, TestAppContext, point, px, size, test::TestWindowExt};

#[gpui_kit::test]
fn adding_physics_channels_preserves_bindings_after_runtime_id_reuse(cx: &mut TestAppContext) {
    use editor_core::{ObjectKind, ObjectRef};
    let (mut document, assets) = document_fixture::combined_animation_document().unwrap();
    let original = document.physics_definition().unwrap().clone();
    let input = original.parameter_bindings["ParamX"].clone();
    let output = original.parameter_bindings["ParamY"].clone();
    document
        .set_object_property(
            &ObjectRef::new(ObjectKind::Parameter, &output),
            "runtime_id",
            serde_json::json!("RenamedY"),
        )
        .unwrap();
    document
        .set_object_property(
            &ObjectRef::new(ObjectKind::Parameter, &input),
            "runtime_id",
            serde_json::json!("ParamY"),
        )
        .unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.resize(size(px(1100.), px(1200.)));
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx);
            view.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::AnimationPhysics, None, window, cx);
        });
        window.render_frame(cx);
        window.click(("physics-group-row", 0usize), cx);
        window.render_frame(cx);
        for output_channel in [false, true] {
            if output_channel {
                window.click("physics-fold-physics-inputs", cx);
                window.render_frame(cx);
                window.click("physics-fold-physics-chain", cx);
                window.render_frame(cx);
            }
            window.click(format!("physics-channel-{output_channel}-0"), cx);
            window.render_frame(cx);
            harness.view.read_with(cx, |view, _| {
                let asset = view
                    .editor
                    .document()
                    .unwrap()
                    .physics_definition()
                    .unwrap();
                assert_eq!(asset.parameter_bindings, original.parameter_bindings);
                let rig = &asset.data.settings[0];
                assert_eq!(rig.inputs[0], original.data.settings[0].inputs[0]);
                assert_eq!(rig.outputs[0], original.data.settings[0].outputs[0]);
                assert_eq!(rig.inputs.len(), 2);
                if output_channel {
                    assert_eq!(rig.outputs.len(), 2);
                    assert_eq!(
                        asset.parameter_bindings[&rig.outputs[1].destination.id],
                        input
                    );
                } else {
                    assert_eq!(asset.parameter_bindings[&rig.inputs[1].source.id], input);
                }
            });
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn physics_list_and_properties_are_visible_together_and_numbers_commit_once_or_cancel(
    cx: &mut TestAppContext,
) {
    use super::slide_number_input::{press, release, slide};
    use gpui_kit::component::dock::{DockPlacement, PanelId};
    let (document, assets) = document_fixture::combined_animation_document().unwrap();
    let original = document.physics_definition().unwrap().data.settings[0].inputs[0].weight;
    let harness = setup_with_document(cx, document, assets);
    let origin = cx
        .update_window(harness.window, |_, window, cx| {
            window.resize(size(px(1000.), px(1200.)));
            window.activate_window();
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::AnimationWorkspace, window, cx);
                view.run_command(CommandId::ResetLayout, window, cx);
                let dock = view.dock.as_ref().unwrap();
                let tree = dock.area.read(cx).layout(DockPlacement::Center).unwrap();
                assert_ne!(
                    tree.find_panel_node(PanelId::from(dock.properties.entity_id())),
                    tree.find_panel_node(PanelId::from(
                        dock.animation_inspectors[&PanelKind::AnimationPhysics].entity_id()
                    ))
                );
                dock.show(PanelKind::AnimationPhysics, None, window, cx);
            });
            window.render_frame(cx);
            window.click(("physics-group-row", 0usize), cx);
            window.render_frame(cx);
            assert!(window.try_find("physics-number-Fps").is_none());
            assert!(window.try_find("physics-play").is_none());
            assert!(window.try_find("physics-reset-inputs").is_none());
            assert!(window.try_find("physics-stabilize").is_none());
            window.click("physics-manual", cx);
            window.render_frame(cx);
            let origin = window
                .within("physics-number-InputWeight(0)")
                .find("slide-number-input")
                .bounds()
                .center();
            press(window, origin, cx);
            origin
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin - point(px(80.), px(0.)), false, cx)
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        assert!(
            view.editor
                .document()
                .unwrap()
                .physics_definition()
                .unwrap()
                .data
                .settings[0]
                .inputs[0]
                .weight
                < original
        );
    });
    cx.update_window(harness.window, |_, window, cx| {
        release(window, origin - point(px(80.), px(0.)), cx)
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.animation().unwrap().playing);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0))
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        press(window, origin, cx);
        slide(window, origin - point(px(160.), px(0.)), false, cx);
        window.press("escape", cx);
        release(window, origin - point(px(160.), px(0.)), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.animation().unwrap().playing);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0))
    });
}

#[gpui_kit::test]
fn physics_group_actions_and_manual_controls_keep_preview_out_of_history(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::combined_animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.resize(size(px(1000.), px(1200.)));
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx);
            view.run_command(CommandId::ResetLayout, window, cx);
            view.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::AnimationPhysics, None, window, cx);
        });
        window.render_frame(cx);
        window.click(("physics-group-row", 0usize), cx);
        window.render_frame(cx);
        window.click("physics-copy-group", cx);
        window.render_frame(cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor
                    .document()
                    .unwrap()
                    .physics_definition()
                    .unwrap()
                    .data
                    .settings
                    .len(),
                2
            )
        });
        window.click("physics-delete-group", cx);
        window.render_frame(cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor
                    .document()
                    .unwrap()
                    .physics_definition()
                    .unwrap()
                    .data
                    .settings
                    .len(),
                1
            )
        });
        window.click("physics-manual", cx);
        window.render_frame(cx);
        window.click("physics-reset-inputs", cx);
        window.render_frame(cx);
        harness.view.read_with(cx, |view, _| {
            assert!(view.editor.animation().unwrap().playing)
        });
        // Move the chain into view in the short, docked inspector.
        window.click("physics-fold-physics-inputs", cx);
        window.render_frame(cx);
        let canvas = window.find("physics-chain-viewport").bounds();
        let settle = window.find("physics-stabilize").bounds();
        assert!(settle.left() >= canvas.left() && settle.right() <= canvas.right());
        assert!(settle.top() >= canvas.top() && settle.top() <= canvas.top() + px(6.));
        window.click("physics-stabilize", cx);
        window.render_frame(cx);
        window.click("physics-bypass", cx);
        window.render_frame(cx);
        harness.view.read_with(cx, |view, _| {
            let animation = view.editor.animation().unwrap();
            assert!(animation.physics_preview.manual);
            assert!(animation.physics_preview.bypass);
            assert!(animation.physics_preview.stabilized);
            assert!(animation.playing);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
        });
        window.click("physics-manual", cx);
        window.render_frame(cx);
        harness.view.read_with(cx, |view, _| {
            assert!(!view.editor.animation().unwrap().physics_preview.manual)
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn physics_table_cells_keep_their_mapping_and_paired_fields_stay_inline(cx: &mut TestAppContext) {
    use super::slide_number_input::{press, release, slide};
    let (document, assets) = document_fixture::combined_animation_document().unwrap();
    let first = document.physics_definition().unwrap().data.settings[0].inputs[0].weight;
    let harness = setup_with_document(cx, document, assets);
    let origin = cx
        .update_window(harness.window, |_, window, cx| {
            window.resize(size(px(1100.), px(1200.)));
            window.activate_window();
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::AnimationWorkspace, window, cx);
                view.dock
                    .as_ref()
                    .unwrap()
                    .show(PanelKind::AnimationPhysics, None, window, cx);
            });
            window.render_frame(cx);
            window.click(("physics-group-row", 0usize), cx);
            window.render_frame(cx);
            window.click("physics-channel-false-0", cx);
            window.render_frame(cx);
            assert!(window.try_find("physics-input-table").is_some());
            assert!(window.try_find("physics-output-table").is_some());
            assert!(window.try_find("physics-fit-chain").is_none());
            let limit =
                crate::ui::compact::scaled(crate::ui::compact::HEADER_CONTROL, cx) + px(0.5);
            for button in [
                "physics-manual",
                "physics-channel-false-0",
                "physics-add-node",
            ] {
                assert!(
                    window.find(button).bounds().size.height <= limit,
                    "{button} is not compact"
                );
            }
            let graph = window.find("physics-chain-viewport").bounds().size;
            assert!(graph.height > px(0.));
            let settings = window.find("physics-pendulum-settings").bounds();
            let viewport = window.find("physics-chain-viewport").bounds();
            assert!((viewport.top() - settings.top()).abs() <= px(1.));
            assert!((viewport.bottom() - settings.bottom()).abs() <= px(1.));
            let reuse_label = window.find("physics-reuse-label").bounds();
            let reuse_control = window.find("physics-select-Reuse").bounds();
            assert!(reuse_label.bottom() <= reuse_control.top());
            assert!(
                window.find("physics-number-Vertex(1, 0)").bounds().right()
                    <= window.find("physics-chain-viewport").bounds().left()
            );
            let label = window.find("property-label-Length").bounds();
            let number = window
                .within("physics-number-Vertex(1, 0)")
                .find("slide-number-input")
                .bounds();
            assert!(label.right() <= number.left() + px(1.));
            assert!((label.center().y - number.center().y).abs() <= px(1.));
            let origin = window
                .within("physics-number-InputWeight(1)")
                .find("slide-number-input")
                .bounds()
                .center();
            press(window, origin, cx);
            origin
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin - point(px(40.), px(0.)), false, cx);
        release(window, origin - point(px(40.), px(0.)), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let document = view.editor.document().unwrap();
        let rig = &document.physics_definition().unwrap().data.settings[0];
        assert_eq!(rig.inputs[0].weight, first);
        assert!(rig.inputs[1].weight < 100.);
        assert_eq!(document.history_lengths(), (2, 0));
    });
}

#[gpui_kit::test]
fn manual_input_drag_updates_before_release_and_uses_one_decimal_display(cx: &mut TestAppContext) {
    use super::slide_number_input::{press, release, slide};
    let (document, assets) = document_fixture::combined_animation_document().unwrap();
    let asset = document.physics_definition().unwrap();
    let parameter = asset.parameter_bindings[&asset.data.settings[0].inputs[0].source.id].clone();
    let harness = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(harness.window, size(px(1200.), px(1000.)));
    let control = format!("physics-number-Manual({parameter:?})");
    let origin = cx
        .update_window(harness.window, |_, window, cx| {
            window.activate_window();
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::AnimationWorkspace, window, cx);
                view.run_command(CommandId::ResetLayout, window, cx);
                let id = view.editor.active_id().unwrap();
                let motion = view.editor.document().unwrap().animation_catalog().motions[0]
                    .selection
                    .clone();
                view.editor.select_motion(id, motion).unwrap();
                view.editor.set_physics_manual(id, true).unwrap();
                view.editor
                    .set_physics_input(id, &parameter, 0.723)
                    .unwrap();
                view.dock
                    .as_ref()
                    .unwrap()
                    .show(PanelKind::AnimationPhysics, None, window, cx);
            });
            window.render_frame(cx);
            window.click(("physics-group-row", 0usize), cx);
            window.render_frame(cx);
            let origin = window
                .within(control.clone())
                .find("slide-number-input")
                .bounds()
                .center();
            let text = window
                .within(control.clone())
                .find_by_label(crate::t!("physics.test_input").as_ref())
                .value()
                .map(str::to_owned);
            assert_eq!(text.as_deref(), Some("0.7"));
            press(window, origin, cx);
            origin
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin - point(px(20.), px(0.)), false, cx);
    })
    .unwrap();
    let first = harness.view.read_with(cx, |view, _| {
        view.editor
            .animation()
            .unwrap()
            .physics_preview
            .inputs
            .last()
            .unwrap()
            .value
    });
    assert!(first < 0.723, "input must update before pointer release");
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        slide(window, origin - point(px(40.), px(0.)), false, cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(
            view.editor
                .animation()
                .unwrap()
                .physics_preview
                .inputs
                .last()
                .unwrap()
                .value
                < first
        );
        assert!(view.editor.animation().unwrap().playing);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        release(window, origin - point(px(40.), px(0.)), cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.press("space", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let animation = view.editor.animation().unwrap();
        assert!(!animation.physics_preview.manual);
        assert!(animation.playing);
    });
}

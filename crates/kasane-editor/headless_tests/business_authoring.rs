use super::{
    Harness, document_fixture,
    properties::{fill, show},
    setup_with_document,
};
use crate::authoring::AuthoringContext;
use editor_core::{BusinessAction as A, CommandId, ObjectKind as K, ObjectRef as R, PanelKind};
use gpui_kit::{AppContext as _, TestAppContext, px, size, test::TestWindowExt};
fn setup(cx: &mut TestAppContext, combined: bool) -> Harness {
    let (document, assets) = if combined {
        document_fixture::combined_animation_document().unwrap()
    } else {
        document_fixture::animation_document().unwrap()
    };
    let h = setup_with_document(cx, document, assets);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(h.window, |_, window, cx| {
        window.resize(size(px(1200.), px(1500.)));
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        h.view.update(cx, |v, cx| {
            v.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::AnimationPose, None, window, cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    h
}
fn action(h: &Harness, action: A, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            let context = AuthoringContext::capture(&v.editor).unwrap();
            v.run_business_action(context, action, window, cx);
        })
    })
    .unwrap();
    cx.run_until_parked();
}
fn click(h: &Harness, id: impl Into<gpui_kit::ElementId>, cx: &mut TestAppContext) {
    let id = id.into();
    cx.update_window(h.window, |_, window, cx| window.click(id, cx))
        .unwrap();
    cx.run_until_parked();
}
fn choose(h: &Harness, id: &str, text: &str, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        window.within(id.to_owned()).click("input", cx);
        window.input(text, cx);
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
#[gpui_kit::test]
fn pose_toolbar_creates_singleton_empty_group_members_and_links_with_undo(cx: &mut TestAppContext) {
    let h = setup(cx, false);
    click(&h, "pose-new-group", cx);
    let root = h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let p = v.editor.document().unwrap().pose_definition().unwrap();
        assert_eq!(p.groups.len(), 1);
        assert!(p.groups[0].entries.is_empty());
        assert_eq!(
            v.editor.active_inspection().unwrap().object(),
            Some(R::new(K::PoseGroup, &p.groups[0].id))
        );
        p.id.clone()
    });
    click(&h, "pose-new-entry", cx);
    let entry = h.view.read_with(cx, |v, _| {
        v.editor.active_inspection().unwrap().object().unwrap()
    });
    assert_eq!(entry.kind, K::PoseEntry);
    cx.update_window(h.window, |_, window, _| {
        assert!(window.try_find("property-select-target").is_some())
    })
    .unwrap();
    click(&h, "pose-new-link", cx);
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        assert_eq!(d.history_lengths(), (3, 0));
        let p = d.pose_definition().unwrap();
        assert_eq!(p.id, root);
        assert_eq!(p.groups[0].entries[0].id, entry.id);
        assert_eq!(p.groups[0].entries[0].links.as_ref().unwrap().len(), 1);
        assert_eq!(
            v.editor.active_inspection().unwrap().object().unwrap().kind,
            K::PoseLink
        );
    });
    cx.update_window(h.window, |_, window, cx| {
        h.view
            .update(cx, |v, cx| v.run_command(CommandId::Undo, window, cx))
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(
            v.editor
                .document()
                .unwrap()
                .pose_definition()
                .unwrap()
                .groups[0]
                .entries[0]
                .links
                .as_ref()
                .is_none_or(|ls| ls.is_empty())
        )
    });
}
#[gpui_kit::test]
fn pose_name_root_fade_and_member_part_use_ordinary_properties(cx: &mut TestAppContext) {
    let h = setup(cx, false);
    click(&h, "pose-new-group", cx);
    fill(&h, "property-input-name", "Blink", cx);
    cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    click(&h, "pose-new-entry", cx);
    let (part, name) = h.view.read_with(cx, |v, _| {
        let index = v.editor.document().unwrap().object_index();
        let p = index
            .objects
            .iter()
            .find(|o| o.reference.kind == K::Part)
            .unwrap();
        (p.reference.id.clone(), p.name.clone())
    });
    choose(&h, "property-select-target", &name, cx);
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let p = v.editor.document().unwrap().pose_definition().unwrap();
        assert_eq!(p.groups[0].name, "Blink");
        assert_eq!(
            p.groups[0].entries[0].part,
            kasane_core::document::PosePartRef::Resolved { part_id: part }
        );
    });
    let root = h.view.read_with(cx, |v, _| {
        v.editor
            .document()
            .unwrap()
            .pose_definition()
            .unwrap()
            .id
            .clone()
    });
    click(&h, format!("pose-row-{root}"), cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(
            v.editor.active_inspection().unwrap().object(),
            Some(R::new(K::Pose, &root)),
            "{:?}",
            v.edit_error
        )
    });
    show(&h, "property-custom-fade_in", cx);
    click(&h, "property-custom-fade_in", cx);
    fill(&h, "property-input-fade_in", "0.7", cx);
    cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        assert_eq!(
            v.editor
                .document()
                .unwrap()
                .pose_definition()
                .unwrap()
                .fade_in,
            Some(0.7)
        );
    });
}
#[gpui_kit::test]
fn pose_list_selection_collapsing_and_keyboard_preserve_motion_preview(cx: &mut TestAppContext) {
    let h = setup(cx, true);
    let (group, entry) = h.view.read_with(cx, |v, _| {
        let p = v.editor.document().unwrap().pose_definition().unwrap();
        (p.groups[0].id.clone(), p.groups[0].entries[0].id.clone())
    });
    cx.update_window(h.window, |_, window, cx| {
        window.double_click(("animation-motion", 0usize), cx);
        h.view
            .update(cx, |v, _| v.editor.advance_animation_clock(0.4));
    })
    .unwrap();
    click(&h, format!("pose-row-{entry}"), cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(
            v.editor.active_inspection().unwrap().object(),
            Some(R::new(K::PoseEntry, &entry))
        );
        assert!(v.editor.animation().unwrap().playing);
        assert_eq!(v.editor.animation().unwrap().time, 0.4);
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0));
    });
    click(&h, format!("pose-fold-{group}"), cx);
    cx.update_window(h.window, |_, window, _| {
        assert!(window.try_find(format!("pose-row-{entry}")).is_none())
    })
    .unwrap();
    click(&h, format!("pose-row-{group}"), cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("right", cx);
        window.render_frame(cx);
        window.press("down", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert_eq!(
            v.editor.active_inspection().unwrap().object(),
            Some(R::new(K::PoseEntry, &entry))
        );
        assert!(v.editor.animation().unwrap().playing);
    });
}
#[gpui_kit::test]
fn pose_migration_picker_shows_effects_and_moves_members_by_identity(cx: &mut TestAppContext) {
    let h = setup(cx, true);
    let (group, entries) = h.view.read_with(cx, |v, _| {
        let g = v
            .editor
            .document()
            .unwrap()
            .pose_definition()
            .unwrap()
            .groups[0]
            .clone();
        (R::new(K::PoseGroup, &g.id), g.entries)
    });
    action(&h, A::Create(K::PoseGroup, None), cx);
    let destination = h.view.read_with(cx, |v, _| {
        v.editor.active_inspection().unwrap().object().unwrap()
    });
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            let context = AuthoringContext::capture(&v.editor).unwrap();
            v.open_business_destination(context, group.clone(), true, window, cx);
        })
    })
    .unwrap();
    cx.run_until_parked();
    choose(&h, "business-destination", &destination.id, cx);
    cx.update_window(h.window, |_, window, cx| {
        assert!(
            window
                .find(("business-effect", 0usize))
                .label()
                .unwrap()
                .contains("deleted")
        );
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (1, 0)
        );
        window.click("business-dialog-submit", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        assert_eq!(d.pose_definition().unwrap().groups.len(), 1);
        assert_eq!(d.pose_definition().unwrap().groups[0].entries, entries);
        assert_eq!(d.history_lengths(), (2, 0));
        assert_eq!(
            v.editor.active_inspection().unwrap().object(),
            Some(R::new(K::PoseEntry, &entries[0].id))
        );
    });
}
#[gpui_kit::test]
fn business_delete_review_cancels_by_default_and_rejects_stale_history(cx: &mut TestAppContext) {
    let h = setup(cx, true);
    let group = h.view.read_with(cx, |v, _| {
        R::new(
            K::PoseGroup,
            &v.editor
                .document()
                .unwrap()
                .pose_definition()
                .unwrap()
                .groups[0]
                .id,
        )
    });
    action(&h, A::Delete(group.clone()), cx);
    cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0))
    });
    action(&h, A::Create(K::PoseGroup, None), cx);
    action(&h, A::Delete(group.clone()), cx);
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |v, cx| {
            v.editor.execute(editor_core::EditorCommand::Undo).unwrap();
            cx.notify();
        })
    })
    .unwrap();
    cx.run_until_parked();
    click(&h, "business-dialog-submit", cx);
    h.view.read_with(cx, |v, _| {
        assert!(
            v.editor
                .document()
                .unwrap()
                .object_index()
                .get(&group)
                .is_some()
        );
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 1));
    });
}
#[gpui_kit::test]
fn physics_child_inspection_tracks_same_input_after_native_reorder(cx: &mut TestAppContext) {
    let h = setup(cx, true);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            v.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::AnimationPhysics, None, window, cx)
        });
        window.render_frame(cx);
        window.click(("physics-group-row", 0usize), cx);
        window.render_frame(cx);
        window.click("physics-channel-false-0", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let selected = h.view.read_with(cx, |v, _| {
        let d = v.editor.document().unwrap();
        R::new(
            K::PhysicsInput,
            &d.physics_definition().unwrap().data.settings[0].inputs[0].id,
        )
    });
    click(&h, "physics-input-row-0", cx);
    action(&h, A::Step(selected.clone(), true), cx);
    cx.update_window(h.window, |_, window, cx| {
        assert_eq!(window.find("physics-input-row-1").selected(), Some(true));
        h.view
            .update(cx, |v, cx| v.run_command(CommandId::Undo, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert_eq!(
            v.editor.active_inspection().unwrap().object(),
            Some(selected)
        )
    });
    cx.update_window(h.window, |_, window, _| {
        assert_eq!(window.find("physics-input-row-0").selected(), Some(true))
    })
    .unwrap();
}
#[gpui_kit::test]
fn business_creation_commands_share_capabilities_and_root_physics_settings(
    cx: &mut TestAppContext,
) {
    let h = setup(cx, false);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            assert!(
                !v.editor
                    .command_available(v.editor.command(CommandId::CreatePoseEntry))
            );
            v.run_command(CommandId::CreatePhysicsGroup, window, cx);
        })
    })
    .unwrap();
    cx.run_until_parked();
    click(&h, "physics-inspect-root", cx);
    fill(&h, "property-input-fps", "48", cx);
    cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        assert_eq!(d.physics_definition().unwrap().data.meta.fps, Some(48.));
        assert_eq!(d.history_lengths(), (2, 0));
        assert_eq!(
            v.editor.active_inspection().unwrap().object().unwrap().kind,
            K::Physics
        );
    });
}

#[gpui_kit::test]
fn pose_group_right_click_copy_captures_source_and_shortcut_reorders(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, test::ClickOptions};
    let h = setup(cx, true);
    let original = h.view.read_with(cx, |v, _| {
        v.editor
            .document()
            .unwrap()
            .pose_definition()
            .unwrap()
            .groups[0]
            .clone()
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(
            format!("pose-row-{}", original.id),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        )
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        let copy = (0..16usize)
            .find(|i| {
                window
                    .within("popup-menu")
                    .try_find(*i)
                    .is_some_and(|e| e.label() == Some("Duplicate"))
            })
            .unwrap();
        window.within("popup-menu").click(copy, cx);
    })
    .unwrap();
    cx.run_until_parked();
    let copy = h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let p = v.editor.document().unwrap().pose_definition().unwrap();
        assert_eq!(p.groups.len(), 2);
        assert_ne!(p.groups[1].entries[0].id, original.entries[0].id);
        assert_eq!(p.groups[1].entries[0].part, original.entries[0].part);
        p.groups[1].id.clone()
    });
    click(&h, format!("pose-row-{copy}"), cx);
    cx.update_window(h.window, |_, window, cx| window.press("alt-up", cx))
        .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        assert_eq!(
            v.editor
                .document()
                .unwrap()
                .pose_definition()
                .unwrap()
                .groups[0]
                .id,
            copy
        );
        assert_eq!(v.editor.document().unwrap().history_lengths(), (2, 0));
    });
}
#[gpui_kit::test]
fn output_destination_picker_requires_explicit_node_and_never_chooses_by_position(
    cx: &mut TestAppContext,
) {
    let h = setup(cx, true);
    let output = h.view.read_with(cx, |v, _| {
        R::new(
            K::PhysicsOutput,
            &v.editor
                .document()
                .unwrap()
                .physics_definition()
                .unwrap()
                .data
                .settings[0]
                .outputs[0]
                .id,
        )
    });
    action(&h, A::Create(K::PhysicsGroup, None), cx);
    let destination = h.view.read_with(cx, |v, _| {
        v.editor.active_inspection().unwrap().object().unwrap()
    });
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            let context = AuthoringContext::capture(&v.editor).unwrap();
            v.open_business_destination(context, output.clone(), false, window, cx);
        })
    })
    .unwrap();
    cx.run_until_parked();
    choose(&h, "business-destination", &destination.id, cx);
    click(&h, "business-dialog-submit", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (1, 0))
    });
    cx.update_window(h.window, |_, window, _| {
        assert!(
            window
                .find("business-dialog-reason")
                .label()
                .unwrap()
                .contains("sample node")
        )
    })
    .unwrap();
    choose(&h, "business-output-node", "1", cx);
    click(&h, "business-dialog-submit", cx);
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        let g = &d.physics_definition().unwrap().data.settings[1];
        assert_eq!(g.outputs[0].id, output.id);
        assert_eq!(g.outputs[0].node_id, g.vertices[1].id);
        assert_eq!(d.history_lengths(), (2, 0));
        assert_eq!(v.editor.active_inspection().unwrap().object(), Some(output));
    });
}
#[gpui_kit::test]
fn dedicated_node_copy_preserves_reference_and_root_inspection_stays_on_root(
    cx: &mut TestAppContext,
) {
    let h = setup(cx, true);
    let root = h.view.read_with(cx, |v, _| {
        R::new(
            K::PhysicsNode,
            &v.editor
                .document()
                .unwrap()
                .physics_definition()
                .unwrap()
                .data
                .settings[0]
                .vertices[0]
                .id,
        )
    });
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            v.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::AnimationPhysics, None, window, cx)
        });
        window.render_frame(cx);
        window.click(("physics-group-row", 0usize), cx);
    })
    .unwrap();
    cx.run_until_parked();
    let count = h.view.read_with(cx, |v, _| {
        v.editor
            .document()
            .unwrap()
            .physics_definition()
            .unwrap()
            .data
            .settings[0]
            .vertices
            .len()
    });
    click(&h, "physics-fold-physics-inputs", cx);
    click(&h, "physics-node-copy", cx);
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        let g = &d.physics_definition().unwrap().data.settings[0];
        assert_eq!(g.vertices.len(), count + 1);
        assert_ne!(g.vertices[1].id, g.vertices[2].id);
        assert_eq!(g.outputs[0].node_id, g.vertices[1].id);
        assert_eq!(
            v.editor.active_inspection().unwrap().object(),
            Some(R::new(K::PhysicsNode, &g.vertices[2].id))
        );
    });
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            let result = v.inspect_authoring_object(
                v.editor.active_id().unwrap(),
                root.clone(),
                false,
                window,
                cx,
            );
            v.complete_edit(result, cx);
        })
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.active_inspection().unwrap().object(), Some(root))
    });
    cx.update_window(h.window, |_, window, _| {
        assert!(window.try_find("physics-delete-node").is_none())
    })
    .unwrap();
}

#[gpui_kit::test]
fn physics_child_inspection_receives_presented_frame_notifications(cx: &mut TestAppContext) {
    let h = setup(cx, true);
    let children = h.view.read_with(cx, |v, _| {
        let rig = &v
            .editor
            .document()
            .unwrap()
            .physics_definition()
            .unwrap()
            .data
            .settings[0];
        [
            R::new(K::PhysicsInput, &rig.inputs[0].id),
            R::new(K::PhysicsOutput, &rig.outputs[0].id),
            R::new(K::PhysicsNode, &rig.vertices[1].id),
        ]
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click(("animation-motion", 0usize), cx);
        h.view.update(cx, |v, cx| {
            v.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::AnimationPhysics, None, window, cx);
        });
        window.render_frame(cx);
        window.click(("physics-group-row", 0usize), cx);
    })
    .unwrap();
    let panel = h
        .view
        .read_with(cx, |v, _| v.dock.as_ref().unwrap().properties.clone());
    let notifications = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = notifications.clone();
    let _observe = cx.update(|cx| cx.observe(&panel, move |_, _| observed.set(observed.get() + 1)));
    for (i, child) in children.into_iter().enumerate() {
        h.view.update(cx, |v, cx| {
            v.editor
                .inspect_authoring_object(v.editor.active_id().unwrap(), child.clone())
                .unwrap();
            v.redraw(cx);
        });
        cx.run_until_parked();
        cx.update_window(h.window, |_, window, cx| window.render_frame(cx))
            .unwrap();
        cx.run_until_parked();
        notifications.set(0);
        let value = h.view.update(cx, |v, cx| {
            let id = v.editor.active_id().unwrap();
            v.editor.seek_animation(id, (i + 1) as f32 * 0.25).unwrap();
            v.editor
                .set_animation_preview_mode(id, editor_core::AnimationPreviewMode::Result)
                .unwrap();
            let request = v.editor.animation_request().unwrap();
            let frame = editor_core::AnimationEvaluator::default()
                .evaluate(&request, |_, _| true)
                .unwrap();
            let value = frame.snapshot.physics[0].inputs[0];
            assert!(v.editor.present_animation(frame.identity, frame.snapshot));
            v.notify_animation(cx);
            value
        });
        cx.run_until_parked();
        assert!(
            notifications.get() > 0,
            "missing frame notification for {child:?}"
        );
        cx.update_window(h.window, |_, window, cx| {
            // render_frame() forces every view to refresh and would hide missing notifications.
            window.draw(cx).clear(cx);
            assert_eq!(
                window.find("animation-physics-input-0-0").label(),
                Some(crate::ui::slide_number_input::format_one_decimal(value as f64).as_str())
            );
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn deleting_last_physics_channel_keeps_parent_available_for_creation(cx: &mut TestAppContext) {
    for output in [false, true] {
        let h = setup(cx, true);
        let (group, child) = h.view.read_with(cx, |v, _| {
            let rig = &v
                .editor
                .document()
                .unwrap()
                .physics_definition()
                .unwrap()
                .data
                .settings[0];
            (
                R::new(K::PhysicsGroup, &rig.id),
                if output {
                    R::new(K::PhysicsOutput, &rig.outputs[0].id)
                } else {
                    R::new(K::PhysicsInput, &rig.inputs[0].id)
                },
            )
        });
        cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |v, cx| {
                v.dock
                    .as_ref()
                    .unwrap()
                    .show(PanelKind::AnimationPhysics, None, window, cx);
            });
            window.render_frame(cx);
            window.click(("physics-group-row", 0usize), cx);
            window.render_frame(cx);
            if output {
                window.click("physics-fold-physics-inputs", cx);
                window.render_frame(cx);
                window.click("physics-fold-physics-chain", cx);
            }
            h.view.update(cx, |v, cx| {
                v.editor
                    .inspect_authoring_object(v.editor.active_id().unwrap(), child.clone())
                    .unwrap();
                v.redraw(cx);
            });
        })
        .unwrap();
        cx.run_until_parked();
        click(&h, format!("physics-channel-{output}-1"), cx);
        h.view.read_with(cx, |v, _| {
            let d = v.editor.document().unwrap();
            let rig = &d.physics_definition().unwrap().data.settings[0];
            assert_eq!(
                if output {
                    rig.outputs.len()
                } else {
                    rig.inputs.len()
                },
                0
            );
            assert_eq!(
                v.editor.active_inspection().unwrap().object(),
                Some(group.clone())
            );
            assert_eq!(d.history_lengths(), (1, 0));
        });
        cx.update_window(h.window, |_, window, _| {
            assert!(window.try_find("physics-properties").is_some());
        })
        .unwrap();
        click(&h, format!("physics-channel-{output}-0"), cx);
        h.view.read_with(cx, |v, _| {
            let d = v.editor.document().unwrap();
            let rig = &d.physics_definition().unwrap().data.settings[0];
            assert_eq!(
                if output {
                    rig.outputs.len()
                } else {
                    rig.inputs.len()
                },
                1
            );
            assert_eq!(
                v.editor.active_inspection().unwrap().object().unwrap().kind,
                child.kind
            );
            assert_eq!(d.history_lengths(), (2, 0));
            assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        });
    }
}

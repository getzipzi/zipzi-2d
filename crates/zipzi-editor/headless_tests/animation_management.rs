use super::{Harness, document_fixture, setup_with_document};
use crate::authoring::{AuthoringContext, animation::AnimationAction as A};
use gpui_kit::{
    AppContext as _, MouseButton, TestAppContext, px,
    test::{ClickOptions, TestWindowExt},
};
use zipzi_editor_core::{
    AnimationRemoval as D, CommandId, MotionCopyKind as Copy, ObjectKind as K, ObjectRef as R,
};
fn setup(cx: &mut TestAppContext) -> Harness {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(h.window, |_, window, cx| {
        window.resize(gpui_kit::size(px(1200.), px(1500.)));
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.double_click(("animation-motion", 0usize), cx);
        h.view
            .update(cx, |v, _| v.editor.advance_animation_clock(0.4));
    })
    .unwrap();
    h
}
fn action(h: &Harness, action: A, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            let context = AuthoringContext::capture(&v.editor).unwrap();
            v.run_animation_action(context, action, window, cx);
        })
    })
    .unwrap();
    cx.run_until_parked();
}
fn menu(h: &Harness, label: &str, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        let index = (0..24usize)
            .find(|i| {
                window
                    .within("popup-menu")
                    .try_find(*i)
                    .is_some_and(|e| e.label() == Some(label))
            })
            .expect(label);
        assert_ne!(
            window.within("popup-menu").find(index).disabled(),
            Some(true)
        );
        window.within("popup-menu").click(index, cx);
    })
    .unwrap();
    cx.run_until_parked();
}
fn choose_motion(h: &Harness, name: &str, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        window.within("motion-reference-select").click("input", cx);
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
#[gpui_kit::test]
fn right_click_content_copy_inspects_new_content_without_activating_it(cx: &mut TestAppContext) {
    let h = setup(cx);
    let selected = h.view.read_with(cx, |v, _| {
        v.editor.animation().unwrap().motion.clone().unwrap()
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(
            ("animation-motion", 1usize),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        )
    })
    .unwrap();
    cx.run_until_parked();
    menu(&h, "Copy Motion Content", cx);
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let doc = v.editor.document().unwrap();
        assert_eq!(doc.history_lengths(), (1, 0));
        let copied = &doc.motion_groups()[0].entries[2];
        assert_ne!(copied.clip_id, selected.clip_id);
        assert_eq!(
            v.editor.active_inspection().unwrap().object(),
            Some(R::new(K::Motion, &copied.clip_id))
        );
        assert_eq!(
            v.editor.animation().unwrap().motion.as_ref(),
            Some(&selected)
        );
        assert!(v.editor.animation().unwrap().playing);
        assert_eq!(v.editor.animation().unwrap().time, 0.4);
    });
}
#[gpui_kit::test]
fn reference_picker_inserts_after_captured_registration_and_inherits_without_preview(
    cx: &mut TestAppContext,
) {
    let h = setup(cx);
    let (selected, group) = h.view.read_with(cx, |v, _| {
        (
            v.editor.animation().unwrap().motion.clone().unwrap(),
            v.editor.document().unwrap().motion_groups()[0].clone(),
        )
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(
            ("animation-motion", 1usize),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        )
    })
    .unwrap();
    cx.run_until_parked();
    menu(&h, "Add Existing Motion Reference…", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0));
        assert!(v.editor.animation().unwrap().playing);
    });
    choose_motion(&h, "Loop", cx);
    cx.update_window(h.window, |_, window, cx| {
        assert_ne!(
            window.find("motion-reference-submit").disabled(),
            Some(true)
        );
        window.click("motion-reference-submit", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let doc = v.editor.document().unwrap();
        assert_eq!(doc.history_lengths(), (1, 0));
        assert_eq!(doc.motion_groups()[0].entries[..2], group.entries);
        let entry = &doc.motion_groups()[0].entries[2];
        assert_eq!(doc.motion_definition(&entry.clip_id).unwrap().name, "Loop");
        assert_eq!(
            (entry.fade_in, entry.fade_out, entry.sound.as_ref()),
            (None, None, None)
        );
        assert_eq!(
            v.editor.active_inspection().unwrap().object(),
            Some(R::new(K::MotionRegistration, &entry.id))
        );
        assert_eq!(
            v.editor.animation().unwrap().motion.as_ref(),
            Some(&selected)
        );
        assert!(v.editor.animation().unwrap().playing);
        assert_eq!(v.editor.animation().unwrap().time, 0.4);
    });
}
#[gpui_kit::test]
fn group_copy_and_empty_group_reference_keep_shared_content_and_playback(cx: &mut TestAppContext) {
    let h = setup(cx);
    let source = h.view.read_with(cx, |v, _| {
        R::new(
            K::MotionGroup,
            &v.editor.document().unwrap().motion_groups()[0].id,
        )
    });
    action(&h, A::CopyMotion(source, Copy::Group), cx);
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let groups = v.editor.document().unwrap().motion_groups();
        assert_eq!(groups.len(), 2);
        assert_ne!(groups[0].entries[0].id, groups[1].entries[0].id);
        assert_eq!(groups[0].entries[0].clip_id, groups[1].entries[0].clip_id);
        assert_eq!(groups[0].entries[1].fade_in, groups[1].entries[1].fade_in);
        assert!(v.editor.animation().unwrap().playing);
    });
    action(&h, A::Create(K::MotionGroup, None), cx);
    let group = h.view.read_with(cx, |v, _| {
        v.editor.active_inspection().unwrap().object().unwrap()
    });
    action(&h, A::AddMotionReference(Some(group.clone())), cx);
    choose_motion(&h, "Once", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("motion-reference-submit", cx)
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let doc = v.editor.document().unwrap();
        assert_eq!(
            doc.motion_groups()
                .iter()
                .find(|g| g.id == group.id)
                .unwrap()
                .entries
                .len(),
            1
        );
        assert_eq!(doc.history_lengths(), (3, 0));
        assert!(v.editor.animation().unwrap().playing);
        assert_eq!(v.editor.animation().unwrap().time, 0.4);
    });
}
#[gpui_kit::test]
fn deletion_review_defaults_to_cancel_and_executes_the_reported_migration(cx: &mut TestAppContext) {
    let h = setup(cx);
    let group = h.view.read_with(cx, |v, _| {
        v.editor.document().unwrap().motion_groups()[0].clone()
    });
    let source = R::new(K::MotionGroup, &group.id);
    action(&h, A::DeleteAnimation(source.clone(), D::Group), cx);
    cx.update_window(h.window, |_, window, cx| {
        assert!(window.try_find("animation-deletion-review").is_some());
        assert!(h.view.read(cx).editor.animation().unwrap().playing);
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (0, 0)
        );
        assert!(
            window
                .find(("animation-delete-effect", 0usize))
                .label()
                .unwrap()
                .contains("Will be deleted")
        );
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().motion_groups()[0], group)
    });
    action(&h, A::DeleteAnimation(source, D::Group), cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("animation-delete-submit", cx)
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let doc = v.editor.document().unwrap();
        assert!(doc.motion_groups().is_empty());
        assert_eq!(doc.history_lengths(), (1, 0));
        let remaining: Vec<_> = group
            .entries
            .iter()
            .filter(|e| doc.motion_registration(&e.id).is_some())
            .collect();
        assert_eq!(remaining.len(), 1);
        assert_eq!(
            doc.motion_registration(&remaining[0].id).unwrap().1,
            remaining[0]
        );
        assert!(doc.motion_definition(&remaining[0].clip_id).is_some());
        // The surviving last registration may differ from the previewed one. Never auto-activate it.
        let a = v.editor.animation().unwrap();
        if a.motion.is_some() {
            assert_eq!(
                a.motion.as_ref().unwrap().entry.as_ref(),
                Some(&remaining[0].id)
            );
        } else {
            assert!(!a.playing);
        }
    });
    cx.update_window(h.window, |_, window, cx| {
        h.view
            .update(cx, |v, cx| v.run_command(CommandId::Undo, window, cx))
    })
    .unwrap();
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().motion_groups()[0], group)
    });
}
#[gpui_kit::test]
fn deleting_shared_clip_reviews_all_usages_cleans_preview_and_does_not_autoplay_on_undo(
    cx: &mut TestAppContext,
) {
    let h = setup(cx);
    let clip = h.view.read_with(cx, |v, _| {
        R::new(
            K::Motion,
            &v.editor
                .animation()
                .unwrap()
                .motion
                .as_ref()
                .unwrap()
                .clip_id,
        )
    });
    action(&h, A::DeleteAnimation(clip.clone(), D::Motion), cx);
    cx.update_window(h.window, |_, window, cx| {
        let mut lines = String::new();
        for i in 0..20usize {
            if let Some(row) = window.try_find(("animation-delete-effect", i)) {
                lines.push_str(&row.label().unwrap_or_default());
            }
        }
        assert!(lines.contains("Idle [1]"), "{lines}");
        assert!(lines.contains("Idle [2]"), "{lines}");
        assert!(lines.contains("Motion track"), "{lines}");
        assert!(h.view.read(cx).editor.animation().unwrap().playing);
        window.click("animation-delete-submit", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let doc = v.editor.document().unwrap();
        assert!(doc.motion_definition(&clip.id).is_none());
        assert_eq!(doc.history_lengths(), (1, 0));
        assert!(v.editor.animation().unwrap().motion.is_none());
        assert!(!v.editor.animation().unwrap().playing);
    });
    cx.update_window(h.window, |_, window, cx| {
        h.view
            .update(cx, |v, cx| v.run_command(CommandId::Undo, window, cx))
    })
    .unwrap();
    h.view.read_with(cx, |v, _| {
        assert!(
            v.editor
                .document()
                .unwrap()
                .motion_definition(&clip.id)
                .is_some()
        );
        assert!(v.editor.animation().unwrap().motion.is_none());
        assert!(!v.editor.animation().unwrap().playing);
    });
}
#[gpui_kit::test]
fn last_registration_is_explained_and_stale_review_cannot_consume_redo(cx: &mut TestAppContext) {
    let h = setup(cx);
    let root = h.view.read_with(cx, |v, _| {
        R::new(
            K::MotionRegistration,
            v.editor
                .document()
                .unwrap()
                .motion_resources()
                .iter()
                .find(|r| r.name == "Loop")
                .unwrap()
                .selection
                .entry
                .as_ref()
                .unwrap(),
        )
    });
    action(&h, A::DeleteAnimation(root, D::Registration), cx);
    cx.update_window(h.window, |_, window, cx| {
        assert!(
            window
                .find("animation-delete-reason")
                .label()
                .unwrap()
                .contains("last registration")
        );
        window.click("animation-delete-submit", cx);
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (0, 0)
        );
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    action(&h, A::Create(K::MotionGroup, None), cx);
    let group = h.view.read_with(cx, |v, _| {
        R::new(
            K::MotionGroup,
            &v.editor.document().unwrap().motion_groups()[0].id,
        )
    });
    action(&h, A::DeleteAnimation(group, D::DissolveGroup), cx);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            v.editor
                .execute(zipzi_editor_core::EditorCommand::Undo)
                .unwrap();
            cx.notify();
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        assert!(
            window
                .find("animation-delete-reason")
                .label()
                .unwrap()
                .contains("context changed")
        );
        window.click("animation-delete-submit", cx);
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 1));
        assert_eq!(v.editor.document().unwrap().motion_groups().len(), 1);
    });
}
#[gpui_kit::test]
fn reference_cancel_and_workspace_roundtrip_reject_frozen_choice(cx: &mut TestAppContext) {
    let h = setup(cx);
    action(&h, A::AddMotionReference(None), cx);
    choose_motion(&h, "Once", cx);
    cx.update_window(h.window, |_, window, cx| window.press("escape", cx))
        .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0))
    });
    action(&h, A::AddMotionReference(None), cx);
    choose_motion(&h, "Loop", cx);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            v.editor
                .set_workspace(zipzi_editor_core::WorkspaceKind::Modeling)
                .unwrap();
            v.editor
                .set_workspace(zipzi_editor_core::WorkspaceKind::Animation)
                .unwrap();
            cx.notify();
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        assert!(
            window
                .find("motion-reference-reason")
                .label()
                .unwrap()
                .contains("context changed")
        );
        window.click("motion-reference-submit", cx);
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0))
    });
}

#[gpui_kit::test]
fn group_menu_dissolve_preserves_all_registrations_and_preview(cx: &mut TestAppContext) {
    let h = setup(cx);
    let (group, selection) = h.view.read_with(cx, |v, _| {
        (
            v.editor.document().unwrap().motion_groups()[0].clone(),
            v.editor.animation().unwrap().motion.clone().unwrap(),
        )
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(
            ("animation-motion-group-header", 0usize),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        )
    })
    .unwrap();
    cx.run_until_parked();
    menu(&h, "Dissolve Motion Group…", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("animation-delete-submit", cx)
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let doc = v.editor.document().unwrap();
        assert!(doc.motion_groups().is_empty());
        for entry in &group.entries {
            assert_eq!(doc.motion_registration(&entry.id).unwrap().1, entry);
        }
        assert_eq!(
            v.editor.animation().unwrap().motion.as_ref(),
            Some(&selection)
        );
        assert!(v.editor.animation().unwrap().playing);
        assert_eq!(v.editor.animation().unwrap().time, 0.4);
        assert_eq!(doc.history_lengths(), (1, 0));
    });
}
#[gpui_kit::test]
fn expression_deletion_clears_only_expression_preview_and_empty_group_deletes_directly(
    cx: &mut TestAppContext,
) {
    let h = setup(cx);
    let expression = h.view.read_with(cx, |v, _| {
        R::new(
            K::Expression,
            &v.editor.document().unwrap().expression_resources()[0].id,
        )
    });
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |v, _| {
            v.editor
                .select_expression(v.editor.active_id().unwrap(), expression.id.clone())
                .unwrap()
        })
    })
    .unwrap();
    action(
        &h,
        A::DeleteAnimation(expression.clone(), D::Expression),
        cx,
    );
    cx.update_window(h.window, |_, window, cx| {
        window.click("animation-delete-submit", cx)
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        assert!(
            v.editor
                .document()
                .unwrap()
                .expression_definition(&expression.id)
                .is_none()
        );
        let a = v.editor.animation().unwrap();
        assert!(a.expression.is_none());
        assert!(a.motion.is_some());
        assert!(!a.playing);
        assert_eq!(a.time, 0.4);
    });
    action(&h, A::Create(K::MotionGroup, None), cx);
    let group = h.view.read_with(cx, |v, _| {
        v.editor.active_inspection().unwrap().object().unwrap()
    });
    action(&h, A::DeleteAnimation(group.clone(), D::Group), cx);
    cx.update_window(h.window, |_, window, _| {
        assert!(window.try_find("animation-deletion-review").is_none())
    })
    .unwrap();
    h.view.read_with(cx, |v, _| {
        assert!(
            v.editor
                .document()
                .unwrap()
                .object_index()
                .get(&group)
                .is_none()
        );
        assert_eq!(v.editor.document().unwrap().history_lengths(), (3, 0));
    });
}

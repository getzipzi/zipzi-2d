use super::{
    document_fixture,
    properties::{fill, show},
    setup_with_document,
};
use crate::authoring::{AuthoringContext, animation::AnimationAction};
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};
use zipzi_editor_core::{CommandId, ObjectKind as K, ObjectRef as R, PalettePosition as P};

#[gpui_kit::test]
fn empty_group_creation_and_rename_preserve_preview_and_have_separate_undo(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.double_click(("animation-motion", 0usize), cx);
        h.view.update(cx, |v, _| {
            v.editor.advance_animation_clock(0.4);
        });
        window.click("animation-new-motion-group", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.editor.animation().unwrap().playing);
        assert_eq!(v.editor.animation().unwrap().time, 0.4);
        assert_eq!(v.editor.document().unwrap().motion_groups().len(), 2);
        assert!(
            v.editor.document().unwrap().motion_groups()[1]
                .entries
                .is_empty()
        );
        assert_eq!(v.editor.document().unwrap().history_lengths(), (1, 0));
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
    });
    cx.update_window(h.window, |_, window, _cx| {
        assert!(
            window
                .find(("animation-motion-group-header", 1usize))
                .visible()
        );
        assert!(
            window
                .try_find(("animation-motion-group-count", 1usize))
                .is_none()
        );
    })
    .unwrap();
    fill(&h, "property-input-name", "New Group", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.update(cx, |v, _| {
        let d = v.editor.document().unwrap();
        assert_eq!(d.motion_groups()[1].name, "New Group");
        assert_eq!(d.history_lengths(), (2, 0));
        assert!(v.editor.animation().unwrap().playing);
        assert_eq!(v.editor.animation().unwrap().time, 0.4);
        v.editor
            .execute(zipzi_editor_core::EditorCommand::Undo)
            .unwrap();
        assert_eq!(v.editor.document().unwrap().motion_groups().len(), 2);
        v.editor
            .execute(zipzi_editor_core::EditorCommand::Undo)
            .unwrap();
        assert_eq!(v.editor.document().unwrap().motion_groups().len(), 1);
    });
}

#[gpui_kit::test]
fn collapsed_group_creation_reveals_motion_and_only_creation_activates(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion-group-name", 0usize), cx);
        window.click(("animation-motion-group", 0usize), cx);
        window.click("animation-new-motion", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window
                .find(("animation-motion-group-header", 0usize))
                .expanded(),
            Some(true)
        );
        let v = h.view.read(cx);
        let a = v.editor.animation().unwrap();
        let d = v.editor.document().unwrap();
        assert_eq!(d.motion_groups()[0].entries.len(), 3);
        assert_eq!(
            a.motion.as_ref().unwrap().entry.as_ref(),
            Some(&d.motion_groups()[0].entries[2].id)
        );
        assert!(!a.playing);
        assert_eq!(a.time, 0.);
    })
    .unwrap();
}

#[gpui_kit::test]
fn new_expression_has_no_members_and_member_creation_focuses_parameter(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click("animation-new-expression", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    let expression = h.view.read_with(cx, |v, _| {
        let expression = v.editor.animation().unwrap().expression.clone().unwrap();
        assert!(
            v.editor
                .document()
                .unwrap()
                .expression_definition(&expression)
                .unwrap()
                .entries
                .is_empty()
        );
        expression
    });
    let button = format!("expression-action-new-{expression}");
    show(&h, &button, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(button.clone(), cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let v = h.view.read(cx);
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        let entries = &d.expression_definition(&expression).unwrap().entries;
        assert_eq!(entries.len(), 1);
        assert_eq!(
            v.editor.active_inspection().unwrap().object().unwrap(),
            R::new(K::ExpressionEntry, &entries[0].id)
        );
        assert_eq!(d.history_lengths(), (2, 0));
        assert!(window.try_find("property-select-target").is_some());
        assert_eq!(
            v.editor.animation().unwrap().expression.as_ref(),
            Some(&expression)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn registration_fade_fields_edit_only_current_registration_and_clear_override(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.resize(gpui_kit::size(gpui_kit::px(1200.), gpui_kit::px(1500.)));
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
    })
    .unwrap();
    let selection = h.view.read_with(cx, |v, _| {
        v.editor.animation().unwrap().motion.clone().unwrap()
    });
    let entry = selection.entry.as_ref().unwrap();
    let toggle = format!("property-custom-registration_fade_in-{entry}");
    let input = format!("property-input-registration_fade_in-{entry}");
    fill(&h, &input, "0.35", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        assert_eq!(d.motion_registration(entry).unwrap().1.fade_in, Some(0.35));
        assert_eq!(
            d.motion_definition(&selection.clip_id).unwrap().fade_in,
            Some(0.)
        );
        let other = &d.motion_groups()[0].entries[1];
        assert_eq!(other.fade_in, Some(1.));
    });
    show(&h, &toggle, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(toggle.clone(), cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |v, _| {
        assert_eq!(
            v.editor
                .document()
                .unwrap()
                .motion_registration(entry)
                .unwrap()
                .1
                .fade_in,
            None
        )
    });
}

#[gpui_kit::test]
fn resource_moves_do_not_activate_and_deferred_actions_reject_changed_context(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.double_click(("animation-motion", 0usize), cx);
        h.view.update(cx, |v, cx| {
            v.editor.advance_animation_clock(0.4);
            let selected = v.editor.animation().unwrap().motion.clone().unwrap();
            let source = R::new(K::MotionRegistration, selected.entry.as_ref().unwrap());
            let context = AuthoringContext::capture(&v.editor).unwrap();
            v.run_animation_action(
                context,
                AnimationAction::Move(source.clone(), None, P::Inside),
                window,
                cx,
            );
            assert_eq!(
                v.editor.animation().unwrap().motion.as_ref(),
                Some(&selected)
            );
            assert_eq!(v.editor.animation().unwrap().time, 0.4);
            assert!(v.editor.animation().unwrap().playing);
            let history = v.editor.document().unwrap().history_lengths();
            v.run_animation_action(context, AnimationAction::Step(source, true), window, cx);
            assert_eq!(v.editor.document().unwrap().history_lengths(), history);
            assert!(v.edit_error.is_some());
            v.run_command(CommandId::Undo, window, cx);
            assert_eq!(
                v.editor.animation().unwrap().motion.as_ref(),
                Some(&selected)
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn expression_member_fields_follow_ids_and_number_gestures_commit_once(cx: &mut TestAppContext) {
    use super::slide_number_input::{press, release, slide};
    use gpui_kit::{point, px};
    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    let (expression, entry) = h.view.read_with(cx, |v, _| {
        let d = v.editor.document().unwrap();
        let expression = d.expression_resources()[0].id.clone();
        let entry = d.expression_definition(&expression).unwrap().entries[0]
            .id
            .clone();
        (expression, entry)
    });
    cx.update_window(h.window, |_, window, cx| {
        window.resize(gpui_kit::size(px(1200.), px(1500.)));
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        super::animation::click_expression(window, 0, cx);
        window.render_frame(cx);
    })
    .unwrap();
    let field = format!("property-input-value-{entry}");
    fill(&h, &field, "0.6", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        assert_eq!(d.expression_entry(&entry).unwrap().1.value, 0.6);
        assert_eq!(d.history_lengths(), (1, 0));
    });
    show(&h, &field, cx);
    let origin = cx
        .update_window(h.window, |_, window, cx| {
            let origin = window
                .within(field.clone())
                .find("slide-number-input")
                .bounds()
                .center();
            press(window, origin, cx);
            origin
        })
        .unwrap();
    cx.update_window(h.window, |_, window, cx| {
        slide(window, origin + point(px(30.), px(0.)), false, cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(
            v.editor.document().unwrap().has_pending_edit(),
            "{:?}",
            v.edit_error
        );
        assert_eq!(v.editor.document().unwrap().history_lengths(), (1, 0));
    });
    cx.update_window(h.window, |_, window, cx| {
        release(window, origin + point(px(30.), px(0.)), cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        assert_eq!(v.editor.document().unwrap().history_lengths(), (2, 0));
        assert_eq!(
            v.editor.animation().unwrap().expression.as_ref(),
            Some(&expression)
        );
    });
}

#[gpui_kit::test]
fn clip_and_expression_number_drags_cancel_commit_and_roundtrip_history(cx: &mut TestAppContext) {
    use super::slide_number_input::{press, release, slide};
    use gpui_kit::{point, px};
    use zipzi_editor_core::{EditorCommand, property};
    let (document, assets) = document_fixture::animation_document().unwrap();
    let motion = R::new(K::Motion, &document.motion_resources()[0].selection.clip_id);
    let expression = R::new(K::Expression, &document.expression_resources()[0].id);
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.resize(gpui_kit::size(px(1200.), px(1500.)));
        window.click("workspace-animation", cx);
        window.render_frame(cx);
    })
    .unwrap();
    for (object, key) in [
        (motion, property::motion::DURATION.key),
        (expression, property::expression::FADE_IN.key),
    ] {
        cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |v, cx| {
                v.inspect_authoring_object(
                    v.editor.active_id().unwrap(),
                    object.clone(),
                    false,
                    window,
                    cx,
                )
                .unwrap();
                v.redraw(cx);
            });
            window.render_frame(cx);
        })
        .unwrap();
        let (original, history) = h.view.read_with(cx, |v, _| {
            let doc = v.editor.document().unwrap();
            (
                doc.property_values(&object).unwrap()[&key].clone(),
                doc.history_lengths(),
            )
        });
        let field = format!("property-input-{}", key.name());
        for cancel in [true, false] {
            show(&h, &field, cx);
            let origin = cx
                .update_window(h.window, |_, window, cx| {
                    let origin = window
                        .within(field.clone())
                        .find("slide-number-input")
                        .bounds()
                        .center();
                    press(window, origin, cx);
                    origin
                })
                .unwrap();
            let destination = origin + point(px(30.), px(0.));
            cx.update_window(h.window, |_, window, cx| {
                slide(window, destination, false, cx);
            })
            .unwrap();
            cx.run_until_parked();
            h.view.read_with(cx, |v, _| {
                assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
                let doc = v.editor.document().unwrap();
                assert!(doc.has_pending_edit());
                assert_eq!(doc.property_values(&object).unwrap()[&key], original);
                assert_eq!(doc.history_lengths(), history);
            });
            cx.update_window(h.window, |_, window, cx| {
                if cancel {
                    window.press("escape", cx);
                }
                release(window, destination, cx);
                window.render_frame(cx);
            })
            .unwrap();
            h.view.update(cx, |v, cx| {
                assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
                let doc = v.editor.document().unwrap();
                assert!(!doc.has_pending_edit());
                let value = doc.property_values(&object).unwrap()[&key].clone();
                if cancel {
                    assert_eq!(value, original);
                    assert_eq!(doc.history_lengths(), history);
                } else {
                    assert_ne!(value, original);
                    assert_eq!(doc.history_lengths(), (history.0 + 1, 0));
                    v.editor.execute(EditorCommand::Undo).unwrap();
                    assert_eq!(
                        v.editor
                            .document()
                            .unwrap()
                            .property_values(&object)
                            .unwrap()[&key],
                        original
                    );
                    v.editor.execute(EditorCommand::Redo).unwrap();
                    assert_eq!(
                        v.editor
                            .document()
                            .unwrap()
                            .property_values(&object)
                            .unwrap()[&key],
                        value
                    );
                    v.redraw(cx);
                }
            });
        }
    }
}

#[gpui_kit::test]
fn motion_drag_and_palette_shortcut_keep_registration_and_transport(cx: &mut TestAppContext) {
    use gpui_kit::{point, px};
    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.resize(gpui_kit::size(px(1200.), px(1500.)));
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.double_click(("animation-motion", 0usize), cx);
        h.view.update(cx, |v, _| {
            v.editor.advance_animation_clock(0.4);
        });
        let from = window.find(("animation-motion", 1usize)).bounds().center();
        let target = window.find(("animation-motion", 0usize)).bounds();
        window.drag(from, point(target.center().x, target.top() + px(2.)), cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        assert_eq!(d.history_lengths(), (1, 0));
        assert_eq!(
            v.editor
                .animation()
                .unwrap()
                .motion
                .as_ref()
                .unwrap()
                .entry
                .as_ref(),
            Some(&d.motion_groups()[0].entries[1].id)
        );
        assert!(v.editor.animation().unwrap().playing);
        assert_eq!(v.editor.animation().unwrap().time, 0.4);
    });
    cx.update_window(h.window, |_, window, cx| {
        window.press("alt-down", cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        assert_eq!(d.history_lengths(), (2, 0));
        assert_eq!(
            v.editor
                .animation()
                .unwrap()
                .motion
                .as_ref()
                .unwrap()
                .entry
                .as_ref(),
            Some(&d.motion_groups()[0].entries[0].id)
        );
        assert!(v.editor.animation().unwrap().playing);
        assert_eq!(v.editor.animation().unwrap().time, 0.4);
    });
}

#[gpui_kit::test]
fn right_click_move_picker_keeps_playing_and_moves_the_clicked_registration(
    cx: &mut TestAppContext,
) {
    use gpui_kit::{MouseButton, px};
    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(h.window, |_, window, cx| {
        window.resize(gpui_kit::size(px(1200.), px(1500.)));
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.double_click(("animation-motion", 0usize), cx);
        h.view.update(cx, |v, _| {
            v.editor.advance_animation_clock(0.4);
        });
        window.click_with_options(
            ("animation-motion", 1usize),
            gpui_kit::test::ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        for i in 0..32usize {
            if let Some(item) = window.within("popup-menu").try_find(i) {
                if let Some(label) = item.label() {
                    assert!(
                        ![
                            "Move up",
                            "Move down",
                            "Move Earlier in Execution",
                            "Move Later in Execution"
                        ]
                        .iter()
                        .any(|prefix| label.starts_with(prefix)),
                        "Unexpected context action: {label}"
                    );
                }
            }
        }
        let index = (0..16usize)
            .find(|i| {
                window
                    .within("popup-menu")
                    .try_find(*i)
                    .is_some_and(|e| e.label() == Some("Move to…"))
            })
            .expect("Move to action");
        window.within("popup-menu").click(index, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        assert!(h.view.read(cx).editor.animation().unwrap().playing);
        assert_eq!(h.view.read(cx).editor.animation().unwrap().time, 0.4);
        window.within("move-destination-select").click("input", cx);
        window.input("Root", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(160));
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        assert_ne!(
            window.find("move-destination-submit").disabled(),
            Some(true)
        );
        window.click("move-destination-submit", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        assert_eq!(d.motion_groups()[0].entries.len(), 1);
        let selected = v.editor.animation().unwrap().motion.as_ref().unwrap();
        assert_eq!(
            selected.entry.as_ref(),
            Some(&d.motion_groups()[0].entries[0].id)
        );
        assert!(v.editor.animation().unwrap().playing);
        assert_eq!(v.editor.animation().unwrap().time, 0.4);
        assert_eq!(d.history_lengths(), (1, 0));
    });
    // A later blocking dialog must not inherit this dialog's playback policy.
    cx.update_window(h.window, |_, window, cx| {
        use gpui_kit::{ParentElement as _, component::WindowExt as _};
        window.open_dialog(cx, |dialog, _, _| {
            dialog.title("Blocking dialog").child("Content")
        });
        window.render_frame(cx);
        assert!(!h.view.read(cx).editor.animation().unwrap().playing);
        window.close_dialog(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn resource_context_menus_group_creation_and_organization_before_removal(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, px, size, test::ClickOptions};
    use zipzi_editor_core::PanelKind;
    let (document, assets) = document_fixture::combined_animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.resize(size(px(1200.), px(1500.)));
        window.click("workspace-animation", cx);
    })
    .unwrap();
    for (panel, row, expected) in [
        (
            PanelKind::AnimationMotions,
            ("animation-motion", 0usize),
            vec![
                "New Motion",
                "Copy Motion Content",
                "Move to…",
                "Remove Registration",
                "Delete Motion Content Everywhere…",
            ],
        ),
        (
            PanelKind::AnimationExpressions,
            ("animation-expression", 0usize),
            vec![
                "New Expression",
                "Add Expression Member",
                "Delete Expression…",
            ],
        ),
        (
            PanelKind::AnimationPhysics,
            ("physics-group-row", 0usize),
            vec!["Add group", "Duplicate", "Delete"],
        ),
    ] {
        cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |v, cx| {
                v.dock.as_ref().unwrap().show(panel, None, window, cx)
            });
            window.render_frame(cx);
            window.click_with_options(row, ClickOptions::new().with_button(MouseButton::Right), cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(h.window, |_, window, cx| {
            let labels: Vec<_> = (0..32usize)
                .filter_map(|i| {
                    window
                        .within("popup-menu")
                        .try_find(i)
                        .and_then(|item| item.label().map(str::to_owned))
                })
                .collect();
            assert!(!labels.iter().any(|label| label == "Properties"));
            assert_eq!(
                labels.first().map(String::as_str),
                expected.first().copied()
            );
            let mut previous = None;
            for expected in expected {
                let at = labels
                    .iter()
                    .position(|label| label.starts_with(expected))
                    .unwrap_or_else(|| panic!("Missing {expected}: {labels:?}"));
                assert!(
                    previous.is_none_or(|previous| previous < at),
                    "Wrong menu order: {labels:?}"
                );
                previous = Some(at);
            }
            assert!(!labels.iter().any(|label| {
                ["Move up", "Move down", "Move Earlier", "Move Later"]
                    .iter()
                    .any(|prefix| label.starts_with(prefix))
            }));
            window.press("escape", cx);
        })
        .unwrap();
        cx.run_until_parked();
    }
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0))
    });
}

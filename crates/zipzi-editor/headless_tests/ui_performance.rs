//! Fast work-count regressions, using synthetic data and no wall-clock budgets.
//! Keep functional assertions alongside work limits so skipping updates cannot pass.
use super::{setup, setup_with_document};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use zipzi_editor_core::property as schema;

#[gpui_kit::test]
fn timeline_channel_scrubs_reuse_overview_and_do_not_notify_the_editor_owner(
    cx: &mut TestAppContext,
) {
    use super::{
        document_fixture,
        slide_number_input::{press, release, slide},
    };
    use gpui_kit::{point, px};
    use std::{cell::Cell, rc::Rc};
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let (timeline, origin, key) = cx
        .update_window(harness.window, |_, window, cx| {
            window.activate_window();
            window.click("workspace-animation", cx);
            window.render_frame(cx);
            window.click(("animation-motion", 0usize), cx);
            window.render_frame(cx);
            let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
            let row = projection
                .rows
                .iter()
                .find(|row| row.track_id.is_some() && !row.keys.is_empty())
                .unwrap();
            let origin = window
                .within("timeline-rows")
                .find(format!("timeline-number-RowValue({:?})", row.id))
                .bounds()
                .center();
            let timeline = harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .timeline
                .clone();
            press(window, origin, cx);
            (timeline, origin, row.keys[0].clone())
        })
        .unwrap();
    cx.run_until_parked();
    let notifications = Rc::new(Cell::new(0));
    let observed = notifications.clone();
    let _observe =
        cx.update(|cx| cx.observe(&harness.view, move |_, _| observed.set(observed.get() + 1)));
    let status_bar = harness
        .view
        .read_with(cx, |view, _| view.status_bar.clone());
    let before = timeline.read_with(cx, |timeline, _| {
        (
            timeline.renders(),
            timeline.scene_builds(),
            timeline.scene_paints(),
        )
    });
    let status_renders_before = status_bar.read_with(cx, |bar, _| bar.renders());
    for sample in 1..=12 {
        cx.update_window(harness.window, |_, window, cx| {
            slide(
                window,
                origin + point(px(sample as f32 * 8.), px(0.)),
                false,
                cx,
            );
        })
        .unwrap();
        cx.update_window(harness.window, |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
    }
    let after = timeline.read_with(cx, |timeline, _| {
        (
            timeline.renders(),
            timeline.scene_builds(),
            timeline.scene_paints(),
        )
    });
    let status_renders_after = status_bar.read_with(cx, |bar, _| bar.renders());
    assert_eq!(
        notifications.get(),
        0,
        "candidate updates notified unrelated editor views"
    );
    assert!(
        status_renders_after - status_renders_before <= 1,
        "StatusBar rendered {} times during 12 scrub samples",
        status_renders_after - status_renders_before
    );
    assert!(
        after.0 - before.0 <= 13,
        "{} Timeline renders for 12 moves",
        after.0 - before.0
    );
    assert_eq!(
        after.1, before.1,
        "value-only edits rebuilt the overview scene"
    );
    assert!(
        after.2 - before.2 <= 1,
        "static overview was repainted repeatedly"
    );
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.timeline_edit().unwrap().identity.revision, 12);
        assert!(
            (view
                .editor
                .timeline_projection()
                .unwrap()
                .key(&key)
                .unwrap()
                .value
                - 0.24)
                .abs()
                < 0.00001
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.press("escape", cx);
        release(window, origin, cx);
        window.render_frame(cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.timeline_edit().is_none());
        assert_eq!(
            view.editor
                .timeline_projection()
                .unwrap()
                .key(&key)
                .unwrap()
                .value,
            0.
        );
    });
}

/// New preview frames must not trigger a second frame just to repaint an
/// unchanged zoom readout in the separately observed dock header.
#[gpui_kit::test]
fn canvas_frame_notifications_do_not_redraw_unchanged_header(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let canvas = harness.view.read_with(cx, |view, _| {
        view.dock.as_ref().unwrap().active_viewport().clone()
    });
    for _ in 0..3 {
        canvas.update(cx, |_, cx| cx.notify());
        cx.update_window(harness.window, |_, window, cx| {
            window.draw(cx).clear(cx);
        })
        .unwrap();
    }
    cx.update_window(harness.window, |_, window, _| window.reset_layout_stats())
        .unwrap();
    for _ in 0..10 {
        cx.update_window(harness.window, |_, window, cx| {
            canvas.update(cx, |_, cx| cx.notify());
            window.draw(cx).clear(cx);
        })
        .unwrap();
    }
    let frames = cx
        .update_window(harness.window, |_, window, _| window.layout_stats().frames)
        .unwrap();
    assert_eq!(
        frames, 10,
        "unchanged canvas header scheduled redundant draws"
    );
}

#[gpui_kit::test]
fn dense_document_properties_virtualize_and_remain_scrollable_after_resize(
    cx: &mut TestAppContext,
) {
    use gpui_kit::component::dock::DockPlacement;
    use gpui_kit::{prelude::*, *};
    use zipzi_core::{Canvas, Vec2, document::CdiParameterGroup};
    use zipzi_sdk::AuthoringSession;
    let assets = tempfile::tempdir().unwrap();
    let mut session = AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::default(), 1.),
    )
    .unwrap();
    session
        .edit("Dense property fixture", None, |edit| {
            for i in 0..200 {
                edit.create_parameter_group(CdiParameterGroup {
                    id: uuid::Uuid::from_u128(i + 1).to_string(),
                    name: format!("Group {i}"),
                    runtime_id: format!("Group{i}"),
                    parent_id: None,
                    extensions: Default::default(),
                })?;
            }
            Ok(())
        })
        .unwrap();
    let saved = session
        .save_project(&assets.path().join("dense.zipzi"), None)
        .unwrap();
    let document = zipzi_editor_core::Document::open(&saved.manifest).unwrap();
    let fields: Vec<_> = crate::panels::document_panel::property_rows(
        &document.object_index().objects[0].properties,
    )
    .into_iter()
    .filter(|(field, _)| {
        !field.starts_with("export_settings.") && !field.starts_with("source_canvas.")
    })
    .collect();
    assert!(fields.len() > 500);
    let first_detail: SharedString =
        format!("object-property-{}", fields.first().unwrap().0).into();
    let last: SharedString = format!("object-property-{}", fields.last().unwrap().0).into();
    let harness = setup_with_document(cx, document, assets);
    let properties = harness
        .view
        .read_with(cx, |view, _| view.dock.as_ref().unwrap().properties.clone());
    cx.update_window(harness.window, |_, window, cx| {
        window.click("property-group-$details", cx);
        window.render_frame(cx);
    })
    .unwrap();
    for width in [280., 180., 340.] {
        cx.update_window(harness.window, |_, window, cx| {
            let builds = properties.read(cx).row_builds();
            let cache = properties.read(cx).properties_cache_stats();
            let projection_builds = properties.read(cx).work_counts().0;
            let area = harness.view.read(cx).dock.as_ref().unwrap().area.clone();
            area.update(cx, |area, cx| {
                area.set_dock_size(DockPlacement::Right, px(width), window, cx)
            });
            window.render_frame(cx);
            // Count construction across every intermediate draw, not just the
            // elements left in the final frame. Allow overscan and remeasurement.
            let built = properties.read(cx).row_builds() - builds;
            assert_eq!(
                properties.read(cx).work_counts().0,
                projection_builds,
                "resize must reuse the complete row projection"
            );
            assert_eq!(
                properties.read(cx).properties_cache_stats(),
                cache,
                "resize must reuse property data"
            );
            assert!(
                built > 0 && built < 120,
                "resize built {built} rows at width {width}"
            );
            let visible = fields
                .iter()
                .filter(|(field, _)| {
                    window
                        .try_find(format!("object-property-{field}"))
                        .is_some()
                })
                .count();
            assert!(
                visible > 0 && visible < 40,
                "laid out {visible} properties at width {width}"
            );
            for _ in 0..3 {
                window.scroll(
                    "object-properties",
                    ScrollDelta::Pixels(point(px(0.), px(-100_000.))),
                    cx,
                );
                window.render_frame(cx);
            }
            let viewport = window.find("object-properties").bounds();
            let end = window.find(last.clone()).bounds();
            assert!(end.top() >= viewport.top() && end.bottom() <= viewport.bottom() + px(1.));
            window.scroll(
                "object-properties",
                ScrollDelta::Pixels(point(px(0.), px(100_000.))),
                cx,
            );
            window.render_frame(cx);
            assert!(
                window
                    .try_find("object-property-export_settings.padding_pixels")
                    .is_none()
            );
            window.click("property-group-$details", cx);
            window.render_frame(cx);
            assert!(
                window
                    .try_find("object-property-export_settings.padding_pixels")
                    .is_none()
            );
            assert!(window.try_find(first_detail.clone()).is_none());
            window.click("property-group-$details", cx);
            window.render_frame(cx);
            assert!(window.try_find(first_detail.clone()).is_some());
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn property_choices_and_rows_survive_redraws_and_refresh_after_history(cx: &mut TestAppContext) {
    use super::slide_number_input::{press, release, slide};
    use gpui_kit::{point, px, size};
    use zipzi_editor_core::{
        Document, EditorCommand, NewParameter, ObjectKind, ObjectRef, ParameterGroupDraft,
    };

    let mut doc = Document::new_blank().unwrap();
    let group = doc
        .create_parameter_group(
            ParameterGroupDraft {
                name: "Face".into(),
                runtime_id: "Face".into(),
                parent_id: None,
            },
            doc.parameter_model_revision(),
        )
        .unwrap();
    let parameter = doc.create_parameter(NewParameter::default()).unwrap();
    let h = setup_with_document(cx, doc, tempfile::tempdir().unwrap());
    cx.simulate_window_resize(h.window, size(px(1100.), px(1400.)));
    let panel = h
        .view
        .read_with(cx, |view, _| view.dock.as_ref().unwrap().properties.clone());
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        assert!(window.try_find("property-select-group_id").is_some());
        assert!(
            panel
                .read(cx)
                .choice_labels(schema::parameter::GROUP_ID.key)
                .contains(&"Face".into())
        );
    })
    .unwrap();
    cx.run_until_parked();
    let before = panel.read_with(cx, |p, _| p.work_counts());
    let notifications = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = notifications.clone();
    let _observe = cx.update(|cx| cx.observe(&panel, move |_, _| observed.set(observed.get() + 1)));
    for _ in 0..5 {
        h.view.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        cx.update_window(h.window, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }
    assert_eq!(
        notifications.get(),
        0,
        "unrelated owner notifications must not notify the inspector again"
    );
    let after = panel.read_with(cx, |p, _| p.work_counts());
    assert_eq!((after.0, after.1), (before.0, before.1));
    for _ in 0..5 {
        panel.update(cx, |_, cx| cx.notify());
        cx.update_window(h.window, |_, window, cx| window.render_frame(cx))
            .unwrap();
    }
    let after = panel.read_with(cx, |p, _| p.work_counts());
    assert!(after.2 > before.2);
    assert_eq!((after.0, after.1), (before.0, before.1));

    let origin = cx
        .update_window(h.window, |_, window, cx| {
            let origin = window
                .within("property-input-default_value")
                .find("slide-number-input")
                .bounds()
                .center();
            press(window, origin, cx);
            origin
        })
        .unwrap();
    for sample in 1..=8 {
        cx.update_window(h.window, |_, window, cx| {
            slide(
                window,
                origin + point(px(sample as f32 * 8.), px(0.)),
                false,
                cx,
            );
            window.render_frame(cx);
        })
        .unwrap();
    }
    let after = panel.read_with(cx, |p, _| p.work_counts());
    assert_eq!(
        (after.0, after.1),
        (before.0, before.1),
        "candidate values must reuse choices and collapsed rows"
    );
    h.view.read_with(cx, |view, _| {
        assert!(
            view.editor
                .document()
                .unwrap()
                .object_index()
                .get(&ObjectRef::new(ObjectKind::Parameter, &parameter))
                .unwrap()
                .properties["default_value"]
                .as_f64()
                .unwrap()
                > 0.
        );
    });
    cx.update_window(h.window, |_, window, cx| {
        window.press("escape", cx);
        release(window, origin, cx);
        window.render_frame(cx);
    })
    .unwrap();
    h.view.update(cx, |view, cx| {
        view.editor
            .execute(EditorCommand::SetObjectProperty {
                object: ObjectRef::new(ObjectKind::ParameterGroup, &group),
                field: "name".into(),
                value: serde_json::json!("Eyes"),
            })
            .unwrap();
        view.editor
            .execute(EditorCommand::Navigate(ObjectRef::new(
                ObjectKind::Parameter,
                &parameter,
            )))
            .unwrap();
        view.redraw(cx);
    });
    cx.update_window(h.window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    assert!(
        panel
            .read_with(cx, |p, _| p.choice_labels(schema::parameter::GROUP_ID.key))
            .contains(&"Eyes".into())
    );
    h.view.update(cx, |view, cx| {
        view.editor.execute(EditorCommand::Undo).unwrap();
        view.editor
            .execute(EditorCommand::Navigate(ObjectRef::new(
                ObjectKind::Parameter,
                &parameter,
            )))
            .unwrap();
        view.redraw(cx);
    });
    cx.update_window(h.window, |_, window, cx| window.render_frame(cx))
        .unwrap();
    let labels = panel.read_with(cx, |p, _| p.choice_labels(schema::parameter::GROUP_ID.key));
    assert!(labels.contains(&"Face".into()));
    assert!(!labels.contains(&"Eyes".into()));
}

fn physics_properties_harness(cx: &mut TestAppContext) -> super::Harness {
    use gpui_kit::{px, size};
    use zipzi_editor_core::{CommandId, PanelKind};

    let (document, assets) = super::document_fixture::combined_animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(h.window, |_, window, cx| {
        window.resize(size(px(1100.), px(1400.)));
        window.activate_window();
        h.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx);
            view.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::AnimationPhysics, None, window, cx);
        });
        window.render_frame(cx);
        window.click(("physics-group-row", 0usize), cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    h
}

#[gpui_kit::test]
fn physics_resource_commits_reuse_controls_but_keep_revision_and_structure_guards(
    cx: &mut TestAppContext,
) {
    use crate::properties::physics::NumberField;
    use zipzi_editor_core::{EditorCommand, PropertyPatch, PropertyScope, PropertyValue};

    let h = physics_properties_harness(cx);
    let panel = h
        .view
        .read_with(cx, |v, _| v.dock.as_ref().unwrap().properties.clone());
    let key = h.view.read_with(cx, |v, _| {
        NumberField::InputWeight(0)
            .spec(
                &v.editor
                    .document()
                    .unwrap()
                    .physics_definition()
                    .unwrap()
                    .data
                    .settings[0],
            )
            .unwrap()
            .key
    });
    let (name, weight, initial) = panel.read_with(cx, |p, _| {
        (
            p.field_input(schema::physics::NAME.key),
            p.field_input(key),
            p.current_target().unwrap().clone(),
        )
    });
    let mut stale_form = crate::properties::form::PropertyForm::default();
    h.view.read_with(cx, |v, _| {
        stale_form.sync_definition(
            initial.definition(&v.editor),
            initial.values(&v.editor).unwrap(),
        )
    });
    stale_form.edit(&schema::physics::NAME, "Stale name".into());
    let stale_patch = PropertyPatch {
        scope: initial.scope.clone(),
        ..stale_form
            .patch(initial.object.kind, schema::physics::NAME.key)
            .unwrap()
    };

    for (index, value) in [25., 75., 35.].into_iter().enumerate() {
        cx.update_window(h.window, |_, window, cx| {
            window
                .within("physics-number-InputWeight(0)")
                .click("slide-number-input", cx);
            window.press("cmd-a", cx);
            window.input(&value.to_string(), cx);
            window.press("enter", cx);
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        h.view.read_with(cx, |v, cx| {
            assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
            let doc = v.editor.document().unwrap();
            assert_eq!(
                doc.physics_definition().unwrap().data.settings[0].inputs[0].weight,
                value
            );
            assert_eq!(doc.history_lengths(), (index + 1, 0));
            let p = panel.read(cx);
            assert_eq!(p.field_input(schema::physics::NAME.key), name);
            assert_eq!(p.field_input(key), weight);
            assert_eq!(weight.read(cx).value(), value.to_string());
            assert_eq!(
                p.current_target().unwrap().scope,
                PropertyScope::Resource {
                    revision: doc.model_generation()
                }
            );
            assert_ne!(p.current_target().unwrap(), &initial);
        });
    }
    h.view.update(cx, |v, _| {
        assert!(
            initial.commit(&mut v.editor, stale_patch).is_err(),
            "widget reuse must not revive a captured resource revision"
        );
        assert_eq!(v.editor.document().unwrap().history_lengths(), (3, 0));
    });
    for (command, value) in [(EditorCommand::Undo, 75.), (EditorCommand::Redo, 35.)] {
        cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |v, cx| {
                v.editor.execute(command).unwrap();
                v.redraw(cx);
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        panel.read_with(cx, |p, cx| {
            assert_eq!(p.field_input(key), weight);
            assert_eq!(weight.read(cx).value(), value.to_string());
            assert_eq!(
                p.current_target()
                    .unwrap()
                    .values(&h.view.read(cx).editor)
                    .unwrap()[&key],
                PropertyValue::Float(value)
            );
        });
    }
    cx.update_window(h.window, |_, window, cx| {
        window.click("physics-channel-false-0", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    let after_structure = panel.read_with(cx, |p, _| {
        assert_ne!(
            p.field_input(key),
            weight,
            "new fields must invalidate the cached controls"
        );
        let input = p.field_input(schema::physics::NAME.key);
        assert_ne!(input, name);
        input
    });
    // The Physics resource list owns copy through its current context menu.
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(
            ("physics-group-row", 0usize),
            gpui_kit::test::ClickOptions::new().with_button(gpui_kit::MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        let duplicate = (0..16usize)
            .find(|index| {
                window
                    .within("popup-menu")
                    .try_find(*index)
                    .is_some_and(|item| item.label() == Some("Duplicate"))
            })
            .expect("Physics Duplicate command");
        window.within("popup-menu").click(duplicate, cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    panel.read_with(cx, |p, _| {
        assert_ne!(p.current_target().unwrap().object, initial.object);
        assert_ne!(p.field_input(schema::physics::NAME.key), after_structure);
    });
}

#[gpui_kit::test]
fn physics_resource_revision_refresh_discards_unsubmitted_widget_text(cx: &mut TestAppContext) {
    use zipzi_editor_core::{PropertyPatch, PropertyValue};

    let h = physics_properties_harness(cx);
    let panel = h
        .view
        .read_with(cx, |v, _| v.dock.as_ref().unwrap().properties.clone());
    let input = panel.read_with(cx, |p, _| p.field_input(schema::physics::NAME.key));
    let target = panel.read_with(cx, |p, _| p.current_target().unwrap().clone());
    let patch = h.view.read_with(cx, |v, _| PropertyPatch {
        scope: target.scope.clone(),
        expected: [(
            schema::physics::NAME.key,
            target.values(&v.editor).unwrap()[&schema::physics::NAME.key].clone(),
        )]
        .into(),
        changes: [(
            schema::physics::NAME.key,
            PropertyValue::Text("External name".into()),
        )]
        .into(),
    });
    cx.update_window(h.window, |_, window, cx| {
        input.update(cx, |input, cx| {
            // No Change event: this draft has not reached the form yet.
            input.set_value("Unsubmitted draft", window, cx);
            input.focus(window, cx);
        });
        h.view.update(cx, |v, cx| {
            target.commit(&mut v.editor, patch).unwrap();
            v.redraw(cx);
        });
        window.render_frame(cx);
        assert_eq!(panel.read(cx).field_input(schema::physics::NAME.key), input);
        assert_eq!(input.read(cx).value(), "External name");
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        let doc = v.editor.document().unwrap();
        assert_eq!(doc.history_lengths(), (1, 0));
        assert_eq!(
            doc.property_values(&target.object).unwrap()[&schema::physics::NAME.key].text(),
            "External name"
        );
        assert!(
            !v.property_forms
                .dirty_document(v.editor.active_id().unwrap())
        );
    });
}

#[gpui_kit::test]
fn motion_select_keeps_static_properties_retained_during_canvas_frames(cx: &mut TestAppContext) {
    let (document, assets) = super::document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update(|cx| cx.set_reduce_motion(true));
    let (panel, canvas) = cx
        .update_window(h.window, |_, window, cx| {
            window.activate_window();
            window.click("workspace-animation", cx);
            window.render_frame(cx);
            window.click(("animation-motion", 0usize), cx);
            window.render_frame(cx);
            window.blur(cx);
            let dock = h.view.read(cx).dock.as_ref().unwrap();
            (dock.properties.clone(), dock.active_viewport().clone())
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        // Settle initial selection and its changed searchable configuration.
        for _ in 0..3 {
            window.draw(cx).clear(cx);
        }
        let target = panel.read(cx).current_target().unwrap().clone();
        let sound = target
            .definition(&h.view.read(cx).editor)
            .fields
            .into_iter()
            .find(|f| f.binding.key.name() == "registration_sound")
            .unwrap();
        assert!(
            window
                .try_find(format!("property-select-{}", sound.binding.key.identity()))
                .is_some()
        );
        window.reset_layout_stats();
    })
    .unwrap();
    let before = panel.read_with(cx, |p, _| p.work_counts());
    let name = panel.read_with(cx, |p, cx| {
        p.field_input(schema::motion::NAME.key).read(cx).value()
    });
    for _ in 0..12 {
        cx.update_window(h.window, |_, window, cx| {
            canvas.update(cx, |_, cx| cx.notify());
            window.draw(cx).clear(cx);
        })
        .unwrap();
    }
    let after = panel.read_with(cx, |p, cx| {
        assert_eq!(
            p.field_input(schema::motion::NAME.key).read(cx).value(),
            name
        );
        p.work_counts()
    });
    assert_eq!(
        after, before,
        "unchanged Select configuration invalidated the properties subtree"
    );
    cx.update_window(h.window, |_, window, _| {
        assert_eq!(window.layout_stats().frames, 12)
    })
    .unwrap();
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0))
    });
}

#[gpui_kit::test]
fn physics_resource_refresh_reuses_closed_selects_but_invalidates_open_choices(
    cx: &mut TestAppContext,
) {
    use zipzi_editor_core::{PropertyPatch, PropertyValue};

    let h = physics_properties_harness(cx);
    let panel = h
        .view
        .read_with(cx, |v, _| v.dock.as_ref().unwrap().properties.clone());
    let key = panel.read_with(cx, |p, cx| {
        p.current_target()
            .unwrap()
            .definition(&h.view.read(cx).editor)
            .fields
            .into_iter()
            .find(|f| f.binding.key.name() == "input_axis")
            .unwrap()
            .binding
            .key
    });
    let original = panel.read_with(cx, |p, _| p.field_select(key));
    for open in [false, true] {
        cx.update_window(h.window, |_, window, cx| {
            if open {
                window
                    .within(format!("property-select-{}", key.identity()))
                    .click("input", cx);
                assert!(original.read(cx).is_open());
            }
            let target = panel.read(cx).current_target().unwrap().clone();
            h.view.update(cx, |v, cx| {
                let patch = PropertyPatch {
                    scope: target.scope.clone(),
                    expected: [(
                        schema::physics::NAME.key,
                        target.values(&v.editor).unwrap()[&schema::physics::NAME.key].clone(),
                    )]
                    .into(),
                    changes: [(
                        schema::physics::NAME.key,
                        PropertyValue::Text(format!("External {open}")),
                    )]
                    .into(),
                };
                target.commit(&mut v.editor, patch).unwrap();
                v.redraw(cx);
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        panel.read_with(cx, |p, cx| {
            let current = p.field_select(key);
            assert_eq!(current == original, !open);
            assert!(!current.read(cx).is_open());
        });
    }
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (2, 0))
    });
}

#[gpui_kit::test]
fn list_searchable_only_notifies_when_the_value_changes(cx: &mut TestAppContext) {
    use gpui_kit::component::{
        IndexPath,
        list::{ListDelegate, ListItem, ListState},
    };
    use gpui_kit::{Context, Window};
    use std::{cell::Cell, rc::Rc};

    struct Items;
    impl ListDelegate for Items {
        type Item = ListItem;
        fn items_count(&self, _: usize, _: &gpui_kit::App) -> usize {
            1
        }
        fn set_selected_index(
            &mut self,
            _: Option<IndexPath>,
            _: &mut Window,
            _: &mut Context<ListState<Self>>,
        ) {
        }
        fn render_item(
            &mut self,
            index: IndexPath,
            _: &mut Window,
            _: &mut Context<ListState<Self>>,
        ) -> Option<ListItem> {
            Some(ListItem::new(index.row))
        }
    }
    let h = setup(cx);
    let list = cx
        .update_window(h.window, |_, window, cx| {
            cx.new(|cx| ListState::new(Items, window, cx))
        })
        .unwrap();
    cx.run_until_parked();
    let notifications = Rc::new(Cell::new(0));
    let observed = notifications.clone();
    let _observe = cx.update(|cx| cx.observe(&list, move |_, _| observed.set(observed.get() + 1)));
    for (searchable, expected) in [(false, 0), (true, 1), (true, 1), (false, 2), (false, 2)] {
        cx.update_window(h.window, |_, _, cx| {
            list.update(cx, |list, cx| list.set_searchable(searchable, cx))
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(notifications.get(), expected);
    }
}

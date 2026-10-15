//! Fast work-count regressions, using synthetic data and no wall-clock budgets.
//! Keep functional assertions alongside work limits so skipping updates cannot pass.
use super::{setup, setup_with_document};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

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
    let before = timeline.read_with(cx, |timeline, _| {
        (
            timeline.renders(),
            timeline.scene_builds(),
            timeline.scene_paints(),
        )
    });
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
    assert_eq!(
        notifications.get(),
        0,
        "candidate updates notified unrelated editor views"
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
        view.dock.as_ref().unwrap().active_canvas().clone()
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
    use kasane_core::{Canvas, Vec2, document::CdiParameterGroup};
    use kasane_sdk::AuthoringSession;
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
        .save_project(&assets.path().join("dense.kasane"), None)
        .unwrap();
    let document = editor_core::Document::open(&saved.manifest).unwrap();
    let mut fields = crate::panels::document_panel::property_rows(
        &document.object_index().objects[0].properties,
    );
    fields.sort_by_key(|(field, _)| (field != "name", field.clone()));
    assert!(fields.len() > 500);
    let last: SharedString = format!("object-property-{}", fields.last().unwrap().0).into();
    let harness = setup_with_document(cx, document, assets);
    let properties = harness
        .view
        .read_with(cx, |view, _| view.dock.as_ref().unwrap().properties.clone());
    for width in [280., 180., 340.] {
        cx.update_window(harness.window, |_, window, cx| {
            let builds = properties.read(cx).row_builds();
            let area = harness.view.read(cx).dock.as_ref().unwrap().area.clone();
            area.update(cx, |area, cx| {
                area.set_dock_size(DockPlacement::Right, px(width), window, cx)
            });
            window.render_frame(cx);
            // Count construction across every intermediate draw, not just the
            // elements left in the final frame. Allow overscan and remeasurement.
            let built = properties.read(cx).row_builds() - builds;
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
            window.click("property-group-canvas", cx);
            window.render_frame(cx);
            assert!(window.try_find("object-property-canvas.width").is_none());
            window.click("property-group-canvas", cx);
            window.render_frame(cx);
            assert!(window.try_find("object-property-canvas.width").is_some());
        })
        .unwrap();
    }
}

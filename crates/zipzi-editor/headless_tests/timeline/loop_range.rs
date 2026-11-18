use super::super::slide_number_input::{press, press_with_modifiers, release, slide};
use super::super::{document_fixture, setup_with_document};
use gpui_kit::{
    App, AppContext as _, InputEvent as _, Modifiers, MouseButton, MouseMoveEvent, Pixels, Point,
    TestAppContext, Window, point, px, size,
    test::{ClickOptions, TestWindowExt},
};
use zipzi_editor_core::{AnimationEvaluator, AnimationPreviewMode};

fn hover(window: &mut Window, position: Point<Pixels>, cx: &mut App) {
    window.dispatch_event(
        MouseMoveEvent {
            position,
            pressed_button: None,
            modifiers: Modifiers::default(),
        }
        .to_platform_input(),
        cx,
    );
    window.render_frame(cx);
}

#[gpui_kit::test]
fn timeline_ruler_routes_loop_and_playhead_gestures_and_switches_playing_clocks(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    let range = |c: &App| h.view.read(c).editor.timeline_view().unwrap().loop_range();
    cx.update_window(h.window, |_, w, c| {
        w.click("workspace-animation", c);
        w.render_frame(c);
        w.click(("animation-motion", 0usize), c);
        h.view.update(c, |v, c| {
            let document = v.editor.active_id().unwrap();
            v.editor
                .set_animation_preview_mode(document, AnimationPreviewMode::Editing)
                .unwrap();
            v.animation_clock_running = true;
            v.redraw(c);
        });
        w.render_frame(c);
        let ticks = w.within("animation-seek").find("timeline-ruler").bounds();
        assert!(ticks.size.height >= px(30.));
        assert!(w.try_find("timeline-loop-lane").is_none());
        let at = |fraction| {
            point(
                ticks.left() + ticks.size.width * fraction,
                ticks.top() + px(4.),
            )
        };
        let scrub_at =
            |fraction| point(ticks.left() + ticks.size.width * fraction, ticks.center().y);
        assert!(w.try_find("timeline-loop-range").is_none());
        assert!(w.try_find("timeline-loop-controls").is_none());

        // Even at the top edge an unmodified drag creates no range.
        w.drag(at(0.6), at(0.25), c);
        assert!(range(c).is_none());
        press_with_modifiers(
            w,
            scrub_at(0.60),
            Modifiers {
                shift: true,
                ..Default::default()
            },
            c,
        );
        slide(w, scrub_at(0.25), true, c);
        assert!(range(c).is_none());
        release(w, scrub_at(0.25), c);
        w.render_frame(c);
        let selected = range(c).unwrap();
        assert!(selected.enabled);

        // Dragging through the existing marker scrubs; a stationary click toggles it.
        press(w, at(0.40), c);
        slide(w, at(0.85), false, c);
        release(w, at(0.85), c);
        w.render_frame(c);
        assert_eq!(range(c), Some(selected));
        assert!(f64::from(h.view.read(c).editor.animation().unwrap().edit_time) > selected.end);

        let view = h.view.read(c).editor.timeline_view().unwrap().time_range;
        let end_x = ((selected.end - view.0) / (view.1 - view.0)) as f32;
        press(w, at(end_x), c);
        slide(w, at(0.75), false, c);
        w.press("escape", c);
        release(w, at(0.75), c);
        assert_eq!(range(c), Some(selected));
        w.render_frame(c);

        press(w, at(end_x), c);
        slide(w, at(0.65), false, c);
        release(w, at(0.65), c);
        w.render_frame(c);
        let mut adjusted = range(c).unwrap();
        assert_eq!(adjusted.start, selected.start);
        assert!(adjusted.end > selected.end);
        assert!(adjusted.enabled);

        // Click while paused, then enable again without starting playback.
        for enabled in [false, true] {
            press(w, at(0.43), c);
            release(w, at(0.43), c);
            let v = h.view.read(c);
            assert!(!v.editor.animation().unwrap().playing);
            assert_eq!(
                range(c),
                Some(zipzi_editor_core::TimelineLoopRange {
                    enabled,
                    ..adjusted
                })
            );
            if !enabled {
                let end_x = ((adjusted.end - view.0) / (view.1 - view.0)) as f32;
                press(w, at(end_x), c);
                slide(w, at(0.70), false, c);
                release(w, at(0.70), c);
                let resized = range(c).unwrap();
                assert!(!resized.enabled && resized.end > adjusted.end);
                assert_eq!(resized.start, adjusted.start);
                adjusted = resized;
            }
        }
        w.render_frame(c);
        w.click("animation-play-pause", c);
        let presented = h.view.update(c, |v, _| {
            v.editor.advance_animation_clock_precise(0.25);
            let request = v.editor.animation_request().unwrap();
            let frame = AnimationEvaluator::default()
                .evaluate(&request, |_, _| true)
                .unwrap();
            assert!(v.editor.present_animation(frame.identity, frame.snapshot));
            v.editor.animation().unwrap().edit_time
        });

        press(w, at(0.43), c);
        assert!(h.view.read(c).editor.animation().unwrap().playing);
        release(w, at(0.43), c);
        let a = h.view.read(c).editor.animation().unwrap();
        assert!(a.playing && !a.is_range_playing());
        assert_eq!(a.time, presented);
        assert!(!range(c).unwrap().enabled);

        press(w, at(0.43), c);
        release(w, at(0.43), c);
        assert!(
            h.view
                .read(c)
                .editor
                .animation()
                .unwrap()
                .is_range_playing()
        );
        hover(w, at(0.43), c);
        assert!(w.find("timeline-loop-clear").visible());
        let clear = w.find("timeline-loop-clear").bounds().center();
        assert_eq!(clear.y, ticks.top() + px(6.));
        hover(w, clear, c);
        assert!(w.find("timeline-loop-clear").visible());
        hover(w, point(clear.x, ticks.center().y), c);
        assert!(w.try_find("timeline-loop-clear").is_none());
        hover(w, at(0.43), c);
        w.click("timeline-loop-clear", c);
        w.render_frame(c);
        assert!(range(c).is_none());
        let a = h.view.read(c).editor.animation().unwrap();
        assert!(a.playing && !a.is_range_playing());
        w.click("animation-play-pause", c);
        super::tool(w, "timeline-view", c);
        assert_eq!(
            h.view.read(c).editor.timeline_view().unwrap().mode,
            zipzi_editor_core::TimelineDisplayMode::Curves
        );
    })
    .unwrap();

    cx.simulate_window_resize(h.window.into(), size(px(760.), px(760.)));
    cx.run_until_parked();
    cx.update_window(h.window, |_, w, c| {
        w.render_frame(c);
        w.click("timeline-more", c);
        w.render_frame(c);
        assert!(w.find("timeline-menu-view").visible());
        assert!(w.try_find("timeline-menu-loop-range").is_none());
        w.press("escape", c);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, w, c| {
        w.render_frame(c);
        let lane = w.find("timeline-ruler").bounds();
        let at = |frame: f32| {
            point(
                lane.left() + lane.size.width * (frame / 120.),
                lane.center().y,
            )
        };
        press_with_modifiers(
            w,
            at(24.),
            Modifiers {
                shift: true,
                ..Default::default()
            },
            c,
        );
        slide(w, at(25.), true, c);
        release(w, at(25.), c);
        w.render_frame(c);
        let selected = range(c).unwrap();
        assert_eq!(selected.start, 24. / 60.);
        assert_eq!(selected.end, 25. / 60.);
        hover(w, point(at(24.5).x, lane.top() + px(4.)), c);
        w.click("timeline-loop-clear", c);
        w.render_frame(c);
        assert!(range(c).is_none());
        assert_eq!(
            h.view.read(c).editor.document().unwrap().history_lengths(),
            (0, 0)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn timeline_ruler_menu_creates_loop_from_playhead_and_clicked_frame(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    let click = |w: &mut Window, at: f32, button, c: &mut App| {
        let ruler = w.find("timeline-ruler").bounds();
        w.click_with_options(
            "timeline-ruler",
            ClickOptions::new()
                .with_button(button)
                .with_offset(point(ruler.size.width * at, ruler.size.height / 2.)),
            c,
        );
    };
    cx.update_window(h.window, |_, w, c| {
        w.click("workspace-animation", c);
        w.render_frame(c);
        w.click(("animation-motion", 0usize), c);
        w.render_frame(c);
        click(w, 0.2, MouseButton::Left, c);
    })
    .unwrap();

    for (at, end) in [(0.55, 66. / 60.), (0.2, 25. / 60.)] {
        cx.update_window(h.window, |_, w, c| {
            click(w, at, MouseButton::Right, c);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(h.window, |_, w, c| {
            w.render_frame(c);
            w.click("timeline-create-loop-range", c);
            let editor = &h.view.read(c).editor;
            let range = editor.timeline_view().unwrap().loop_range().unwrap();
            assert_eq!(range.start, 24. / 60.);
            assert_eq!(range.end, end);
            assert!(range.enabled);
            assert_eq!(editor.animation().unwrap().edit_time, 24. / 60.);
            assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
        })
        .unwrap();
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn timeline_range_clock_scales_wall_time_and_settles_before_a_speed_change(
    cx: &mut TestAppContext,
) {
    use std::time::{Duration, Instant};

    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, w, c| {
        w.click("workspace-animation", c);
        w.render_frame(c);
        w.click(("animation-motion", 0usize), c);
        w.render_frame(c);
        h.view.update(c, |view, c| {
            let panel = view.dock.as_ref().unwrap().timeline.read(c).id;
            let binding = view.editor.timeline_binding(panel).unwrap();
            let document = binding.document;
            view.editor
                .set_animation_preview_mode(
                    document,
                    zipzi_editor_core::AnimationPreviewMode::Editing,
                )
                .unwrap();
            view.editor
                .set_timeline_loop_range(binding, 0.25, 0.75)
                .unwrap();
            let start = Instant::now();
            for (speed, seconds) in [(0.5, 1.), (2., 0.25)] {
                view.editor.seek_timeline(document, 0.).unwrap();
                view.editor
                    .set_animation_playback_speed(document, speed)
                    .unwrap();
                view.editor.toggle_animation().unwrap();
                view.animation_anchor = Some((start, 0.));
                view.settle_animation_clock_at(start + Duration::from_secs_f64(seconds));
                let animation = view.editor.animation().unwrap();
                assert_eq!(animation.playback_clock_time(), 0.5);
                assert_eq!(animation.time, 0.25);
            }

            view.editor.seek_timeline(document, 0.).unwrap();
            view.editor
                .set_animation_playback_speed(document, 0.5)
                .unwrap();
            view.editor.toggle_animation().unwrap();
            view.animation_anchor = Some((start, 0.));
            let speed_change = start + Duration::from_millis(500);
            view.settle_animation_clock_at(speed_change);
            assert_eq!(view.editor.animation().unwrap().time, 0.5);
            view.editor
                .set_animation_playback_speed(document, 2.)
                .unwrap();
            view.animation_anchor = Some((
                speed_change,
                view.editor.animation().unwrap().playback_clock_time(),
            ));
            view.settle_animation_clock_at(speed_change + Duration::from_millis(125));
            assert_eq!(view.editor.animation().unwrap().time, 0.25);
            view.editor.pause_animation();
            view.animation_anchor = None;
        });
    })
    .unwrap();
}

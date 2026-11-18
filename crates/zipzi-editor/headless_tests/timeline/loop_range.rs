use super::super::slide_number_input::{press, release, slide};
use super::super::{document_fixture, setup_with_document};
use gpui_kit::{AppContext as _, TestAppContext, point, px, size, test::TestWindowExt};

#[gpui_kit::test]
fn timeline_loop_strip_and_transport_share_ranges_in_both_views(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    let range = |c: &gpui_kit::App| h.view.read(c).editor.timeline_view().unwrap().loop_range();
    cx.update_window(h.window, |_, w, c| {
        w.click("workspace-animation", c);
        w.render_frame(c);
        w.click(("animation-motion", 0usize), c);
        w.render_frame(c);
        let strip = w.find("timeline-loop-strip").bounds();
        let at = |fraction| point(strip.left() + strip.size.width * fraction, strip.center().y);
        // Drag backwards to create the interval.
        press(w, at(0.6), c);
        slide(w, at(0.25), false, c);
        assert!(range(c).is_none());
        release(w, at(0.25), c);
        w.render_frame(c);
        let selected = range(c).unwrap();
        assert!(selected.enabled);
        let state = h.view.read(c).editor.timeline_view().unwrap();
        let fraction = ((selected.end - state.time_range.0)
            / (state.time_range.1 - state.time_range.0)) as f32;
        press(w, at(fraction), c);
        slide(w, at(0.8), false, c);
        w.press("escape", c);
        release(w, at(0.8), c);
        assert_eq!(range(c), Some(selected));
        w.render_frame(c);

        super::tool(w, "timeline-loop-range", c);
        assert!(w.find("timeline-loop-controls").visible());
        w.click("timeline-loop-start", c);
        w.render_frame(c);
        w.press("secondary-a", c);
        w.input("1", c);
    })
    .unwrap();
    cx.update_window(h.window, |_, w, c| w.press("enter", c))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, w, c| {
        w.render_frame(c);
        let earlier = range(c).unwrap();
        let fps = h.view.read(c).editor.timeline_projection().unwrap().fps;
        assert_eq!(earlier.start, 1. / f64::from(fps));
        w.click("timeline-loop-end", c);
        w.render_frame(c);
        w.press("secondary-a", c);
        w.input("0", c);
        earlier
    })
    .map(|earlier| {
        cx.update_window(h.window, |_, w, c| w.press("enter", c))
            .unwrap();
        cx.run_until_parked();
        cx.update_window(h.window, |_, w, c| {
            w.render_frame(c);
            assert!(w.find("timeline-loop-error").visible());
            assert_eq!(range(c), Some(earlier));
            w.press("escape", c);
            w.render_frame(c);
            assert!(w.try_find("timeline-loop-error").is_none());

            w.click("timeline-loop-toggle", c);
            w.render_frame(c);
            assert!(!range(c).unwrap().enabled);
            let seeker = w.find("timeline-ruler").bounds();
            let outside = point(seeker.left() + seeker.size.width * 0.9, seeker.center().y);
            press(w, outside, c);
            release(w, outside, c);
            assert!(f64::from(h.view.read(c).editor.animation().unwrap().edit_time) > earlier.end);
            w.click("animation-play-pause", c);
            assert!(
                h.view
                    .read(c)
                    .editor
                    .animation()
                    .unwrap()
                    .playback_clock_time()
                    > earlier.end
            );
            w.click("animation-play-pause", c);
            super::tool(w, "timeline-view", c);
            assert_eq!(range(c).unwrap().end, earlier.end);
            assert_eq!(
                h.view.read(c).editor.timeline_view().unwrap().mode,
                zipzi_editor_core::TimelineDisplayMode::Curves
            );
            super::tool(w, "timeline-loop-range", c);
            assert!(w.try_find("timeline-loop-controls").is_none());
        })
        .unwrap();
    })
    .unwrap();

    cx.simulate_window_resize(h.window.into(), size(px(760.), px(760.)));
    cx.run_until_parked();
    cx.update_window(h.window, |_, w, c| {
        w.render_frame(c);
        assert!(w.try_find("timeline-loop-range").is_none());
        assert!(w.find("timeline-more").visible());
        w.click("timeline-more", c);
        w.render_frame(c);
        w.click("timeline-menu-loop-range", c);
        w.render_frame(c);
        assert!(w.find("timeline-loop-controls").visible());
        w.click("timeline-loop-clear", c);
        w.render_frame(c);
        let editor = &h.view.read(c).editor;
        assert!(editor.timeline_view().unwrap().loop_range().is_none());
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    })
    .unwrap();
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

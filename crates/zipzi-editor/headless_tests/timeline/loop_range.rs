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

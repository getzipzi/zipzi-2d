//! Native Metal, production Editor widgets and real pointer/focus routing.
use super::super::{
    fixture,
    harness::{Harness, update},
};
use super::common::save;
use crate::i18n;
use gpui_kit::{
    InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, point, px,
    test::TestWindowExt,
};
use zipzi_core::document::{MotionCurve, MotionEasingPreset, MotionFrameStep};
use zipzi_editor_core::{CommandId, Editor};

pub(super) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    for locale in ["en", "zh-CN", "ja"] {
        let mut h = Harness::new()?;
        let (document, assets) = fixture::animation_document()?;
        h._assets = assets;
        i18n::activate(Some(locale));
        update(&mut h, |view, window, cx| {
            view.replace_editor(Editor::from_document(document), cx);
            view.run_command(CommandId::AnimationWorkspace, window, cx);
        })?;
        h.cx.update_window(h.window, |_, w, c| {
            w.render_frame(c);
            w.click(("animation-motion", 0usize), c);
            w.render_frame(c);
        })?;
        h.capture()?;
        h.cx.update_window(h.window, |_, w, c| {
            let p = h.view.read(c).editor.timeline_projection().unwrap();
            let key = p.tracks.values().next().unwrap().keys[0].reference.clone();
            let position = h
                .view
                .read(c)
                .dock
                .as_ref()
                .unwrap()
                .timeline
                .read(c)
                .key_position(&key)
                .unwrap()
                + point(px(2.), px(0.));
            w.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                c,
            );
            w.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                c,
            );
        })?;
        h.capture()?;
        for (button, item) in [
            ("timeline-interpolation", "timeline-interpolation-bezier"),
            ("timeline-easing", "timeline-easing-cubic_out"),
            ("timeline-frame-step", "timeline-frame-step-3"),
        ] {
            h.cx.update_window(h.window, |_, w, c| {
                w.click(button, c);
                w.render_frame(c);
                w.click(item, c);
                w.render_frame(c);
            })?;
            h.capture()?;
        }
        h.view.read_with(&h.cx, |view, _| {
            let clip = view
                .editor
                .document()
                .unwrap()
                .motion_definition(&view.editor.timeline_projection().unwrap().clip_id)
                .unwrap();
            assert!(matches!(
                clip.tracks[0].segments[0].curve,
                MotionCurve::Bezier {
                    preset: Some(MotionEasingPreset::CubicOut),
                    ..
                }
            ));
            assert_eq!(
                clip.tracks[0].segments[0].frame_step,
                MotionFrameStep::EveryFrames { frames: 3 }
            );
            assert_eq!(view.editor.document().unwrap().history_lengths(), (3, 0));
        });
        if locale == "en" {
            let path = directory.join("curve-authoring.zipzi");
            if path.exists() {
                std::fs::remove_file(&path)?;
            }
            let task = update(&mut h, |view, _, _| {
                view.editor
                    .prepare_save(view.editor.active_id().unwrap(), &path, false)
            })??;
            let mut completion = task.run();
            update(&mut h, |view, _, _| {
                view.editor.finish_save(&mut completion)
            })??;
            let reopened = zipzi_editor_core::Document::open(&path)?;
            let expected = h.view.read_with(&h.cx, |view, _| {
                view.editor
                    .document()
                    .unwrap()
                    .motion_definition(&view.editor.timeline_projection().unwrap().clip_id)
                    .unwrap()
            });
            anyhow::ensure!(
                reopened.motion_definition(&expected.id).as_deref() == Some(expected.as_ref()),
                "native curve configuration changed on reopen"
            );
        }
        save(
            &h.capture()?,
            directory,
            &format!("curve-authoring-{locale}-overview.png"),
        )?;
        h.cx.update_window(h.window, |_, window, cx| {
            window.click("timeline-view", cx);
            window.render_frame(cx);
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("curve-authoring-{locale}-stairs.png"),
        )?;
        h.cx.update_window(h.window, |_, w, c| {
            w.click("timeline-easing", c);
            w.render_frame(c);
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("curve-authoring-{locale}-presets.png"),
        )?;
        h.cx.update_window(h.window, |_, w, c| {
            w.press("escape", c);
            w.render_frame(c);
        })?;
        h.capture()?;
        h.cx.update_window(h.window, |_, w, c| {
            use gpui_kit::component::dock::{DockPlacement, InsertTarget, PanelId};
            let dock = h.view.read(c).dock.as_ref().unwrap();
            let area = dock.area.clone();
            let panel = PanelId::from(dock.timeline.entity_id());
            let info = PanelId::from(dock.properties.entity_id());
            let node = area
                .read(c)
                .layout(DockPlacement::Center)
                .unwrap()
                .find_panel_node(info)
                .unwrap();
            area.update(c, |area, c| {
                area.move_panel(
                    panel,
                    InsertTarget::Tabs {
                        node,
                        ix: None,
                        activate: true,
                    },
                    w,
                    c,
                )
            });
            w.render_frame(c);
        })?;
        let narrow = h.capture()?;
        h.cx.update_window(h.window, |_, w, _| {
            assert!(w.find("timeline-curve-menu").visible());
            assert!(w.find("timeline-frame-step-number").visible());
        })?;
        save(
            &narrow,
            directory,
            &format!("curve-authoring-{locale}-narrow.png"),
        )?;
    }
    println!("Curve authoring native interaction, focus and Metal screenshots passed.");
    Ok(())
}

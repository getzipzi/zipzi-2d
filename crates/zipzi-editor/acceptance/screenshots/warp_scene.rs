//! Real Metal review of the Warp frame, selection scope and numeric dialog.
use super::super::harness::Harness;
use super::{common::save, properties_scene::show, toolbar_scene::toolbar_document};
use gpui_kit::{
    InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, point,
    px, test::TestWindowExt,
};
use zipzi_editor_core::{CommandId, Editor, EditorCommand, ObjectKind, ObjectRef};

pub(super) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    for locale in ["en", "zh-CN", "ja"] {
        let mut h = Harness::new()?;
        crate::i18n::activate(Some(locale));
        let (document, _assets) = toolbar_document()?;
        let object = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(502).to_string());
        h.cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |view, cx| {
                view.replace_editor(Editor::from_document(document), cx);
                view.editor
                    .execute(EditorCommand::SelectObject(object.clone()))
                    .unwrap();
                view.refresh_document(window, cx);
            });
        })?;
        h.capture()?;
        h.cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |view, cx| {
                view.run_command(CommandId::FitView, window, cx);
                // An empty Warp has no drawable bounds for Fit View.
                view.editor
                    .pan_by(zipzi_editor_core::ViewportPoint { x: -128., y: 128. });
                view.redraw(cx);
            })
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("warp-whole-{locale}.png"),
        )?;
        h.view.read_with(&h.cx, |view, _| {
            assert!(!view.editor.warp_fine());
            assert_eq!(view.editor.overlay().gizmo_lines.len(), 8);
        });
        h.cx.update_window(h.window, |_, window, cx| {
            window.activate_window();
            window.click("deformer-warp-fine", cx);
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("warp-division-points-{locale}.png"),
        )?;
        h.view.read_with(&h.cx, |view, _| {
            assert!(view.editor.warp_fine());
            assert!(view.editor.overlay().gizmo_lines.is_empty());
        });
        h.cx.update_window(h.window, |_, window, cx| {
            window.click("deformer-warp-bezier", cx);
        })?;
        h.capture()?;
        h.view.read_with(&h.cx, |view, _| {
            assert!(!view.editor.warp_fine());
            assert_eq!(view.editor.overlay().gizmo_lines.len(), 8);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        });
        h.cx.update_window(h.window, |_, window, cx| {
            window.click("deformer-warp-fine", cx);
        })?;
        h.capture()?;
        h.cx.update_window(h.window, |_, window, cx| {
            let points = h.view.read_with(cx, |view, _| {
                let frame = &view
                    .modeling_frame_for(view.viewport_slots[0])
                    .unwrap()
                    .canvas;
                let control = frame
                    .geometry
                    .controls
                    .iter()
                    .find(|c| c.object == object)
                    .unwrap();
                [
                    frame.transform.to_viewport(control.points[0]),
                    frame.transform.to_viewport(control.points[1]),
                ]
            });
            window.click_at("canvas-body", point(px(points[0].x), px(points[0].y)), cx);
            let position = window.find("canvas-body").bounds().origin
                + point(px(points[1].x), px(points[1].y));
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
        })?;
        h.view.read_with(&h.cx, |view, _| {
            assert_eq!(
                view.editor
                    .warp_transform_target(&object)
                    .unwrap()
                    .components
                    .len(),
                2
            )
        });
        save(
            &h.capture()?,
            directory,
            &format!("warp-selection-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, cx| {
            show(window, "property-warp-transform", cx);
            window.click("property-warp-transform", cx);
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("warp-transform-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, cx| {
            window.press("escape", cx);
            window.render_frame(cx);
            window.click("deformer-whole", cx);
            window.render_frame(cx);
            window.click("deformer-frame", cx);
            window.render_frame(cx);
            window.click("warp-frame-adjust", cx);
            window.render_frame(cx);
        })?;
        h.capture()?;
        h.cx.update_window(h.window, |_, window, cx| {
            let (start, center) = h.view.read_with(cx, |view, _| {
                let binding = view.editor.primary_viewport_binding().unwrap();
                let center = view.modeling_frame().unwrap().canvas.transform.to_viewport(
                    view.editor
                        .warp_frame_info(binding, &object)
                        .unwrap()
                        .frame
                        .center,
                );
                (
                    view.editor.overlay().handles.last().unwrap().position,
                    center,
                )
            });
            let origin = window.find("canvas-body").bounds().origin;
            let from = origin + point(px(start.x), px(start.y));
            let (s, c) = (-30f32.to_radians()).sin_cos();
            let (x, y) = (start.x - center.x, start.y - center.y);
            let end = origin + point(px(center.x + x * c - y * s), px(center.y + x * s + y * c));
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position: from,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseMoveEvent {
                    position: end,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position: end,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
        })?;
        h.view.read_with(&h.cx, |view, _| {
            let info = view
                .editor
                .warp_frame_info(view.editor.primary_viewport_binding().unwrap(), &object)
                .unwrap();
            assert!((info.frame.radians.to_degrees() - 30.).abs() < 0.01);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        });
        save(
            &h.capture()?,
            directory,
            &format!("warp-frame-adjust-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, cx| {
            window.click("deformer-frame", cx);
            window.render_frame(cx);
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("warp-frame-menu-{locale}.png"),
        )?;
    }
    Ok(())
}

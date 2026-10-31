//! Real Metal review of the Warp frame, selection scope and numeric dialog.
use super::super::harness::Harness;
use super::{common::save, properties_scene::show, toolbar_scene::toolbar_document};
use gpui_kit::{
    InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, point, px,
    test::TestWindowExt,
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
            })
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("warp-whole-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, cx| {
            window.activate_window();
            window.click("deformer-warp-mode", cx);
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
    }
    Ok(())
}

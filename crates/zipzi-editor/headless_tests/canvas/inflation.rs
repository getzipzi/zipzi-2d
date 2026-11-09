use super::super::setup;
use super::common::{advance_zoom, present_canvas};
use gpui_kit::{
    AppContext as _, BorrowAppContext as _, InputEvent as _, Modifiers, ModifiersChangedEvent,
    MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext, point, px, test::TestWindowExt,
};
use zipzi_editor_core::{BrushMode, BrushSelectionSettings, CanvasGestureKind, EditorCommand};

#[gpui_kit::test]
fn inflation_native_hold_runs_on_frames_shift_contracts_and_finish_or_escape_stops_it(
    cx: &mut TestAppContext,
) {
    for cancel in [false, true] {
        let harness = setup(cx);
        present_canvas(&harness, cx);
        let (start, before) = cx
            .update_window(harness.window, |_, window, cx| {
                window.activate_window();
                window.click("tool_brush_select", cx);
                window.click("brush-mode-inflate", cx);
                assert_eq!(
                    harness.view.read(cx).editor.brush_mode(),
                    BrushMode::Inflate
                );
                harness.view.update(cx, |view, _| {
                    view.editor
                        .execute(EditorCommand::SetBrushSelectionSettings(
                            BrushSelectionSettings {
                                diameter: 80.,
                                weight: 1.,
                                hardness: 1.,
                                ..Default::default()
                            },
                        ))
                        .unwrap();
                });
                let view = harness.view.read(cx);
                let p = view.editor.overlay().vertices[0].position;
                let before = view.editor.document().unwrap().selected_vertex_positions();
                let start =
                    window.find("canvas-body").bounds().origin + point(px(p.x - 20.), px(p.y));
                window.dispatch_event(
                    MouseDownEvent {
                        position: start,
                        button: MouseButton::Left,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                        first_mouse: false,
                    }
                    .to_platform_input(),
                    cx,
                );
                assert_eq!(
                    harness.view.read(cx).editor.overlay().active_gesture,
                    Some(CanvasGestureKind::BrushInflate)
                );
                (start, before)
            })
            .unwrap();
        for _ in 0..5 {
            advance_zoom(&harness, cx, 50);
        }
        let inflated = harness.view.read_with(cx, |view, _| {
            view.editor.document().unwrap().selected_vertex_positions()
        });
        assert!(inflated[0].1.x > before[0].1.x);
        cx.update_window(harness.window, |_, window, cx| {
            window.dispatch_event(
                ModifiersChangedEvent {
                    modifiers: Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                    capslock: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            assert!(
                harness
                    .view
                    .read(cx)
                    .editor
                    .overlay()
                    .selection_brush
                    .unwrap()
                    .subtract
            );
        })
        .unwrap();
        for _ in 0..3 {
            advance_zoom(&harness, cx, 50);
        }
        let contracted = harness.view.read_with(cx, |view, _| {
            view.editor.document().unwrap().selected_vertex_positions()
        });
        assert!(contracted[0].1.x < inflated[0].1.x);
        cx.update_window(harness.window, |_, window, cx| {
            if cancel {
                window.press("escape", cx);
            }
            window.dispatch_event(
                MouseUpEvent {
                    position: start,
                    button: MouseButton::Left,
                    modifiers: Modifiers {
                        shift: true,
                        ..Default::default()
                    },
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
        })
        .unwrap();
        advance_zoom(&harness, cx, 100);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor.document().unwrap().selected_vertex_positions(),
                if cancel { before } else { contracted }
            );
            assert_eq!(
                view.editor.document().unwrap().history_lengths(),
                (usize::from(!cancel), 0)
            );
            assert!(!view.editor.gesture_active());
            assert!(!view.editor.document().unwrap().has_pending_edit());
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        });
    }
}

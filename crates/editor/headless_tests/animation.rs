use super::{document_fixture, setup_with_document};
use editor_core::{CommandId, WorkspaceKind};
use gpui_kit::component::{WindowExt, dock::PanelId};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, KeyDownEvent, KeyUpEvent, Keystroke, TestAppContext,
};

#[gpui_kit::test]
fn motion_selection_space_button_and_menu_share_transport_and_repeat_guard(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        let focus = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .canvas
            .read(cx)
            .focus
            .clone();
        window.focus(&focus, cx);
        for held in [false, true, true] {
            window.dispatch_event(
                KeyDownEvent {
                    keystroke: Keystroke::parse("space").unwrap(),
                    is_held: held,
                    prefer_character_input: false,
                }
                .to_platform_input(),
                cx,
            );
        }
        window.dispatch_event(
            KeyUpEvent {
                keystroke: Keystroke::parse("space").unwrap(),
            }
            .to_platform_input(),
            cx,
        );
        assert!(harness.view.read(cx).editor.animation().unwrap().playing);
        window.render_frame(cx);
        window.click("animation-play-pause", cx);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::PlayPause, window, cx)
        });
        assert!(harness.view.read(cx).editor.animation().unwrap().playing);
        window.render_frame(cx);
        // Selecting a second registration of the same clip is a new recipe.
        window.click(("animation-motion", 1usize), cx);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        let entry = &harness
            .view
            .read(cx)
            .editor
            .animation()
            .unwrap()
            .motion
            .as_ref()
            .unwrap()
            .entry;
        assert_eq!(entry.as_ref().unwrap().1, 1);
        window.click("animation-play-pause", cx);
        window.render_frame(cx);
        window.click("animation-rewind", cx);
        assert_eq!(harness.view.read(cx).editor.animation().unwrap().time, 0.);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        assert!(!harness.view.read(cx).editor.document().unwrap().modified());
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn navigation_keeps_playing_modal_and_workspace_switch_pause_and_panels_reopen(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 2usize), cx);
        window.render_frame(cx);
        window.click("animation-play-pause", cx);
        harness.view.update(cx, |view, cx| {
            view.editor
                .pan_by(editor_core::CanvasPoint { x: 12., y: 5. });
            view.redraw_canvas(cx);
        });
        assert!(harness.view.read(cx).editor.animation().unwrap().playing);
        window.open_dialog(cx, |dialog, _, _| dialog.title("Modal"));
        window.render_frame(cx);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        window.press("space", cx);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        window.close_dialog(cx);
        window.render_frame(cx);
        window.click("animation-play-pause", cx);
        window.click("workspace-modeling", cx);
        window.render_frame(cx);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        assert_eq!(
            harness.view.read(cx).editor.workspace(),
            WorkspaceKind::Modeling
        );
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        let info = PanelId::from(
            harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .animation_info
                .entity_id(),
        );
        for visible in [false, true] {
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::AnimationInfo, window, cx)
            });
            window.render_frame(cx);
            let area = harness.view.read(cx).dock.as_ref().unwrap().area.read(cx);
            assert_eq!(area.panel(info).is_some(), visible);
            crate::docking::validate_layout(&area.dump(cx), WorkspaceKind::Animation).unwrap();
        }
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
    })
    .unwrap();
}

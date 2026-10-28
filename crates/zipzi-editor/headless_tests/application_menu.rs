//! Application-menu actions retain the input focus through nested menus.
use gpui_kit::component::input::{self, Input, InputState};
use gpui_kit::{
    AppContext as _, ClipboardItem, Context, Entity, Focusable, IntoElement, Menu, MenuItem,
    OsAction, Render, TestAppContext, Window, WindowOptions, div, prelude::*, test::TestWindowExt,
};

struct MenuHarness {
    input: Entity<InputState>,
    editor: Option<Entity<crate::EditorView>>,
}

impl Render for MenuHarness {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(editor) = &self.editor {
            editor.update(cx, |editor, cx| editor.refresh_menus(false, window, cx));
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(Input::new(&self.input))
            .child(crate::workspace::title_bar::application_menu_button(
                false, window, cx,
            ))
    }
}

#[gpui_kit::test]
fn application_menu_dispatches_text_actions_to_the_original_input(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::i18n::activate(Some("en"));
        cx.set_reduce_motion(true);
        cx.set_menus([Menu::new("Edit").items([
            MenuItem::os_action("Copy", input::Copy, OsAction::Copy),
            MenuItem::os_action("Cut", input::Cut, OsAction::Cut),
            MenuItem::os_action("Paste", input::Paste, OsAction::Paste),
            MenuItem::os_action("Undo", input::Undo, OsAction::Undo),
            MenuItem::os_action("Redo", input::Redo, OsAction::Redo),
        ])]);
    });
    let (window, view) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| MenuHarness {
                input: cx.new(|cx| InputState::new(window, cx).default_value("original")),
                editor: None,
            })
        })
        .unwrap()
    });
    let input = view.read_with(cx, |view, _| view.input.clone());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        input.focus_handle(cx).focus(window, cx);
        input.update(cx, |input, cx| input.select_all(window, cx));
        window.render_frame(cx);
        assert_eq!(input.read(cx).selected_value().as_ref(), "original");
    })
    .unwrap();
    cx.update(|cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("sentinel".into()));
    });
    for action in 0..5usize {
        cx.update_window(window, |_, window, cx| {
            if action == 2 {
                cx.write_to_clipboard(ClipboardItem::new_string("replacement".into()));
            }
            window.click("application-menu", cx);
            if action < 2 {
                assert_eq!(
                    input.read(cx).selected_value().as_ref(),
                    "original",
                    "opening the application menu must preserve the text selection"
                );
            }
            window.within("popup-menu").hover(0usize, cx);
            window
                .within("submenu")
                .within("popup-menu")
                .click(action, cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            assert!(input.focus_handle(cx).is_focused(window));
            let value = input.read(cx).value();
            match action {
                0 => assert_eq!(
                    cx.read_from_clipboard().unwrap().text().unwrap(),
                    "original"
                ),
                1 => assert_eq!(value.as_ref(), ""),
                2 | 4 => assert_eq!(value.as_ref(), "replacement"),
                3 => assert_ne!(value.as_ref(), "replacement"),
                _ => unreachable!(),
            }
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn application_menu_preserves_text_undo_when_editor_rebuilds_menus(cx: &mut TestAppContext) {
    let harness = super::setup(cx);
    let input = cx
        .update_window(harness.window, |_, window, cx| {
            let view = window.replace_root(cx, |window, cx| MenuHarness {
                input: cx.new(|cx| InputState::new(window, cx).default_value("original")),
                editor: Some(harness.view.clone()),
            });
            window.render_frame(cx);
            let input = view.read(cx).input.clone();
            input.focus_handle(cx).focus(window, cx);
            input.update(cx, |input, cx| input.select_all(window, cx));
            window.render_frame(cx);
            cx.write_to_clipboard(ClipboardItem::new_string("replacement".into()));
            window.dispatch_action(Box::new(input::Paste), cx);
            input
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(input.read(cx).value().as_ref(), "replacement");
        assert!(harness.view.read(cx).menu_state.text);
        let edit = cx
            .get_menus()
            .unwrap()
            .iter()
            .position(|menu| menu.name == "Edit")
            .unwrap();

        window.click("application-menu", cx);
        window.render_frame(cx);
        assert!(!harness.view.read(cx).menu_state.text);
        let menus = cx.get_menus().unwrap();
        let gpui_kit::OwnedMenuItem::Action { action, .. } = &menus[edit].items[0] else {
            panic!("Edit menu must start with Undo");
        };
        assert!(action.as_any().is::<crate::actions::RunCommand>());

        window.within("popup-menu").hover(edit, cx);
        window
            .within("submenu")
            .within("popup-menu")
            .click(0usize, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(input.read(cx).value().as_ref(), "original");
        assert!(input.focus_handle(cx).is_focused(window));
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0),
        );
    })
    .unwrap();
}

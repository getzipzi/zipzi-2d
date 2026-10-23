use crate::{
    settings::{Settings, UiScale},
    ui::compact,
};
use gpui_kit::component::{
    Theme, ThemeMode,
    button::Button,
    menu::{DropdownMenu, PopupMenu, PopupMenuItem},
};
use gpui_kit::{
    AppContext, Context, IntoElement, Render, TestAppContext, TestSupportExt, Window,
    WindowOptions, actions, div, prelude::*, px, test::TestWindowExt,
};

actions!(compact_popup_test, [NoAction]);

struct MenuHarness;
fn menu(menu: PopupMenu, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let attached = PopupMenu::build(window, cx, |menu, _, _| {
        menu.menu("Attached", Box::new(NoAction))
    });
    // Apply the density after constructing both forms of submenu.
    compact::menu(
        menu.item(PopupMenuItem::new("Disabled").disabled(true))
            .menu("Enabled", Box::new(NoAction))
            .submenu("Submenu", window, cx, |menu, window, cx| {
                menu.menu("Child", Box::new(NoAction)).submenu(
                    "Nested",
                    window,
                    cx,
                    |menu, _, _| menu.menu("Leaf", Box::new(NoAction)),
                )
            })
            .item(PopupMenuItem::submenu("Attached submenu", attached))
            .item(PopupMenuItem::element(|_, _| {
                div()
                    .id("multiline-project")
                    .test_support()
                    .flex()
                    .flex_col()
                    .child(div().h(compact::rem(18.)).child("项目 Project"))
                    .child(div().h(compact::rem(16.)).child("/models/project.kasane"))
            })),
    )
}

impl Render for MenuHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Button::new("menu-trigger")
            .label("Menu")
            .dropdown_menu(menu)
    }
}

#[gpui_kit::test]
fn menus_scale_inherit_density_and_keep_multiline_rows(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|_| MenuHarness)
        })
        .unwrap()
    });
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        for scale in UiScale::ALL {
            let expected = px(20. * scale.factor());
            cx.update_window(handle, |_, window, cx| {
                crate::theme::change(mode, cx);
                Theme::update(cx, |theme| {
                    theme.font_size = px(16. * scale.factor());
                    theme.radius = px(compact::RADIUS * scale.factor());
                });
                window.render_frame(cx);
                window.click("menu-trigger", cx);
                let row = window.within("popup-menu").find(1usize).bounds();
                assert!((row.size.height - expected).abs() <= px(0.5));
                assert!(window.find("multiline-project").bounds().size.height > expected);
                for index in [2usize, 3usize] {
                    window.hover(index, cx);
                    let child = window
                        .within("submenu")
                        .within("popup-menu")
                        .find(0usize)
                        .bounds();
                    assert!(
                        (child.size.height - expected).abs() <= px(0.5),
                        "submenu {index} at {scale:?}: {child:?}"
                    );
                }
                window.press("escape", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(handle, |_, window, cx| {
                window.render_frame(cx);
                assert!(window.try_find("popup-menu").is_none());
            })
            .unwrap();
        }
    }
}

#[gpui_kit::test]
fn compact_select_keeps_plain_and_searchable_selection_at_every_scale(cx: &mut TestAppContext) {
    let harness = super::setup(cx);
    cx.update(|cx| cx.set_reduce_motion(true));
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        for scale in UiScale::ALL {
            cx.update_window(harness.window, |_, window, cx| {
                cx.update_global::<Settings, _>(|settings, _| {
                    settings.set_ui_scale(scale).unwrap()
                });
                crate::theme::change(mode, cx);
                window.activate_window();
                window.render_frame(cx);
                window.click("new-parameter", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(harness.window, |_, window, cx| {
                window.render_frame(cx);
                let input_height = window.find("property-input-name").bounds().size.height;
                for id in ["property-select-kind", "property-select-group_id"] {
                    let bounds = window.find(id).bounds();
                    assert!(
                        (bounds.size.height - input_height).abs() <= px(0.5),
                        "{id} at {scale:?}: {bounds:?}"
                    );
                }
                window.click("property-select-kind", cx);
                window.press("down", cx);
                window.press("enter", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.update_window(harness.window, |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(
                    window.find("property-select-kind").value(),
                    Some("BlendShape")
                );
                window.within("property-select-group_id").click("input", cx);
                window.input("Ungrouped", cx);
            })
            .unwrap();
            cx.run_until_parked();
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(150));
            cx.run_until_parked();
            cx.update_window(harness.window, |_, window, cx| window.press("enter", cx))
                .unwrap();
            cx.run_until_parked();
            cx.update_window(harness.window, |_, window, cx| {
                window.render_frame(cx);
                assert_eq!(
                    window.find("property-select-group_id").value(),
                    Some("Ungrouped")
                );
                window.click("property-select-group_id", cx);
                window.press("escape", cx);
                assert_eq!(
                    window.find("property-select-group_id").value(),
                    Some("Ungrouped")
                );
                assert!(window.try_find("object-property-kind").is_some());
            })
            .unwrap();
            cx.run_until_parked();
        }
    }
}

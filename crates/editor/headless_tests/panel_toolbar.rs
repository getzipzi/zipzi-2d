use crate::panels::toolbar::{PanelToolbar, PanelToolbarPopover, PanelToolbarSearch};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Disableable, Selectable, WindowExt, button::Button, input::InputState};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext, Context, Entity, IntoElement, Pixels, Render, TestAppContext,
    TestSupportExt, Window, WindowOptions, div, prelude::*, px,
};

struct ToolbarHarness {
    width: Pixels,
    search: Entity<InputState>,
    conditions_active: bool,
    popup_open: bool,
    group_disabled: bool,
    clicks: usize,
}

impl Render for ToolbarHarness {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let on_create = cx.listener(|this, _, _, cx| {
            this.clicks += 1;
            cx.notify();
        });
        let group_disabled = self.group_disabled;
        div()
            .w(self.width)
            .flex()
            .flex_col()
            .child(
                PanelToolbar::new("actions-toolbar")
                    .action(move |compact| {
                        Button::new("create")
                            .icon(IconName::Plus)
                            .when(!compact, |button| button.label("New parameter"))
                            .accessibility_label("New parameter")
                            .tooltip("New parameter")
                            .on_click(on_create)
                    })
                    .action(move |compact| {
                        Button::new("groups")
                            .icon(IconName::FolderTree)
                            .label(if compact { "3" } else { "Groups 3" })
                            .accessibility_label("Show groups (3)")
                            .tooltip("Show groups (3)")
                            .disabled(group_disabled)
                    }),
            )
            .child(
                PanelToolbar::new("tree-toolbar")
                    .control(
                        PanelToolbarPopover::new(
                            "filter-popover",
                            Button::new("filter")
                                .icon(IconName::ListFilter)
                                .when(self.conditions_active, |button| button.label("2"))
                                .selected(self.conditions_active)
                                .accessibility_label("Object types")
                                .tooltip("Object types"),
                        )
                        .open(self.popup_open)
                        .on_open_change(cx.listener(|this, open, _, cx| {
                            this.popup_open = *open;
                            cx.notify();
                        }))
                        .content(|_, _, _| {
                            div()
                                .id("filter-content")
                                .test_support()
                                .w(px(160.))
                                .child("Object types")
                        }),
                    )
                    .search(
                        PanelToolbarSearch::new(&self.search)
                            .id("search")
                            .label("Search name or ID")
                            .reset_action(
                                "reset",
                                "Clear search and filters",
                                self.conditions_active,
                                cx.listener(|this, _, window, cx| {
                                    this.conditions_active = false;
                                    this.search
                                        .update(cx, |search, cx| search.set_value("", window, cx));
                                    cx.notify();
                                }),
                            ),
                    ),
            )
    }
}

fn setup(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<ToolbarHarness>) {
    crate::i18n::activate(Some("en"));
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
    });
    let result = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| ToolbarHarness {
                width: px(310.),
                search: cx.new(|cx| InputState::new(window, cx).placeholder("Search name or ID")),
                conditions_active: true,
                popup_open: false,
                group_disabled: false,
                clicks: 0,
            })
        })
        .unwrap()
    });
    cx.update_window(result.0, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    result
}

#[gpui_kit::test]
fn toolbar_resizes_without_losing_controls_input_or_accessible_names(cx: &mut TestAppContext) {
    let (handle, view) = setup(cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("search", cx);
        window.input("kept", cx);
    })
    .unwrap();
    let mut wide_create_width = px(0.);
    for width in [360., 310., 260., 220., 100., 310.] {
        cx.update_window(handle, |_, window, cx| {
            view.update(cx, |view, cx| {
                view.width = px(width);
                cx.notify();
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let actions = window.find("actions-toolbar").bounds();
            let tree = window.find("tree-toolbar").bounds();
            assert_eq!(actions.size.width, px(width));
            assert_eq!(actions.size.height, px(27.));
            assert_eq!(actions.size.height, tree.size.height);
            for (container, ids) in [
                (actions, &["create", "groups"][..]),
                (tree, &["filter", "search", "reset"][..]),
            ] {
                for id in ids {
                    let control = window.find(*id).bounds();
                    assert!(control.left() >= container.left(), "{id} at {width}px");
                    assert!(control.right() <= container.right(), "{id} at {width}px");
                    assert!(control.bottom() <= container.bottom(), "{id} at {width}px");
                }
            }
            assert!(window.find("filter").bounds().right() < window.find("search").bounds().left());
            assert_eq!(window.find("create").label(), Some("New parameter"));
            assert_eq!(window.find("groups").label(), Some("Show groups (3)"));
            assert_eq!(window.find("search").value(), Some("kept"));
            assert_eq!(window.find("search").focused(), Some(true));
            if width == 360. {
                wide_create_width = window.find("create").bounds().size.width;
            } else if width == 100. {
                assert!(window.find("create").bounds().size.width < wide_create_width);
            }
        })
        .unwrap();
    }
    cx.update_window(handle, |_, window, cx| {
        window.click("reset", cx);
        assert_eq!(window.find("search").value(), Some(""));
        assert!(window.try_find("reset").is_none());
        assert!(!view.read(cx).conditions_active);
    })
    .unwrap();
}

#[gpui_kit::test]
fn toolbar_preserves_input_arrows_and_forwards_popover_and_action_events(cx: &mut TestAppContext) {
    let (handle, view) = setup(cx);
    cx.update_window(handle, |_, window, cx| {
        window.click("search", cx);
        window.input("abc", cx);
        window.press("left", cx);
        window.input("x", cx);
        assert_eq!(window.find("search").value(), Some("abxc"));
        window.click("filter", cx);
        assert!(window.find("filter-content").visible());
        window.press("escape", cx);
        assert!(window.try_find("filter-content").is_none());
        // Pointer opening preserves the previously focused search on dismissal.
        assert_eq!(window.find("search").focused(), Some(true));
        window.click("create", cx);
        assert_eq!(view.read(cx).clicks, 1);
        // Enter via ordinary Tab order, then exercise the group's toolbar navigation.
        window.blur(cx);
        window.focus_next(cx);
        window.render_frame(cx);
        assert_eq!(window.find("create").focused(), Some(true));
        window.press("right", cx);
        assert_eq!(window.find("groups").focused(), Some(true));
        window.press("right", cx);
        assert_eq!(window.find("create").focused(), Some(true));
        window.press("enter", cx);
        assert_eq!(view.read(cx).clicks, 2);
        view.update(cx, |view, cx| {
            view.group_disabled = true;
            cx.notify();
        });
        window.render_frame(cx);
        window.press("right", cx);
        assert_eq!(window.find("create").focused(), Some(true));
        window.press("tab", cx);
        assert_eq!(window.find("filter").focused(), Some(true));
        window.press("enter", cx);
        assert!(window.find("filter-content").visible());
        window.press("escape", cx);
        assert!(window.try_find("filter-content").is_none());
        assert_eq!(window.find("filter").focused(), Some(true));
    })
    .unwrap();
}

#[gpui_kit::test]
fn parameters_toolbar_only_creates_parameters_and_modal_disables_creation(cx: &mut TestAppContext) {
    let harness = super::setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        assert!(
            window
                .within("parameters-panel-toolbar")
                .try_find("new-parameter")
                .is_some()
        );
        assert!(window.try_find("parameter-groups-toggle").is_none());
        harness.view.update(cx, |view, cx| {
            view.modal = true;
            cx.notify();
        });
        window.render_frame(cx);
        window.click("new-parameter", cx);
        assert!(!window.has_active_dialog(cx));
        harness.view.update(cx, |view, cx| {
            view.modal = false;
            cx.notify();
        });
        window.render_frame(cx);
        window.click("new-parameter", cx);
        assert!(window.has_active_dialog(cx));
    })
    .unwrap();
}

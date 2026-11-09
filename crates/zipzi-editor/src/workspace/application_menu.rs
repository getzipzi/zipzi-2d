use crate::{t, ui::compact};
use gpui_kit::base::actions::{SelectLeft, SelectRight};
use gpui_kit::component::{
    Disableable, Selectable,
    button::{Button, ButtonVariants},
    menu::PopupMenu,
};
use gpui_kit::{prelude::*, *};

/// Window-local state also lets the title bar replace the project selector.
pub(super) fn state(window: &mut Window, cx: &mut App) -> Entity<ApplicationMenu> {
    window.use_keyed_state("application-menu-state", cx, |_, cx| ApplicationMenu {
        focus: cx.focus_handle(),
        snapshot: None,
        active: None,
        subscription: None,
        modal: false,
    })
}

struct Snapshot {
    focus: Option<FocusHandle>,
    menus: Vec<OwnedMenu>,
}

pub(super) struct ApplicationMenu {
    focus: FocusHandle,
    snapshot: Option<Snapshot>,
    active: Option<(usize, Entity<PopupMenu>)>,
    subscription: Option<Subscription>,
    pub(super) modal: bool,
}

impl ApplicationMenu {
    pub(super) fn expanded(&self) -> bool {
        self.snapshot.is_some()
    }

    fn expand(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Save the actions as well as the focus before the Editor rebuilds its
        // menus for the menu bar's focus, particularly text Undo/Redo actions.
        self.snapshot = Some(Snapshot {
            focus: window.focused(cx),
            menus: cx.get_menus().unwrap_or_default(),
        });
        self.focus.focus(window, cx);
        cx.notify();
    }

    fn collapse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let menu_focused = self.focus.contains_focused(window, cx);
        self.active = None;
        self.subscription = None;
        if let Some(snapshot) = self.snapshot.take()
            && menu_focused
            && let Some(focus) = snapshot.focus
        {
            focus.focus(window, cx);
        }
        cx.notify();
    }

    fn open(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal
            || self
                .active
                .as_ref()
                .is_some_and(|(selected, _)| *selected == index)
        {
            return;
        }
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let Some(menu) = snapshot.menus.get(index) else {
            return;
        };
        let items = menu.items.clone();
        let focus = snapshot.focus.clone();
        // A switched dropdown must return to the bar, rather than to the
        // previous dropdown's focus handle after that dropdown is removed.
        self.focus.focus(window, cx);
        let popup = PopupMenu::build(window, cx, move |menu, window, cx| {
            let menu = compact::menu(menu);
            let menu = if let Some(focus) = focus {
                menu.action_context(focus)
            } else {
                menu
            };
            application_menu(menu, items, window, cx)
        });
        self.subscription =
            Some(
                cx.subscribe_in(&popup, window, |this, _, _: &DismissEvent, window, cx| {
                    this.collapse(window, cx);
                }),
            );
        popup.focus_handle(cx).focus(window, cx);
        self.active = Some((index, popup));
        cx.notify();
    }

    fn move_menu(&mut self, right: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(snapshot) = &self.snapshot else {
            return;
        };
        let count = snapshot.menus.len();
        if count == 0 {
            return;
        }
        let index = match &self.active {
            Some((index, _)) if right => (index + 1) % count,
            Some((index, _)) => (index + count - 1) % count,
            None if right => 0,
            None => count - 1,
        };
        self.open(index, window, cx);
    }
}

impl Render for ApplicationMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let expanded = self.expanded();
        let mut bar = div()
            .id("application-menu-bar")
            .test_support()
            .role(Role::MenuBar)
            .track_focus(&self.focus)
            .flex()
            .items_center()
            .min_w_0()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(|_, _, cx| cx.stop_propagation())
            .on_action(
                cx.listener(|this, _: &SelectLeft, window, cx| this.move_menu(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SelectRight, window, cx| this.move_menu(true, window, cx)),
            )
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if !this.expanded() {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "escape" => this.collapse(window, cx),
                    "down" | "right" if this.active.is_none() => this.open(0, window, cx),
                    "left" if this.active.is_none() => {
                        let last = this
                            .snapshot
                            .as_ref()
                            .unwrap()
                            .menus
                            .len()
                            .saturating_sub(1);
                        this.open(last, window, cx);
                    }
                    _ => return,
                }
                cx.stop_propagation();
                window.prevent_default();
            }))
            .when(expanded, |bar| {
                bar.on_mouse_down_out(cx.listener(|this, _, window, cx| {
                    // PopupMenu handles outside clicks while a dropdown is open.
                    if this.active.is_none() {
                        this.collapse(window, cx);
                    }
                }))
            })
            .when(!expanded, |bar| {
                bar.child(
                    compact::button(Button::new("application-menu"))
                        .ghost()
                        .icon(gpui_kit::assets::IconName::Menu)
                        .accessibility_label(t!("menu.application_menu"))
                        .tooltip(t!("menu.application_menu"))
                        .disabled(self.modal)
                        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                        .on_click(cx.listener(|this, _, window, cx| this.expand(window, cx))),
                )
            });

        if let Some(snapshot) = &self.snapshot {
            let mut items = div()
                .id("application-menu-items")
                .test_support()
                .flex()
                .items_center()
                .min_w_0()
                .overflow_x_scroll();
            for (index, menu) in snapshot.menus.iter().enumerate() {
                let active = self
                    .active
                    .as_ref()
                    .is_some_and(|(selected, _)| *selected == index);
                let label = if index == 0 && menu.name == t!("app.title").as_ref() {
                    SharedString::from("Zipzi")
                } else {
                    menu.name.clone()
                };
                items = items.child(
                    div()
                        .id(("application-menu-item", index))
                        .test_support()
                        .relative()
                        .flex_shrink_0()
                        // The compact trigger is replaced under the pointer.
                        // Wait for actual pointer movement over a menu heading.
                        .on_mouse_move(cx.listener(
                            move |this, event: &MouseMoveEvent, window, cx| {
                                if event.pressed_button.is_none() {
                                    this.open(index, window, cx);
                                }
                            },
                        ))
                        .child(
                            compact::button(Button::new(("application-menu-entry", index)))
                                .ghost()
                                .label(label)
                                .px(compact::rem(5.))
                                .selected(active)
                                .disabled(self.modal)
                                .on_mouse_down(MouseButton::Left, |_, window, _| {
                                    window.prevent_default()
                                })
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.open(index, window, cx)
                                })),
                        )
                        .when(active, |item| {
                            item.child(deferred(
                                anchored()
                                    .anchor(Anchor::TopLeft)
                                    .snap_to_window_with_margin(px(8.))
                                    .child(
                                        div()
                                            .occlude()
                                            .pt_1()
                                            .child(self.active.as_ref().unwrap().1.clone()),
                                    ),
                            ))
                        }),
                );
            }
            bar = bar.child(items);
        }
        bar
    }
}

fn application_menu(
    mut menu: PopupMenu,
    items: Vec<OwnedMenuItem>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    for item in items {
        menu = match item {
            OwnedMenuItem::Action {
                name,
                action,
                checked,
                disabled,
                ..
            } => menu.menu_with_check_and_disabled(name, checked, action, disabled),
            OwnedMenuItem::Separator => menu.separator(),
            OwnedMenuItem::Submenu(submenu) => {
                menu.submenu(submenu.name, window, cx, move |menu, window, cx| {
                    application_menu(compact::menu(menu), submenu.items.clone(), window, cx)
                })
            }
            OwnedMenuItem::SystemMenu(_) => menu,
        };
    }
    menu
}

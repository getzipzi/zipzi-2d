//! Regression coverage for the editor-owned Dock cache boundary.
use super::EditorTabGroupSkin;
use gpui_kit::TestSupportExt as _;
use gpui_kit::component::dock::{
    BasePanel, DockArea, DockAreaRenderer as _, DockLayout, DockSkin, Panel, PanelEvent, TabGroup,
    TabGroupRenderer as _, panel_handle,
};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyView, App, AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable,
    InputEvent as _, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Render, StyleRefinement, TestAppContext, WeakEntity, Window, WindowOptions, div, point,
    prelude::*, px, size,
};
use std::{cell::Cell, rc::Rc};

struct CachedChild {
    id: &'static str,
    renders: Rc<Cell<usize>>,
    value: usize,
}

impl Render for CachedChild {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div()
            .id(self.id)
            .test_support()
            .size_full()
            .child(format!("{}: {}", self.id, self.value))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.value += 1;
                    cx.notify();
                }),
            )
    }
}

struct RowContainer {
    children: [Entity<CachedChild>; 2],
    focus: FocusHandle,
    group: Option<WeakEntity<TabGroup>>,
}

impl EventEmitter<PanelEvent> for RowContainer {}
impl Focusable for RowContainer {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for RowContainer {
    fn panel_name(&self) -> &'static str {
        "cache-container"
    }
    fn on_added_to(&mut self, group: WeakEntity<TabGroup>, _: &mut Window, _: &mut Context<Self>) {
        self.group = Some(group);
    }
}
impl Panel for RowContainer {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Cache"
    }
}

impl Render for RowContainer {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("cache-container")
            .test_support()
            .size_full()
            .flex()
            .flex_col()
            .children(
                self.children
                    .iter()
                    .cloned()
                    .map(|child| child.cached(StyleRefinement::default().w_full().h(px(32.)))),
            )
    }
}

struct Composer {
    container: Entity<RowContainer>,
    weak: bool,
    renderer: EditorTabGroupSkin,
    _dock_area: Entity<DockArea>,
}

impl Render for Composer {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view: AnyView = self.container.clone().into();
        let view = if self.weak {
            view.downgrade().upgrade().unwrap()
        } else {
            view
        };
        let group = self
            .container
            .read(cx)
            .group
            .as_ref()
            .unwrap()
            .upgrade()
            .unwrap()
            .read(cx)
            .context(cx);
        div()
            .id("cache-composer")
            .test_support()
            .relative()
            .w(px(300.))
            .h(px(240.))
            .child(
                div()
                    .absolute()
                    .left(px(24.))
                    .top(px(18.))
                    .w(px(180.))
                    .h(px(96.))
                    .flex()
                    .flex_col()
                    .child(self.renderer.render_active_panel(view, &group, window, cx)),
            )
    }
}

#[gpui_kit::test]
fn editor_dock_preserves_row_caches_and_delegates_other_panel_caches(cx: &mut TestAppContext) {
    for (targeted, weak) in [(true, false), (true, true), (false, false)] {
        let counts = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
        let (window, composer) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                let children = ["cached-first", "cached-second"]
                    .into_iter()
                    .enumerate()
                    .map(|(index, id)| {
                        cx.new(|_| CachedChild {
                            id,
                            renders: counts[index].clone(),
                            value: 0,
                        })
                    })
                    .collect::<Vec<_>>();
                let container = cx.new(|cx| RowContainer {
                    children: [children[0].clone(), children[1].clone()],
                    focus: cx.focus_handle(),
                    group: None,
                });
                let mut component = None;
                let dock_area = cx.new(|cx| {
                    let skin = DockSkin::new(cx);
                    component = Some(skin.tab_group_renderer());
                    DockArea::new("cache-test", None, window, cx).with_renderer(skin)
                });
                dock_area.update(cx, |area, cx| {
                    area.set_center(
                        DockLayout::tabs().panel_view(panel_handle(container.clone()), cx),
                        window,
                        cx,
                    );
                });
                let row_cached_panel = if targeted {
                    container.entity_id()
                } else {
                    children[0].entity_id()
                };
                cx.new(|_| Composer {
                    container,
                    weak,
                    renderer: EditorTabGroupSkin {
                        component: component.unwrap(),
                        row_cached_panel,
                        area: dock_area.downgrade(),
                        scroll_handle: Default::default(),
                        last_displayed: Cell::new(None),
                    },
                    _dock_area: dock_area,
                })
            })
            .unwrap()
        });
        let before = cx
            .update_window(window, |_, window, cx| {
                window.draw(cx).clear(cx);
                let parent = window.find("cache-composer").bounds();
                let bounds = window.find("cache-container").bounds();
                assert_eq!(bounds.origin, parent.origin + point(px(24.), px(18.)));
                assert_eq!(bounds.size, size(px(180.), px(96.)));
                counts.each_ref().map(|count| count.get())
            })
            .unwrap();
        composer.update(cx, |_, cx| cx.notify());
        cx.update_window(window, |_, window, cx| {
            window.draw(cx).clear(cx);
            assert_eq!(counts.each_ref().map(|count| count.get()), before);
        })
        .unwrap();
        composer.update(cx, |composer, cx| {
            composer.container.read(cx).children[0]
                .clone()
                .update(cx, |child, cx| {
                    child.value += 1;
                    cx.notify();
                });
        });
        cx.update_window(window, |_, window, cx| {
            window.draw(cx).clear(cx);
            assert_eq!(counts[0].get(), before[0] + 1);
            assert_eq!(counts[1].get(), before[1] + usize::from(!targeted));
            // A reused child's mouse listener must remain live.
            let position = window.find("cached-first").bounds().center();
            window.dispatch_event(
                MouseMoveEvent {
                    position,
                    pressed_button: None,
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
        })
        .unwrap();
        cx.update_window(window, |_, window, cx| {
            let position = window.find("cached-first").bounds().center();
            window.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position,
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
        })
        .unwrap();
        cx.update_window(window, |_, window, cx| {
            window.draw(cx).clear(cx);
            let value = composer.read(cx).container.read(cx).children[0]
                .read(cx)
                .value;
            assert_eq!(value, 2);
            // A full refresh still rebuilds every child, including text caches.
            let before_refresh = counts.each_ref().map(|count| count.get());
            window.refresh();
            window.draw(cx).clear(cx);
            assert_eq!(counts[0].get(), before_refresh[0] + 1);
            assert_eq!(counts[1].get(), before_refresh[1] + 1);
        })
        .unwrap();
    }
}

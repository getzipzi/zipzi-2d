//! Regression coverage for Dock and virtual-list cache boundaries.
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

#[gpui_kit::test]
fn variable_list_resize_preserves_scroll_estimates_and_remeasures_visible_rows(
    cx: &mut TestAppContext,
) {
    struct VariableList {
        state: gpui_kit::ListState,
        width: f32,
        renders: Rc<Cell<usize>>,
        last: Rc<Cell<usize>>,
    }
    impl Render for VariableList {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let renders = self.renders.clone();
            let last = self.last.clone();
            let height = if self.width > 150. { 20. } else { 40. };
            div().w(px(self.width)).h(px(80.)).child(
                gpui_kit::list(self.state.clone(), move |index, _, _| {
                    renders.set(renders.get() + 1);
                    last.set(last.get().max(index));
                    div().w_full().h(px(height)).into_any_element()
                })
                .size_full(),
            )
        }
    }
    let state = gpui_kit::ListState::new(100, gpui_kit::ListAlignment::Top, px(0.))
        .with_uniform_item_height(px(20.));
    let renders = Rc::new(Cell::new(0));
    let last = Rc::new(Cell::new(0));
    let (window, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|_| VariableList {
                state: state.clone(),
                width: 200.,
                renders: renders.clone(),
                last: last.clone(),
            })
        })
        .unwrap()
    });
    cx.update_window(window, |_, w, cx| {
        w.draw(cx).clear(cx);
    })
    .unwrap();
    assert!(state.max_offset_for_scrollbar().y >= px(1900.));
    assert!(renders.get() < 10, "list measured offscreen rows");
    view.update(cx, |v, cx| {
        v.width = 100.;
        cx.notify();
    });
    renders.set(0);
    cx.update_window(window, |_, w, cx| {
        w.draw(cx).clear(cx);
    })
    .unwrap();
    assert!(state.max_offset_for_scrollbar().y >= px(1900.));
    assert!(renders.get() < 10, "resize measured offscreen rows");
    // The two visible rows must have been measured at their new 40px height.
    assert!(state.max_offset_for_scrollbar().y >= px(1960.));
    // Heights are estimates until visited, so a second scroll may compensate
    // for the final rows growing from 20px to 40px at the narrower width.
    for _ in 0..3 {
        cx.update_window(window, |_, w, cx| {
            w.dispatch_event(
                gpui_kit::ScrollWheelEvent {
                    position: point(px(30.), px(30.)),
                    delta: gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-100_000.))),
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
            w.draw(cx).clear(cx);
        })
        .unwrap();
        if last.get() == 99 {
            break;
        }
    }
    assert_eq!(
        last.get(),
        99,
        "scroll could not reach an unmeasured last row"
    );
}

struct WritingInput {
    options: Entity<bool>,
    renders: Rc<Cell<usize>>,
    value: bool,
}

impl Render for WritingInput {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        // Like Input::render configuring InputState: a write without notify,
        // even when the option's value is unchanged.
        self.options.update(cx, |option, _| *option = true);
        div().size_full().bg(if self.value {
            gpui_kit::rgb(0xff0000)
        } else {
            gpui_kit::rgb(0x0000ff)
        })
    }
}

struct VirtualCachedRow {
    input: Entity<WritingInput>,
    renders: Rc<Cell<usize>>,
}

impl Render for VirtualCachedRow {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().size_full().child(self.input.clone())
    }
}

struct CachedVirtualList {
    rows: Vec<Entity<VirtualCachedRow>>,
}

impl Render for CachedVirtualList {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let rows = self.rows.clone();
        div().size_full().child(
            gpui_kit::uniform_list("cached-virtual-rows", rows.len(), move |range, _, _| {
                range
                    .map(|index| {
                        rows[index]
                            .clone()
                            .cached(StyleRefinement::default().w_full().h(px(32.)))
                    })
                    .collect()
            })
            .w(px(240.))
            .h(px(96.)),
        )
    }
}

#[gpui_kit::test]
fn virtual_list_cached_rows_ignore_render_writes_but_refresh_notified_inputs(
    cx: &mut TestAppContext,
) {
    for retention in [true, false] {
        let row_counts = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
        let input_counts = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
        let (window, list) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                window.set_view_retention(retention);
                // Two inputs use the same options. The later row's render
                // writes a dependency of the earlier row without notifying it.
                let options = cx.new(|_| false);
                let rows = (0..2)
                    .map(|index| {
                        let input = cx.new(|_| WritingInput {
                            options: options.clone(),
                            renders: input_counts[index].clone(),
                            value: false,
                        });
                        cx.new(|_| VirtualCachedRow {
                            input,
                            renders: row_counts[index].clone(),
                        })
                    })
                    .collect();
                cx.new(|_| CachedVirtualList { rows })
            })
            .unwrap()
        });
        let draw = |cx: &mut TestAppContext| {
            cx.update_window(window, |_, window, cx| {
                window.draw(cx).clear(cx);
                window
                    .painted_quads()
                    .iter()
                    .map(|quad| format!("{:?} {:?}", quad.bounds, quad.background))
                    .collect::<Vec<_>>()
            })
            .unwrap()
        };
        let original = draw(cx);
        let rows_before = row_counts.each_ref().map(|count| count.get());
        let inputs_before = input_counts.each_ref().map(|count| count.get());
        assert!(rows_before.iter().all(|count| *count > 0));
        assert!(inputs_before.iter().all(|count| *count > 0));

        // uniform_list lays its cached items out as independent roots, without
        // a parent layout key. Their descendants' render writes must not turn
        // that explicit cache boundary into an automatically retained view.
        let options = list.read_with(cx, |list, cx| {
            list.rows[0].read(cx).input.read(cx).options.clone()
        });
        for _ in 0..4 {
            options.update(cx, |option, _| *option = true);
            list.update(cx, |_, cx| cx.notify());
            assert_eq!(draw(cx), original);
            assert_eq!(row_counts.each_ref().map(|count| count.get()), rows_before);
            assert_eq!(
                input_counts.each_ref().map(|count| count.get()),
                inputs_before
            );
        }

        let input = list.read_with(cx, |list, cx| list.rows[0].read(cx).input.clone());
        input.update(cx, |input, cx| {
            input.value = true;
            cx.notify();
        });
        let changed = draw(cx);
        assert_ne!(changed, original, "notified input must repaint");
        assert!(input_counts[0].get() > inputs_before[0]);
        assert_eq!(
            input_counts[1].get(),
            inputs_before[1],
            "sibling stays cached"
        );
        let after_change = input_counts.each_ref().map(|count| count.get());
        for _ in 0..4 {
            assert_eq!(draw(cx), changed);
            assert_eq!(
                input_counts.each_ref().map(|count| count.get()),
                after_change
            );
        }

        cx.update_window(window, |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        })
        .unwrap();
        assert!(input_counts[0].get() > after_change[0]);
        assert!(input_counts[1].get() > after_change[1]);
        assert_eq!(
            draw(cx),
            changed,
            "full refresh preserves the updated paint"
        );
    }
}

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
    for (retention, targeted, weak) in [
        (true, true, false),
        (true, true, true),
        (true, false, false),
        (false, true, false),
        (false, true, true),
        (false, false, false),
    ] {
        let counts = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
        let (window, composer) = cx.update(|cx| {
            gpui_kit::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
                window.set_view_retention(retention);
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
                        owner: WeakEntity::new_invalid(),
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
            // Unchanged siblings stay cached under either dock renderer.
            assert_eq!(
                counts[1].get(),
                before[1],
                "retention={retention}, targeted={targeted}, weak={weak}"
            );
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

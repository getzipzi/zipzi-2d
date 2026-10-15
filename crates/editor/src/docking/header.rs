//! Panel-owned controls rendered in the dock's shared first row.
//! A separate observed view keeps header state current without rebuilding the
//! dock or moving search/popover state out of the owning panel.
use gpui_kit::assets::IconName;
use gpui_kit::base::ElementExt;
use gpui_kit::component::{button::Button, popover::Popover};
use gpui_kit::{prelude::*, *};
use std::collections::HashMap;

pub(crate) trait PanelHeader: Render {
    fn render_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement;

    fn header_resized(&mut self, _: Pixels, _: &mut Context<Self>) {}

    fn header_filtered(&self, _: &App) -> bool {
        false
    }

    fn header_min_width(&self) -> Pixels {
        px(100.)
    }
}

pub(super) type PanelHeaders = HashMap<EntityId, AnyView>;

struct HeaderView<P: PanelHeader> {
    panel: Entity<P>,
    compact: bool,
    width: Pixels,
    _observe: Subscription,
}

impl<P: PanelHeader> Render for HeaderView<P> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let minimum = self.panel.read(cx).header_min_width();
        let filtered = self.panel.read(cx).header_filtered(cx);
        let weak = cx.weak_entity();
        let mut frame = div()
            .id("panel-header-view")
            .h_full()
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .on_prepaint(move |bounds, window, cx| {
                let width = bounds.size.width;
                let compact = width < minimum * crate::ui::compact::scale(cx);
                if weak.upgrade().is_some_and(|view| {
                    view.read(cx).compact != compact || view.read(cx).width != width
                }) {
                    let view = weak.clone();
                    window.defer(cx, move |_, cx| {
                        _ = view.update(cx, |this, cx| {
                            if this.width != width {
                                this.width = width;
                                this.panel
                                    .update(cx, |panel, cx| panel.header_resized(width, cx));
                            }
                            if this.compact != compact {
                                this.compact = compact;
                                cx.notify();
                            }
                        });
                    });
                }
            });
        if self.compact {
            let panel = self.panel.clone();
            frame = frame.child(crate::ui::compact::header_control(
                Popover::new("panel-header-overflow")
                    .trigger(
                        crate::ui::compact::header_button(crate::ui::compact::toggle(
                            Button::new("panel-header-tools"),
                            filtered,
                        ))
                        .icon(IconName::SlidersHorizontal)
                        .accessibility_label(crate::t!("dock.panel_tools").into_owned())
                        .tooltip(crate::t!("dock.panel_tools")),
                    )
                    .content(move |_, window, cx| {
                        div()
                            .w(crate::ui::compact::scaled(360., cx)
                                .min(window.viewport_size().width - px(24.)))
                            .child(panel.update(cx, |panel, cx| panel.render_header(window, cx)))
                    }),
            ));
        } else {
            frame = frame.child(
                self.panel
                    .update(cx, |panel, cx| panel.render_header(window, cx)),
            );
        }
        frame
    }
}

pub(super) fn register<P: PanelHeader>(
    headers: &mut PanelHeaders,
    panel: &Entity<P>,
    cx: &mut App,
) {
    let header = cx.new(|cx| HeaderView {
        panel: panel.clone(),
        compact: false,
        width: px(0.),
        _observe: cx.observe(panel, |_, _, cx| cx.notify()),
    });
    headers.insert(panel.entity_id(), header.into());
}

/// Canvas frames change much more often than their header controls. Observe
/// only the displayed state so a frame arrival does not schedule another draw.
pub(super) fn register_with_state<P: PanelHeader, S: PartialEq + 'static>(
    headers: &mut PanelHeaders,
    panel: &Entity<P>,
    window: &mut Window,
    cx: &mut App,
    state: fn(&Entity<P>, &mut Window, &mut App) -> S,
) {
    // The component root is installed after dock construction. Read window
    // state only once notifications arrive, when the window is ready.
    let mut previous = None;
    let header = cx.new(|cx| HeaderView {
        panel: panel.clone(),
        compact: false,
        width: px(0.),
        _observe: cx.observe_in(panel, window, move |_, panel, window, cx| {
            let next = Some(state(&panel, window, cx));
            if next != previous {
                previous = next;
                cx.notify();
            }
        }),
    });
    headers.insert(panel.entity_id(), header.into());
}

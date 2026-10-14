//! Panel-owned controls rendered in the dock's shared first row.
//! A separate observed view keeps header state current without rebuilding the
//! dock or moving search/popover state out of the owning panel.
use gpui_kit::assets::IconName;
use gpui_kit::base::ElementExt;
use gpui_kit::component::{
    Sizable,
    button::{Button, ButtonVariants},
    popover::Popover,
};
use gpui_kit::{prelude::*, *};
use std::collections::HashMap;

pub(crate) trait PanelHeader: Render {
    fn render_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement;

    fn header_resized(&mut self, _: Pixels, _: &mut Context<Self>) {}

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
                let compact = width < minimum;
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
            frame = frame.child(
                Popover::new("panel-header-overflow")
                    .trigger(
                        Button::new("panel-header-tools")
                            .xsmall()
                            .ghost()
                            .icon(IconName::SlidersHorizontal)
                            .accessibility_label(crate::t!("dock.panel_tools").into_owned())
                            .tooltip(crate::t!("dock.panel_tools")),
                    )
                    .content(move |_, window, cx| {
                        div()
                            .w(px(360.).min(window.viewport_size().width - px(24.)))
                            .child(panel.update(cx, |panel, cx| panel.render_header(window, cx)))
                    }),
            );
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

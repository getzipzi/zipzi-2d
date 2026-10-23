//! Shared panel chrome. Panel owners keep input state, commands and popover state.

use crate::{t, ui::compact};
use gpui_kit::assets::IconName;
use gpui_kit::base::ElementExt;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
    popover::{Popover, PopoverState},
    toolbar::{Toolbar, ToolbarGroup},
};
use gpui_kit::{prelude::*, *};

const COMPACT_WIDTH: Pixels = px(260.);

#[derive(gpui_kit::IntoElement)]
pub struct PanelToolbar {
    id: ElementId,
    actions: Vec<Box<dyn FnOnce(bool) -> Button>>,
    controls: Vec<AnyElement>,
    search: Option<PanelToolbarSearch>,
}

impl PanelToolbar {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            actions: Vec::new(),
            controls: Vec::new(),
            search: None,
        }
    }

    /// The factory receives whether the panel is narrow. Keep the button ID
    /// and accessible name stable when omitting its visible label.
    pub fn action(mut self, build: impl FnOnce(bool) -> Button + 'static) -> Self {
        self.actions.push(Box::new(build));
        self
    }

    pub fn control(mut self, control: impl IntoElement) -> Self {
        self.controls.push(control.into_any_element());
        self
    }

    pub fn search(mut self, search: PanelToolbarSearch) -> Self {
        self.search = Some(search);
        self
    }
}

impl RenderOnce for PanelToolbar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Only layout state lives here. Key it independently of Kit's toolbar
        // focus state and notify only when the width crosses the breakpoint.
        let compact = window.use_keyed_state((self.id.clone(), "compact"), cx, |_, _| true);
        let is_compact = *compact.read(cx);
        let mut bar = Toolbar::new((self.id.clone(), "bar"))
            .xsmall()
            .h_full()
            .p_0()
            .gap(crate::ui::compact::rem(2.))
            .w_full()
            .min_w_0();
        if !self.actions.is_empty() {
            let buttons = self
                .actions
                .into_iter()
                .map(|build| compact::header_button(build(is_compact)))
                .collect();
            let group = compact::group(
                (self.id.clone(), "actions"),
                t!("editor.panel_toolbar_actions"),
                Axis::Horizontal,
                buttons,
            );
            bar = bar.content(compact::header_control(group));
        }
        if !self.controls.is_empty() {
            let mut group = ToolbarGroup::new((self.id.clone(), "controls"))
                .label(t!("editor.panel_toolbar_view"))
                .gap(crate::ui::compact::rem(2.))
                .flex_shrink_0();
            for control in self.controls {
                group = group.content(control);
            }
            bar = bar.content(compact::header_control(group));
        }
        bar = match self.search {
            Some(search) => bar.content(search),
            None => bar.content(div().flex_1().min_w_0()),
        };
        div()
            .id(self.id)
            .test_support()
            .relative()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .h(crate::ui::compact::rem(compact::HEADER_CONTROL))
            .on_prepaint(move |bounds, window, cx| {
                let next = bounds.size.width < COMPACT_WIDTH * compact::scale(cx);
                if next != *compact.read(cx) {
                    let compact = compact.clone();
                    window.defer(cx, move |_, cx| {
                        compact.update(cx, |current, cx| {
                            if *current != next {
                                *current = next;
                                cx.notify();
                            }
                        });
                    });
                }
            })
            .child(bar)
    }
}

#[derive(gpui_kit::IntoElement)]
pub struct PanelToolbarSearch {
    input: Input,
    reset: Option<Button>,
    disabled: bool,
}

impl PanelToolbarSearch {
    pub fn new(state: &Entity<InputState>) -> Self {
        Self {
            input: Input::new(state).cleanable(true),
            reset: None,
            disabled: false,
        }
    }

    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.input = self.input.id(id);
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.input = self.input.aria_label(label);
        self
    }

    /// The owner decides whether reset clears only text or other conditions.
    pub fn reset_action(
        mut self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        active: bool,
        on_reset: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        if active {
            let label = label.into();
            self.reset = Some(
                Button::new(id)
                    .icon(IconName::RotateCcw)
                    .ghost()
                    .xsmall()
                    .accessibility_label(label.clone())
                    .tooltip(label)
                    .on_click(on_reset),
            );
        }
        self
    }
}

impl Disableable for PanelToolbarSearch {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for PanelToolbarSearch {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                compact::header_input(self.input)
                    .disabled(self.disabled)
                    .flex_1()
                    .min_w_0()
                    .prefix(
                        Icon::new(IconName::Search)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                    ),
            )
            .when_some(self.reset, |row, reset| {
                row.child(
                    compact::header_button(reset)
                        .disabled(self.disabled)
                        .flex_shrink_0(),
                )
            })
    }
}

/// A controlled Kit popover with a toolbar-sized trigger. The caller builds
/// the trigger's name, selected state and badge, and owns the popup contents.
#[derive(gpui_kit::IntoElement)]
pub struct PanelToolbarPopover(Popover);

impl PanelToolbarPopover {
    pub fn new(id: impl Into<ElementId>, trigger: Button) -> Self {
        Self(
            Popover::new(id)
                .p_2()
                .trigger(compact::header_button(trigger.prepare_for_toolbar())),
        )
    }

    pub fn open(mut self, open: bool) -> Self {
        self.0 = self.0.open(open);
        self
    }

    pub fn on_open_change(
        mut self,
        callback: impl Fn(&bool, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.0 = self.0.on_open_change(callback);
        self
    }

    pub fn content<F, E>(mut self, content: F) -> Self
    where
        E: IntoElement,
        F: Fn(&mut PopoverState, &mut Window, &mut Context<PopoverState>) -> E + 'static,
    {
        self.0 = self.0.content(content);
        self
    }
}

impl RenderOnce for PanelToolbarPopover {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.0
    }
}

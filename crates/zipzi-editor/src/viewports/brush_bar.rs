//! Brush controls with full-width numeric fields and a bounded settings popover.
use crate::{
    EditorView, t,
    ui::{
        color_picker::{ColorField, ColorPickerEvent, ColorPickerState},
        compact,
        slide_number_input::{
            SlideNumberChange, SlideNumberGesture, SlideNumberInput, SlideNumberOptions,
        },
    },
};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, IndexPath, Sizable,
    button::Button,
    input::{InputEvent, InputState},
    popover::Popover,
    select::{Select, SelectEvent, SelectItem, SelectState},
};
use gpui_kit::{prelude::*, *};
use zipzi_editor_core::{
    BrushMode, BrushSelectionSettings as Settings, BrushShape, EditorCommand, GlueBrushMode,
    ToolId, ViewportBinding, ViewportId,
};

const LABELS: [&str; 5] = [
    "editor.brush_selection_size",
    "editor.brush_selection_weight",
    "editor.brush_selection_hardness",
    "editor.brush_selection_angle",
    "editor.glue_weight_opacity",
];
const IDS: [&str; 5] = [
    "brush-selection-size",
    "brush-selection-weight",
    "brush-selection-hardness",
    "brush-selection-angle",
    "glue-weight-opacity",
];
#[derive(Clone)]
struct ShapeChoice {
    value: BrushShape,
    label: &'static str,
}
impl SelectItem for ShapeChoice {
    type Value = BrushShape;
    fn title(&self) -> SharedString {
        t!(self.label).into()
    }
    fn value(&self) -> &BrushShape {
        &self.value
    }
}
fn value(s: Settings, index: usize) -> f64 {
    (match index {
        0 => s.diameter,
        1 => s.weight * 100.,
        2 => s.hardness * 100.,
        3 => s.angle,
        _ => s.color[3] * 100.,
    }) as f64
}
fn options(s: Settings, index: usize, disabled: bool) -> SlideNumberOptions {
    SlideNumberOptions {
        value: value(s, index),
        min: match index {
            0 => 4.,
            3 => -180.,
            _ => 0.,
        },
        max: match index {
            0 => 1024.,
            3 => 180.,
            _ => 100.,
        },
        step: if index == 0 { 1. } else { 0.1 },
        drag_distance: None,
        disabled: disabled || (index == 3 && s.shape == BrushShape::Circle),
    }
}
fn rgba(c: [f32; 4]) -> Rgba {
    Rgba {
        r: c[0],
        g: c[1],
        b: c[2],
        a: c[3],
    }
}

pub(super) struct BrushSelectionBar {
    owner: WeakEntity<EditorView>,
    viewport: ViewportId,
    binding: Option<ViewportBinding>,
    tool: ToolId,
    inputs: [Entity<InputState>; 5],
    numbers: [Entity<SlideNumberInput>; 5],
    shape: Entity<SelectState<Vec<ShapeChoice>>>,
    color: Entity<ColorPickerState>,
    original: Option<Settings>,
    open: bool,
    locale: String,
    _subscriptions: Vec<Subscription>,
}
impl BrushSelectionBar {
    pub fn new(
        owner: WeakEntity<EditorView>,
        viewport: ViewportId,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| {
            let inputs = std::array::from_fn(|i| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .default_value(value(Settings::default(), i).to_string())
                })
            });
            let numbers = std::array::from_fn(|i| {
                cx.new(|cx| {
                    SlideNumberInput::new(
                        inputs[i].clone(),
                        t!(LABELS[i]).into_owned(),
                        options(Settings::default(), i, false),
                        window,
                        cx,
                    )
                    .suffix(match i {
                        0 => "px",
                        3 => "°",
                        _ => "%",
                    })
                    .format_sliding_value(|v| format!("{v:.1}"))
                    .xsmall()
                })
            });
            let shape = cx.new(|cx| {
                SelectState::new(
                    vec![
                        ShapeChoice {
                            value: BrushShape::Circle,
                            label: "editor.brush_shape_circle",
                        },
                        ShapeChoice {
                            value: BrushShape::Square,
                            label: "editor.brush_shape_square",
                        },
                        ShapeChoice {
                            value: BrushShape::Line,
                            label: "editor.brush_shape_line",
                        },
                    ],
                    Some(IndexPath::new(0)),
                    window,
                    cx,
                )
            });
            let color = cx
                .new(|cx| ColorPickerState::new(rgba(Settings::default().color), true, window, cx));
            let mut subscriptions = Vec::new();
            for i in 0..LABELS.len() {
                subscriptions.push(cx.subscribe_in(
                    &numbers[i],
                    window,
                    move |this: &mut Self, _, event: &SlideNumberChange, _, cx| {
                        this.apply_number(i, event.0, cx)
                    },
                ));
                subscriptions.push(cx.subscribe_in(
                    &numbers[i],
                    window,
                    |this: &mut Self, _, event: &SlideNumberGesture, _, cx| match event {
                        SlideNumberGesture::Begin => this.original = this.settings(cx),
                        SlideNumberGesture::Cancel => {
                            if let Some(s) = this.original.take() {
                                this.apply(s, cx);
                            }
                        }
                        SlideNumberGesture::End => this.original = None,
                    },
                ));
                subscriptions.push(cx.subscribe_in(
                    &inputs[i],
                    window,
                    move |this: &mut Self, _, event: &InputEvent, window, cx| {
                        if matches!(
                            event,
                            InputEvent::Change | InputEvent::Blur | InputEvent::PressEnter { .. }
                        ) {
                            if let Ok(v) = this.inputs[i].read(cx).value().parse::<f64>() {
                                this.apply_number(i, v, cx);
                            }
                            if !matches!(event, InputEvent::Change) {
                                this.sync(window, cx, true);
                            }
                        }
                    },
                ));
            }
            subscriptions.push(cx.subscribe(
                &shape,
                |this: &mut Self, _, _: &SelectEvent<Vec<ShapeChoice>>, cx| {
                    if let (Some(mut s), Some(shape)) = (
                        this.settings(cx),
                        this.shape.read(cx).selected_value().copied(),
                    ) {
                        s.shape = shape;
                        this.apply(s, cx);
                    }
                },
            ));
            subscriptions.push(cx.subscribe(
                &color,
                |this: &mut Self, _, event: &ColorPickerEvent, cx| match event {
                    ColorPickerEvent::Begin => this.original = this.settings(cx),
                    ColorPickerEvent::Preview(c) | ColorPickerEvent::Commit(c) => {
                        if let Some(mut s) = this.settings(cx) {
                            s.color = [c.r, c.g, c.b, c.a];
                            this.apply(s, cx);
                        }
                        if matches!(event, ColorPickerEvent::Commit(_)) {
                            this.original = None;
                        }
                    }
                    ColorPickerEvent::Cancel => {
                        if let Some(s) = this.original.take() {
                            this.apply(s, cx);
                        }
                    }
                },
            ));
            subscriptions.push(cx.observe(&color, |_, _, cx| cx.notify()));
            if let Some(owner) = owner.upgrade() {
                subscriptions.push(cx.observe_in(&owner, window, |this, _, window, cx| {
                    this.sync(window, cx, false);
                    cx.notify();
                }));
            }
            Self {
                owner,
                viewport,
                binding: None,
                tool: ToolId::Select,
                inputs,
                numbers,
                shape,
                color,
                original: None,
                open: false,
                locale: rust_i18n::locale().to_string(),
                _subscriptions: subscriptions,
            }
        })
    }
    fn settings(&self, cx: &App) -> Option<Settings> {
        let owner = self.owner.upgrade()?;
        let editor = &owner.read(cx).editor;
        Some(if editor.active_tool() == ToolId::Glue {
            editor.glue_brush_settings()
        } else {
            editor.brush_selection_settings()
        })
    }
    fn apply_number(&mut self, index: usize, v: f64, cx: &mut Context<Self>) {
        let Some(mut s) = self.settings(cx) else {
            return;
        };
        let o = options(s, index, false);
        if !v.is_finite() || !(o.min..=o.max).contains(&v) {
            return;
        }
        match index {
            0 => s.diameter = v as f32,
            1 => s.weight = v as f32 / 100.,
            2 => s.hardness = v as f32 / 100.,
            3 => s.angle = v as f32,
            _ => s.color[3] = v as f32 / 100.,
        }
        self.apply(s, cx);
    }
    fn apply(&mut self, s: Settings, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let state = owner.read(cx);
        if state.modal
            || state.editor.is_busy()
            || state.editor.gesture_active()
            || !matches!(
                state.editor.active_tool(),
                ToolId::BrushSelect | ToolId::Glue
            )
            || state.editor.active_tool() != self.tool
            || self.binding.is_none()
            || state.editor.viewport_binding(self.viewport) != self.binding
            || self.settings(cx) == Some(s)
        {
            return;
        }
        owner.update(cx, |owner, cx| {
            let command = if self.tool == ToolId::Glue {
                EditorCommand::SetGlueBrushSettings(s)
            } else {
                EditorCommand::SetBrushSelectionSettings(s)
            };
            if let Err(e) = owner.editor.execute(command) {
                owner.set_edit_error(e.to_string(), cx);
            }
            owner.notify_canvas(cx);
            cx.notify();
        });
        cx.notify();
    }
    fn mode_button(&self, target: BrushMode, disabled: bool, cx: &Context<Self>) -> Button {
        let (id, icon, label, hint) = match target {
            BrushMode::Deform => (
                "brush-mode-deform",
                IconName::Move,
                "editor.brush_mode_deform",
                "editor.brush_mode_deform_hint",
            ),
            BrushMode::Inflate => (
                "brush-mode-inflate",
                IconName::Maximize,
                "editor.brush_mode_inflate",
                "editor.brush_mode_inflate_hint",
            ),
            BrushMode::Select => unreachable!("painting is the default brush mode"),
        };
        let active = self
            .owner
            .upgrade()
            .is_some_and(|owner| owner.read(cx).editor.brush_mode() == target);
        compact::toggle(Button::new(id), active)
            .icon(icon)
            .map(|button| super::toolbar::attached_button(button, super::toolbar::SECONDARY, cx))
            .xsmall()
            .w(compact::rem(24.))
            .h(compact::rem(24.))
            .disabled(disabled)
            .accessibility_label(t!(label).into_owned())
            .tooltip(crate::i18n::shortcut_hint(hint))
            .on_click(cx.listener(move |this, _, _, cx| {
                let Some(owner) = this.owner.upgrade() else {
                    return;
                };
                owner.update(cx, |owner, cx| {
                    if owner.modal
                        || owner.editor.is_busy()
                        || owner.editor.gesture_active()
                        || owner.editor.active_tool() != ToolId::BrushSelect
                        || owner.editor.viewport_binding(this.viewport) != this.binding
                    {
                        return;
                    }
                    let mode = if owner.editor.brush_mode() == target {
                        BrushMode::Select
                    } else {
                        target
                    };
                    if let Err(e) = owner.editor.execute(EditorCommand::SetBrushMode(mode)) {
                        owner.set_edit_error(e.to_string(), cx);
                    }
                    owner.notify_canvas(cx);
                    crate::workspace::status_bar::StatusBar::sync(&owner.status_bar, owner, cx);
                    cx.notify();
                });
                cx.notify();
            }))
    }
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>, force: bool) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let state = owner.read(cx);
        let binding = state.editor.viewport_binding(self.viewport);
        let s = self.settings(cx).unwrap_or_default();
        let tool = state.editor.active_tool();
        let disabled = state.modal
            || state.editor.is_busy()
            || state.editor.gesture_active()
            || !matches!(tool, ToolId::BrushSelect | ToolId::Glue);
        let changed = self.binding != binding || self.tool != tool;
        self.binding = binding;
        self.tool = tool;
        if changed {
            self.original = None;
            self.open = false;
        }
        if disabled {
            self.open = false;
        }
        for i in 0..LABELS.len() {
            self.numbers[i].update(cx, |n, cx| {
                if changed {
                    n.reset_drag(cx);
                }
                let label = if i == 1 && tool == ToolId::Glue {
                    "editor.glue_brush_strength"
                } else {
                    LABELS[i]
                };
                n.configure(t!(label).into_owned(), options(s, i, disabled), cx);
            });
            if changed
                || force
                || (!self.inputs[i].read(cx).focus_handle(cx).is_focused(window)
                    && !self.numbers[i].read(cx).is_sliding())
            {
                let text = format!("{:.1}", value(s, i));
                if self.inputs[i].read(cx).value().as_ref() != text {
                    self.inputs[i].update(cx, |input, cx| input.set_value(text, window, cx));
                }
            }
        }
        let locale = rust_i18n::locale().to_string();
        if self.shape.read(cx).selected_value() != Some(&s.shape) || self.locale != locale {
            self.locale = locale;
            self.shape.update(cx, |shape, cx| {
                shape.set_selected_value(&s.shape, window, cx)
            });
        }
        self.color.update(cx, |color, cx| {
            color.sync(
                rgba(s.color),
                t!("editor.brush_selection_color").into_owned(),
                disabled,
                window,
                cx,
            )
        });
    }
    fn field(&self, index: usize, width: f32) -> AnyElement {
        div()
            .id(IDS[index])
            .test_support()
            .w(compact::rem(width))
            .flex_shrink_0()
            .child(self.numbers[index].clone())
            .into_any_element()
    }

    fn glue_follow(
        &self,
        controls: &zipzi_editor_core::GluePaintControls,
        disabled: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let mut row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(compact::rem(4.));
        for (id, side, label) in [
            (
                "glue-follow-both",
                None,
                t!("editor.glue_follow_both").into_owned(),
            ),
            (
                "glue-follow-a",
                Some(0),
                t!("editor.glue_follow", name = controls.meshes[0]).into_owned(),
            ),
            (
                "glue-follow-b",
                Some(1),
                t!("editor.glue_follow", name = controls.meshes[1]).into_owned(),
            ),
        ] {
            row = row.child(
                compact::toggle(
                    Button::new(id),
                    controls.uniform_weights
                        == Some(match side {
                            Some(0) => [0., 1.],
                            Some(1) => [1., 0.],
                            _ => [0.5, 0.5],
                        }),
                )
                .label(label.clone())
                .tooltip(label)
                .max_w(compact::rem(160.))
                .disabled(disabled || controls.diagnostic.is_some())
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some((owner, binding)) = this.owner.upgrade().zip(this.binding) {
                        owner.update(cx, |owner, cx| {
                            let result = owner.editor.set_glue_follow(binding, side);
                            owner.complete_edit(result, cx);
                        });
                    }
                })),
            );
        }
        row
    }

    fn glue_modes(&self, disabled: bool, cx: &Context<Self>) -> impl IntoElement {
        let owner = self.owner.upgrade();
        let active = owner
            .as_ref()
            .map(|owner| owner.read(cx).editor.glue_brush_mode());
        let mut buttons = div()
            .id("glue-brush-modes")
            .test_support()
            .flex()
            .flex_shrink_0()
            .gap_1();
        for (id, mode, label) in [
            (
                "glue-mode-ratio",
                GlueBrushMode::Ratio,
                "editor.glue_brush_ratio",
            ),
            ("glue-mode-a", GlueBrushMode::A, "editor.glue_brush_a"),
            ("glue-mode-b", GlueBrushMode::B, "editor.glue_brush_b"),
            (
                "glue-mode-reattach",
                GlueBrushMode::Reattach,
                "editor.glue_brush_reattach",
            ),
        ] {
            buttons = buttons.child(
                compact::toggle(Button::new(id), active == Some(mode))
                    .xsmall()
                    .label(t!(label).into_owned())
                    .disabled(disabled)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let Some(owner) = this.owner.upgrade() else {
                            return;
                        };
                        if owner.read(cx).editor.active_tool() != ToolId::Glue {
                            return;
                        }
                        owner.update(cx, |owner, cx| {
                            let result =
                                owner.editor.execute(EditorCommand::SetGlueBrushMode(mode));
                            owner.complete_edit(result, cx);
                        });
                        cx.notify();
                    })),
            );
        }
        buttons
    }
}
impl Render for BrushSelectionBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync(window, cx, false);
        let s = self.settings(cx).unwrap_or_default();
        let glue = self.tool == ToolId::Glue;
        let glue_context = glue
            .then(|| {
                self.owner
                    .upgrade()
                    .zip(self.binding)
                    .and_then(|(owner, binding)| owner.read(cx).editor.glue_paint_controls(binding))
            })
            .flatten();
        let disabled = self.owner.upgrade().is_none_or(|owner| {
            let v = owner.read(cx);
            v.modal || v.editor.is_busy() || v.editor.gesture_active()
        });
        let row = |label: &str, control: AnyElement| {
            div()
                .flex()
                .items_center()
                .gap(compact::rem(6.))
                .h(compact::rem(24.))
                .child(
                    div()
                        .w(compact::rem(70.))
                        .flex_shrink_0()
                        .child(t!(label).into_owned()),
                )
                .child(div().flex_1().min_w_0().child(control))
        };
        let shape = Select::new(&self.shape)
            .xsmall()
            .h(compact::rem(18.))
            .id("brush-selection-shape")
            .w_full()
            .disabled(disabled)
            .into_any_element();
        let details = div()
            .id("brush-selection-details")
            .test_support()
            .w(compact::rem(252.))
            .p(compact::rem(8.))
            .text_size(compact::rem(11.))
            .flex()
            .flex_col()
            .gap(compact::rem(4.))
            .child(row("editor.brush_selection_shape", shape))
            .child(row(LABELS[2], self.field(2, 100.)))
            .child(row(LABELS[3], self.field(3, 100.)))
            .when(glue, |details| {
                details
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(t!("editor.glue_paint_intro").into_owned()),
                    )
                    .child(self.glue_modes(disabled, cx))
                    .child(row("editor.glue_brush_strength", self.field(1, 100.)))
                    .child(row("editor.brush_size_short", self.field(0, 100.)))
                    .child(row(LABELS[4], self.field(4, 100.)))
            })
            .when(!glue, |details| {
                details.child(row(
                    "editor.brush_selection_color",
                    ColorField::new("brush-selection-color", &self.color).into_any_element(),
                ))
            });
        let popover = Popover::new("brush-selection-more")
            .open(self.open)
            // The picker is a separately hosted child popup. Its outside-click
            // handler owns dismissal while it is open, leaving these settings mounted.
            .overlay_closable(!self.color.read(cx).is_open())
            .p_0()
            .on_open_change(cx.listener(|this, open, _, cx| {
                this.open = *open;
                cx.notify();
            }))
            .trigger(
                compact::toggle(Button::new("brush-selection-more-button"), self.open)
                    .icon(IconName::Settings2)
                    .map(|button| {
                        super::toolbar::attached_button(button, super::toolbar::SECONDARY, cx)
                    })
                    .xsmall()
                    .when(!glue, |b| b.w(compact::rem(24.)))
                    .when(glue, |b| b.label(t!("editor.glue_advanced").into_owned()))
                    .h(compact::rem(24.))
                    .disabled(disabled)
                    .accessibility_label(
                        t!(if glue {
                            "editor.glue_advanced"
                        } else {
                            "editor.brush_selection_settings"
                        })
                        .into_owned(),
                    )
                    .tooltip(t!(if glue {
                        "editor.glue_advanced"
                    } else {
                        "editor.brush_selection_settings"
                    })),
            )
            .child(details);
        let labelled_field = |index, id: &'static str, key: &str, width| {
            div()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(compact::rem(4.))
                .h(compact::rem(24.))
                .child(div().id(id).test_support().child(t!(key).into_owned()))
                .child(self.field(index, width))
        };
        let description = glue_context
            .as_ref()
            .map(|glue| format!("{} · {} ↔ {}", glue.name, glue.meshes[0], glue.meshes[1]));
        let restriction = glue_context
            .as_ref()
            .and_then(|glue| glue.diagnostic.as_ref())
            .map(|status| {
                let key = match status.code.as_str() {
                    "GLUE_LOCKED" => "editor.glue_locked",
                    "GLUE_HIDDEN" => "editor.glue_hidden",
                    "GLUE_EMPTY" => "properties.glue_empty",
                    "GLUE_MESHES" => "editor.glue_invalid_meshes",
                    "GLUE_OCCUPIED" => "editor.glue_occupied",
                    "GLUE_ENDPOINT" => "editor.glue_unattached",
                    "INVALID_GLUE_WEIGHTS" => "editor.glue_invalid_weights",
                    _ => return status.message.clone(),
                };
                t!(key).into_owned()
            });
        let content = div()
            .id("brush-selection-controls")
            .test_support()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_center()
            .gap(compact::rem(4.))
            .px(compact::rem(2.))
            .max_w_full()
            .text_size(compact::rem(11.))
            .text_color(cx.theme().foreground)
            .when(glue, |content| {
                content
                    .justify_start()
                    .gap(compact::rem(7.))
                    .p(compact::rem(6.))
                    .max_w(compact::rem(440.))
                    .child(
                        div()
                            .id("glue-active-target")
                            .test_support()
                            .w_full()
                            .max_w_full()
                            .truncate()
                            .child(
                                description
                                    .unwrap_or_else(|| t!("editor.glue_choose").into_owned()),
                            ),
                    )
                    .when_some(glue_context.as_ref(), |content, controls| {
                        content.child(self.glue_follow(controls, disabled, cx))
                    })
                    .children(restriction.map(|reason| {
                        div()
                            .id("glue-brush-restriction")
                            .test_support()
                            .max_w_full()
                            .truncate()
                            .text_color(cx.theme().muted_foreground)
                            .child(reason)
                    }))
            })
            .when(!glue, |content| {
                content
                    .child(
                        self.mode_button(BrushMode::Deform, disabled, cx)
                            .flex_shrink_0(),
                    )
                    .child(
                        self.mode_button(BrushMode::Inflate, disabled, cx)
                            .flex_shrink_0(),
                    )
            })
            .when(!glue, |content| {
                content
                    .child(labelled_field(
                        1,
                        "brush-selection-weight-label",
                        if glue {
                            "editor.glue_brush_strength"
                        } else {
                            "editor.brush_weight_short"
                        },
                        80.,
                    ))
                    .child(labelled_field(
                        0,
                        "brush-selection-size-label",
                        "editor.brush_size_short",
                        88.,
                    ))
                    .child(
                        compact::button(Button::new("brush-selection-cycle-shape"))
                            .icon(match s.shape {
                                BrushShape::Circle => IconName::Circle,
                                BrushShape::Square => IconName::Square,
                                BrushShape::Line => IconName::Minus,
                            })
                            .map(|button| {
                                super::toolbar::attached_button(
                                    button,
                                    super::toolbar::SECONDARY,
                                    cx,
                                )
                            })
                            .xsmall()
                            .flex_shrink_0()
                            .w(compact::rem(24.))
                            .h(compact::rem(24.))
                            .disabled(disabled)
                            .accessibility_label(t!("editor.brush_selection_shape").into_owned())
                            .tooltip(t!("editor.brush_selection_shape"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(mut s) = this.settings(cx) {
                                    s.shape = match s.shape {
                                        BrushShape::Circle => BrushShape::Square,
                                        BrushShape::Square => BrushShape::Line,
                                        BrushShape::Line => BrushShape::Circle,
                                    };
                                    this.apply(s, cx);
                                }
                            })),
                    )
            })
            .child(div().flex_shrink_0().child(popover));
        let primary = super::toolbar::PRIMARY;
        div()
            .absolute()
            .top_0()
            .left(compact::rem(primary.button_size + 2. * primary.inset + 2.))
            .right_0()
            .child(super::toolbar::secondary_bar(
                "brush-selection-settings",
                content,
                cx,
            ))
    }
}

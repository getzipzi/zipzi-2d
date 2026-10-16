mod channel;
mod events;
mod keys;
mod numbers;
mod properties;
mod selection;

use super::*;
use crate::ui::slide_number_input::{
    SlideNumberChange, SlideNumberInput, SlideNumberOptions, format_one_decimal,
};
use gpui_kit::component::slider::{Slider, SliderEvent, SliderState, SliderValue};
use kasane_sdk::MotionInterpolation;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum Field {
    RowValue(TimelineRowId),
    KeyTime(TimelineKeyRef),
    KeyValue(TimelineKeyRef),
    EventTime(String),
    EventValue(String),
    Duration,
    Fps,
    FadeIn,
    FadeOut,
    Scale,
}
impl Field {
    fn value_format(&self) -> fn(f64) -> String {
        match self {
            Self::RowValue(_) | Self::KeyValue(_) => format_one_decimal,
            Self::Scale => |value| value.to_string(),
            _ => |value| (value as f32).to_string(),
        }
    }

    fn number_options(
        &self,
        value: f64,
        range: (f64, f64),
        fps: f32,
        disabled: bool,
    ) -> SlideNumberOptions {
        let (step, drag_distance) = match self {
            Self::RowValue(_) | Self::KeyValue(_) if range.1 - range.0 < 1e6 => (0.001, Some(400.)),
            Self::KeyTime(_) | Self::EventTime(_) => (1. / f64::from(fps), None),
            Self::Fps => (1., None),
            _ => (0.01, None),
        };
        SlideNumberOptions {
            value,
            min: range.0,
            max: range.1,
            step,
            drag_distance,
            disabled,
        }
    }

    pub(super) fn exists(&self, projection: &TimelineProjection) -> bool {
        match self {
            Self::RowValue(id) => projection.rows.iter().any(|row| &row.id == id),
            Self::KeyTime(key) | Self::KeyValue(key) => projection.key(key).is_some(),
            Self::EventTime(id) | Self::EventValue(id) => {
                projection.events.iter().any(|event| &event.id == id)
            }
            _ => true,
        }
    }
}
pub(super) struct FieldControl {
    input: Entity<InputState>,
    displayed: String,
    pub(super) number: Entity<SlideNumberInput>,
    _events: Vec<Subscription>,
}
pub(super) struct NumberEdit {
    pub token: TimelineCandidateIdentity,
    pub(super) field: Field,
    command: TimelineCommand,
    pub sliding: bool,
}
pub(super) struct ChannelSlider {
    field: Field,
    state: Entity<SliderState>,
    _events: Subscription,
}
impl Timeline {
    fn labeled_number(
        &mut self,
        label: impl Into<SharedString>,
        field: Field,
        value: f64,
        range: (f64, f64),
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(label.into()),
            )
            .child(self.number(field, value, range, disabled, window, cx))
            .into_any_element()
    }
    fn set_fade(
        &mut self,
        field: Field,
        value: Option<f32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editable(window, cx) {
            return;
        }
        self.finish_number(cx);
        if let Some(mut command) = self.template(&Field::Duration, cx) {
            if let TimelineCommand::Timing {
                fade_in, fade_out, ..
            } = &mut command
            {
                match field {
                    Field::FadeIn => *fade_in = value,
                    Field::FadeOut => *fade_out = value,
                    _ => return,
                }
            }
            self.execute(command, window, cx);
        }
    }
    pub(super) fn render_inspector(
        &mut self,
        projection: &TimelineProjection,
        state: &TimelineViewState,
        time: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let disabled = !self.editable(window, cx);
        let mut content = div()
            .id("timeline-inspector")
            .test_support()
            .w(compact::rem(INSPECTOR))
            .h_full()
            .flex_shrink_0()
            .border_l_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .when(self.compact, |content| {
                content
                    .absolute()
                    .right_0()
                    .top(px(32.))
                    .bottom_0()
                    .h_auto()
                    .occlude()
            })
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .when(self.compact, |content| {
                content.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(t!("timeline.inspector"))
                        .child(
                            compact::header_button(Button::new("timeline-inspector-close"))
                                .icon(IconName::Close)
                                .tooltip(t!("timeline.inspector"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.finish_number(cx);
                                    this.inspector_open = Some(false);
                                    cx.notify();
                                })),
                        ),
                )
            });
        if self.show_properties {
            let Some(owner) = self.owner.upgrade() else {
                return content.into_any_element();
            };
            let Some(clip) = owner
                .read(cx)
                .editor
                .timeline_edit()
                .map(|edit| edit.candidate.clip().clone())
                .or_else(|| {
                    owner
                        .read(cx)
                        .editor
                        .document()
                        .and_then(|d| d.motion_definition(&projection.clip_id))
                        .cloned()
                })
            else {
                return content.into_any_element();
            };
            let scale = owner
                .read(cx)
                .editor
                .document()
                .and_then(|d| d.motion_definition(&projection.clip_id))
                .map_or(1., |original| {
                    clip.duration as f64 / original.duration as f64
                });
            return self
                .render_clip_properties(content, clip, scale, disabled, window, cx)
                .into_any_element();
        }
        content = self.render_selection(content, projection, state, time, disabled, window, cx);
        if disabled {
            content = content.child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("timeline.pause_to_edit")),
            );
        }
        content.into_any_element()
    }

    pub(super) fn add_event(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let time = owner
            .read(cx)
            .editor
            .animation()
            .map_or(0., |a| a.edit_time as f64);
        let before = owner
            .read(cx)
            .editor
            .timeline_projection()
            .map(|p| p.events.iter().map(|e| e.id.clone()).collect::<Vec<_>>())
            .unwrap_or_default();
        self.execute(
            TimelineCommand::Event {
                id: None,
                time,
                value: String::new(),
            },
            window,
            cx,
        );
        let added = owner.read(cx).editor.timeline_projection().and_then(|p| {
            p.events
                .iter()
                .find(|e| !before.contains(&e.id))
                .map(|e| e.id.clone())
        });
        if let Some(id) = added {
            self.change_view(
                move |view| {
                    view.keys.clear();
                    view.events.clear();
                    view.events.insert(id);
                    view.channels.clear();
                    view.channels.insert(TimelineRowId::Events);
                    view.reveal_row = Some(TimelineRowId::Events);
                },
                cx,
            );
        }
    }
}

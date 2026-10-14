use super::*;
use crate::ui::slide_number_input::{SlideNumberChange, SlideNumberInput, SlideNumberOptions};
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
    fn template(&self, field: &Field, cx: &App) -> Option<TimelineCommand> {
        let owner = self.owner.upgrade()?;
        let view = owner.read(cx);
        let projection = view.editor.timeline_projection()?;
        let state = view.editor.timeline_view()?;
        let time = view.editor.animation()?.edit_time as f64;
        match field {
            Field::RowValue(id) => {
                let row = projection.rows.iter().find(|row| &row.id == id)?;
                let selected_key = state
                    .keys
                    .iter()
                    .find(|key| {
                        row.keys.contains(key)
                            && projection.key(key).is_some_and(|key| key.time == time)
                    })
                    .cloned();
                Some(TimelineCommand::WriteValue {
                    channel: row.channel.clone()?,
                    track_id: row.track_id.clone(),
                    selected_key,
                    value: 0.,
                })
            }
            Field::KeyTime(key) | Field::KeyValue(key) => {
                let point = projection.key(key)?;
                Some(TimelineCommand::SetKey {
                    key: key.clone(),
                    time: point.time,
                    value: point.value,
                })
            }
            Field::EventTime(id) | Field::EventValue(id) => {
                let event = projection.events.iter().find(|event| &event.id == id)?;
                Some(TimelineCommand::Event {
                    id: Some(id.clone()),
                    time: event.time as f64,
                    value: event.value.clone(),
                })
            }
            Field::Scale => Some(TimelineCommand::ScaleTime(
                view.editor
                    .document()?
                    .motion_definition(&projection.clip_id)?
                    .duration as f64,
            )),
            _ => {
                let clip = view
                    .editor
                    .document()?
                    .motion_definition(&projection.clip_id)?;
                Some(TimelineCommand::Timing {
                    duration: clip.duration,
                    fps: clip.fps,
                    looping: clip.looping,
                    fade_in: clip.fade_in,
                    fade_out: clip.fade_out,
                })
            }
        }
    }
    fn ensure_field(
        &mut self,
        field: &Field,
        value: String,
        range: (f64, f64),
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let fps = self
            .owner
            .upgrade()
            .and_then(|owner| owner.read(cx).editor.timeline_projection())
            .map_or(30., |projection| projection.fps);
        if !self.fields.contains_key(field) {
            let input = cx.new(|cx| InputState::new(window, cx).default_value(value.clone()));
            let number = cx.new(|cx| {
                SlideNumberInput::new(
                    input.clone(),
                    t!("timeline.value").into_owned(),
                    field.number_options(value.parse().unwrap_or(0.), range, fps, disabled),
                    window,
                    cx,
                )
            });
            let slide_field = field.clone();
            let slide = cx.subscribe_in(
                &number,
                window,
                move |this, source, event: &SlideNumberChange, window, cx| {
                    if this
                        .fields
                        .get(&slide_field)
                        .is_some_and(|control| &control.number == source)
                        && source.read(cx).is_sliding()
                        && this.editable(window, cx)
                    {
                        this.number_change(slide_field.clone(), event.0.to_string(), true, cx);
                    }
                },
            );
            let text_field = field.clone();
            let text = cx.subscribe_in(
                &input,
                window,
                move |this, source, event: &InputEvent, window, cx| {
                    if this
                        .fields
                        .get(&text_field)
                        .is_none_or(|control| &control.input != source)
                        || !this.allowed(window, cx)
                    {
                        return;
                    }
                    match event {
                        InputEvent::Change
                            if source.read(cx).focus_handle(cx).is_focused(window)
                                && this.editable(window, cx) =>
                        {
                            let value = source.read(cx).value().to_string();
                            let active = this
                                .number_edit
                                .as_ref()
                                .is_some_and(|edit| edit.field == text_field);
                            let displayed = &this.fields[&text_field].displayed;
                            let unchanged = if matches!(text_field, Field::EventValue(_)) {
                                &value == displayed
                            } else {
                                value
                                    .parse::<f64>()
                                    .ok()
                                    .zip(displayed.parse::<f64>().ok())
                                    .is_some_and(|(a, b)| {
                                        if matches!(text_field, Field::Scale) {
                                            a == b
                                        } else {
                                            a as f32 == b as f32
                                        }
                                    })
                            };
                            if !active && unchanged {
                                return;
                            }
                            this.number_change(
                                text_field.clone(),
                                source.read(cx).value().to_string(),
                                false,
                                cx,
                            );
                        }
                        InputEvent::PressEnter { .. } | InputEvent::Blur => {
                            if this
                                .number_edit
                                .as_ref()
                                .is_some_and(|edit| edit.field == text_field)
                            {
                                this.finish_number(cx);
                            }
                        }
                        _ => {}
                    }
                },
            );
            let focus = input.read(cx).focus_handle(cx);
            let blur_field = field.clone();
            let blur = cx.on_blur(&focus, window, move |this, _, cx| {
                if this
                    .number_edit
                    .as_ref()
                    .is_some_and(|edit| edit.field == blur_field)
                {
                    this.finish_number(cx);
                }
            });
            self.fields.insert(
                field.clone(),
                FieldControl {
                    input,
                    displayed: value.clone(),
                    number,
                    _events: vec![slide, text, blur],
                },
            );
        }
        let editing = self
            .number_edit
            .as_ref()
            .is_some_and(|edit| &edit.field == field);
        let control = self.fields.get_mut(field).unwrap();
        if !editing && !control.input.read(cx).focus_handle(cx).is_focused(window) {
            control.displayed = value.clone();
        }
        if !editing
            && !control.input.read(cx).focus_handle(cx).is_focused(window)
            && control.input.read(cx).value().as_ref() != value
        {
            control
                .input
                .update(cx, |input, cx| input.set_value(value.clone(), window, cx));
        }
        control.number.update(cx, |number, cx| {
            number.configure(
                t!("timeline.value").into_owned(),
                field.number_options(value.parse().unwrap_or(0.), range, fps, disabled),
                cx,
            )
        });
    }
    pub(super) fn number(
        &mut self,
        field: Field,
        value: f64,
        range: (f64, f64),
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = if matches!(field, Field::Scale) {
            value.to_string()
        } else {
            (value as f32).to_string()
        };
        self.ensure_field(&field, text, range, disabled, window, cx);
        div()
            .id(SharedString::from(format!("timeline-number-{field:?}")))
            .test_support()
            .w_full()
            .child(self.fields[&field].number.clone())
            .into_any_element()
    }
    fn number_change(
        &mut self,
        field: Field,
        value: String,
        sliding: bool,
        cx: &mut Context<Self>,
    ) {
        let numeric = if matches!(field, Field::EventValue(_)) {
            None
        } else {
            match value.parse::<f64>() {
                Ok(value) if value.is_finite() => Some(value),
                _ => {
                    if !sliding {
                        self.cancel_number_candidate(&field, cx);
                    }
                    self.error = Some(t!("timeline.invalid_number").into_owned());
                    cx.notify();
                    return;
                }
            }
        };
        if self
            .number_edit
            .as_ref()
            .is_some_and(|edit| edit.field != field)
        {
            self.finish_number(cx);
        }
        if self.number_edit.is_none() {
            let Some(command) = self.template(&field, cx) else {
                return;
            };
            let Some(target) = self.target(cx) else {
                return;
            };
            let token = self
                .owner
                .update(cx, |owner, cx| {
                    let token = owner
                        .editor
                        .begin_timeline_edit(target.document, "Edit motion value");
                    owner.redraw(cx);
                    token
                })
                .ok()
                .and_then(Result::ok);
            let Some(token) = token else { return };
            self.number_edit = Some(NumberEdit {
                token,
                field: field.clone(),
                command,
                sliding,
            });
        }
        let edit = self.number_edit.as_mut().unwrap();
        edit.sliding |= sliding;
        let mut command = edit.command.clone();
        match (&field, &mut command) {
            (Field::RowValue(_), TimelineCommand::WriteValue { value, .. }) => {
                *value = numeric.unwrap() as f32
            }
            (Field::KeyTime(_), TimelineCommand::SetKey { time, .. }) => *time = numeric.unwrap(),
            (Field::KeyValue(_), TimelineCommand::SetKey { value, .. }) => {
                *value = numeric.unwrap() as f32
            }
            (Field::EventTime(_), TimelineCommand::Event { time, .. }) => *time = numeric.unwrap(),
            (Field::EventValue(_), TimelineCommand::Event { value: text, .. }) => *text = value,
            (Field::Scale, TimelineCommand::ScaleTime(duration)) => *duration *= numeric.unwrap(),
            (Field::Duration, TimelineCommand::Timing { duration, .. }) => {
                *duration = numeric.unwrap() as f32
            }
            (Field::Fps, TimelineCommand::Timing { fps, .. }) => *fps = numeric.unwrap() as f32,
            (Field::FadeIn, TimelineCommand::Timing { fade_in, .. }) => {
                *fade_in = Some(numeric.unwrap() as f32)
            }
            (Field::FadeOut, TimelineCommand::Timing { fade_out, .. }) => {
                *fade_out = Some(numeric.unwrap() as f32)
            }
            _ => return,
        }
        let token = edit.token;
        self.update_candidate(token, command, cx);
        if !sliding && self.error.is_some() {
            self.cancel_number_candidate(&field, cx);
        }
    }
    // Invalid text must not leave a previous valid candidate available to Enter,
    // blur or Save. Keep the text focused so the user can correct it. Pointer
    // drags instead retain their last legal candidate when hitting a boundary.
    fn cancel_number_candidate(&mut self, field: &Field, cx: &mut Context<Self>) {
        if self
            .number_edit
            .as_ref()
            .is_some_and(|edit| &edit.field == field)
        {
            self.number_edit = None;
            let _ = self.owner.update(cx, |owner, cx| {
                owner.editor.cancel_timeline_edit();
                owner.redraw(cx);
            });
        }
    }
    pub(super) fn finish_number_outside(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let finish = self.number_edit.as_ref().is_some_and(|edit| {
            !edit.sliding
                && self
                    .fields
                    .get(&edit.field)
                    .is_none_or(|field| !field.input.read(cx).input_bounds().contains(&position))
        });
        if finish {
            self.finish_number(cx);
        }
    }
    pub(super) fn finish_number(&mut self, cx: &mut Context<Self>) {
        if let Some(edit) = self.number_edit.take() {
            self.finish_token(edit.token, cx);
        }
    }
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
            .w(px(INSPECTOR))
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
                            Button::new("timeline-inspector-close")
                                .small()
                                .ghost()
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
            content = content.child(t!("timeline.properties"));
            for (label, field, value, range) in [
                (
                    t!("timeline.duration"),
                    Field::Duration,
                    clip.duration as f64,
                    (0.001, 1e6),
                ),
                (
                    t!("timeline.fps"),
                    Field::Fps,
                    clip.fps as f64,
                    (0.001, 1e5),
                ),
                (
                    t!("timeline.fade_in"),
                    Field::FadeIn,
                    clip.fade_in.unwrap_or(0.) as f64,
                    (0., 1e6),
                ),
                (
                    t!("timeline.fade_out"),
                    Field::FadeOut,
                    clip.fade_out.unwrap_or(0.) as f64,
                    (0., 1e6),
                ),
                (
                    t!("timeline.scale_time"),
                    Field::Scale,
                    scale,
                    (0.001, 1000.),
                ),
            ] {
                let fade = match field {
                    Field::FadeIn => Some(clip.fade_in),
                    Field::FadeOut => Some(clip.fade_out),
                    _ => None,
                };
                if fade == Some(None) {
                    let field = field.clone();
                    content = content
                        .child(
                            div()
                                .text_color(cx.theme().muted_foreground)
                                .child(label.into_owned()),
                        )
                        .child(
                            Button::new(SharedString::from(format!(
                                "timeline-fade-custom-{field:?}"
                            )))
                            .small()
                            .ghost()
                            .label(t!("timeline.custom_fade"))
                            .tooltip(t!("timeline.custom_fade_hint"))
                            .disabled(disabled)
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.set_fade(field.clone(), Some(0.), window, cx)
                                },
                            )),
                        );
                } else {
                    content = content.child(self.labeled_number(
                        label.into_owned(),
                        field.clone(),
                        value,
                        range,
                        disabled,
                        window,
                        cx,
                    ));
                    if fade.is_some() {
                        content = content.child(
                            Button::new(SharedString::from(format!(
                                "timeline-fade-default-{field:?}"
                            )))
                            .small()
                            .ghost()
                            .label(t!("workspace.resource_default"))
                            .disabled(disabled)
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.set_fade(field.clone(), None, window, cx)
                                },
                            )),
                        );
                    }
                }
            }
            content = content.child(
                Button::new("timeline-asset-loop")
                    .small()
                    .ghost()
                    .label(t!("timeline.asset_loop"))
                    .selected(clip.looping)
                    .disabled(disabled)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.finish_number(cx);
                        if let Some(mut command) = this.template(&Field::Duration, cx) {
                            if let TimelineCommand::Timing { looping, .. } = &mut command {
                                *looping = !*looping;
                            }
                            this.execute(command, window, cx);
                        }
                    })),
            );
            return content.into_any_element();
        }
        if let Some(key) = state
            .keys
            .first()
            .filter(|_| state.keys.len() == 1)
            .and_then(|key| projection.key(key))
        {
            content = content
                .child(t!("timeline.key"))
                .child(self.labeled_number(
                    t!("timeline.time").into_owned(),
                    Field::KeyTime(key.reference.clone()),
                    key.time,
                    (0., projection.duration),
                    disabled || !key.time_editable,
                    window,
                    cx,
                ))
                .child(
                    self.labeled_number(
                        t!("timeline.value").into_owned(),
                        Field::KeyValue(key.reference.clone()),
                        key.value as f64,
                        projection
                            .channel(&projection.tracks[&key.reference.track_id].channel)
                            .and_then(|channel| channel.range)
                            .map_or((-1e9, 1e9), |(a, b)| (a as f64, b as f64)),
                        disabled || !projection.tracks[&key.reference.track_id].editable,
                        window,
                        cx,
                    ),
                );
        }
        if state.keys.iter().any(|key| {
            projection
                .key(key)
                .is_some_and(|key| key.outgoing.is_some())
        }) {
            content = content.child(div().child(t!("timeline.interpolation")));
            let mut choices = div().flex().flex_wrap().gap_1();
            for (id, kind, label) in [
                ("linear", MotionInterpolation::Linear, t!("timeline.linear")),
                ("bezier", MotionInterpolation::Bezier, t!("timeline.bezier")),
                (
                    "stepped",
                    MotionInterpolation::Stepped,
                    t!("timeline.stepped"),
                ),
                (
                    "inverse",
                    MotionInterpolation::InverseStepped,
                    t!("timeline.inverse"),
                ),
            ] {
                let keys: Vec<_> = state.keys.iter().cloned().collect();
                choices = choices.child(
                    Button::new(SharedString::from(format!("timeline-interpolation-{id}")))
                        .w(px(94.))
                        .small()
                        .ghost()
                        .label(label)
                        .selected(
                            state
                                .keys
                                .iter()
                                .filter_map(|key| projection.key(key))
                                .filter_map(|key| key.outgoing)
                                .all(|outgoing| outgoing == kind),
                        )
                        .disabled(disabled)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.execute(
                                TimelineCommand::Interpolation {
                                    keys: keys.clone(),
                                    kind,
                                },
                                window,
                                cx,
                            )
                        })),
                );
            }
            content = content.child(choices).child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("timeline.selected_keys", count = state.keys.len())),
            );
        } else if let Some(id) = state.events.first().filter(|_| state.events.len() == 1) {
            if let Some(event) = projection.events.iter().find(|event| &event.id == id) {
                content = content
                    .child(t!("timeline.event"))
                    .child(self.labeled_number(
                        t!("timeline.time").into_owned(),
                        Field::EventTime(id.clone()),
                        event.time as f64,
                        (0., projection.duration),
                        disabled,
                        window,
                        cx,
                    ));
                let field = Field::EventValue(id.clone());
                self.ensure_field(&field, event.value.clone(), (0., 1.), disabled, window, cx);
                content = content
                    .child(div().child(t!("timeline.event_value")))
                    .child(
                        div().id("timeline-event-value").test_support().child(
                            Input::new(&self.fields[&field].input)
                                .small()
                                .disabled(disabled),
                        ),
                    );
            }
        } else if let Some(row) = state
            .channels
            .first()
            .filter(|_| {
                state.channels.len() == 1 && state.keys.is_empty() && state.events.is_empty()
            })
            .and_then(|id| projection.rows.iter().find(|row| &row.id == id))
        {
            if let Some(channel) = row
                .channel
                .as_ref()
                .and_then(|channel| projection.channel(channel))
            {
                content = content.child(channel.name.clone()).child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(channel.runtime_id.clone()),
                );
                if row.track_id.is_some() || channel.tracks.len() <= 1 {
                    let raw = projection
                        .raw_value(&channel.id, row.track_id.as_deref(), time)
                        .unwrap_or(channel.default_value);
                    let range = channel.range.unwrap_or((-1e9, 1e9));
                    let field = Field::RowValue(row.id.clone());
                    content = content.child(self.labeled_number(
                        t!("timeline.raw_value").into_owned(),
                        field.clone(),
                        raw as f64,
                        (range.0 as f64, range.1 as f64),
                        disabled || !channel.editable,
                        window,
                        cx,
                    ));
                    if channel.editable {
                        if self
                            .slider
                            .as_ref()
                            .is_none_or(|slider| slider.field != field)
                        {
                            let slider = cx
                                .new(|_| SliderState::new().min(range.0).max(range.1).step(0.0001));
                            let target = field.clone();
                            let events = cx.subscribe_in(
                                &slider,
                                window,
                                move |this, source, event: &SliderEvent, window, cx| {
                                    if !this.editable(window, cx)
                                        || this
                                            .slider
                                            .as_ref()
                                            .is_none_or(|slider| &slider.state != source)
                                    {
                                        return;
                                    }
                                    if matches!(event, SliderEvent::Release(_))
                                        && this.number_edit.is_none()
                                    {
                                        return;
                                    }
                                    let (SliderEvent::Change(SliderValue::Single(value))
                                    | SliderEvent::Release(SliderValue::Single(value))) = event
                                    else {
                                        return;
                                    };
                                    this.number_change(
                                        target.clone(),
                                        value.clamp(range.0, range.1).to_string(),
                                        true,
                                        cx,
                                    );
                                    if matches!(event, SliderEvent::Release(_)) {
                                        this.finish_number(cx);
                                    }
                                },
                            );
                            self.slider = Some(ChannelSlider {
                                field: field.clone(),
                                state: slider,
                                _events: events,
                            });
                        }
                        let slider = &self.slider.as_ref().unwrap().state;
                        if self.number_edit.is_none()
                            && slider.read(cx).value()
                                != SliderValue::Single(raw.clamp(range.0, range.1))
                        {
                            slider.update(cx, |slider, cx| {
                                slider.set_value(raw.clamp(range.0, range.1), window, cx)
                            });
                        }
                        content = content.child(Slider::new(slider).disabled(disabled));
                    }
                    let refs = row.keys.clone();
                    let weak = cx.weak_entity();
                    let items: Vec<_> = refs
                        .iter()
                        .filter_map(|key| projection.key(key))
                        .map(|key| {
                            (
                                key.reference.clone(),
                                format!("{:.6}s · {}", key.time, key.value),
                            )
                        })
                        .collect();
                    content = content.child(
                        Button::new("timeline-key-list")
                            .small()
                            .ghost()
                            .label(t!("timeline.key_list"))
                            .dropdown_menu(move |mut menu, _, _| {
                                for (key, label) in &items {
                                    let key = key.clone();
                                    let weak = weak.clone();
                                    menu = menu.item(PopupMenuItem::new(label.clone()).on_click(
                                        move |_, window, cx| {
                                            if window.has_active_dialog(cx) {
                                                return;
                                            }
                                            let _ = weak.update(cx, |this, cx| {
                                                this.select_keys(vec![key.clone()], false, true, cx)
                                            });
                                        },
                                    ));
                                }
                                menu
                            }),
                    );
                }
            }
        }
        if state.channels.contains(&TimelineRowId::Events) || !state.events.is_empty() {
            let events = projection.events.clone();
            let weak = cx.weak_entity();
            content = content.child(
                Button::new("timeline-event-list")
                    .small()
                    .ghost()
                    .label(t!("timeline.event_list"))
                    .dropdown_menu(move |mut menu, _, _| {
                        for (index, event) in events.iter().enumerate() {
                            let id = event.id.clone();
                            let weak = weak.clone();
                            menu = menu.item(
                                PopupMenuItem::new(format!(
                                    "{} · {:.6}s · {}",
                                    index + 1,
                                    event.time,
                                    event.value
                                ))
                                .on_click(move |_, window, cx| {
                                    if window.has_active_dialog(cx) {
                                        return;
                                    }
                                    let _ = weak.update(cx, |this, cx| {
                                        let id = id.clone();
                                        this.change_view(
                                            move |view| {
                                                view.keys.clear();
                                                view.events.clear();
                                                view.events.insert(id);
                                                view.channels.clear();
                                                view.channels.insert(TimelineRowId::Events);
                                            },
                                            cx,
                                        );
                                    });
                                }),
                            );
                        }
                        menu
                    }),
            );
        }
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

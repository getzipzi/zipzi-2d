//! Numeric controls and candidate-edit lifecycle.
use super::*;

impl Timeline {
    pub(crate) fn benchmark_number(
        &self,
        projection: &TimelineProjection,
        cx: &App,
    ) -> Option<(Point<Pixels>, Entity<SlideNumberInput>)> {
        let channel = projection
            .channels
            .iter()
            .find(|channel| channel.runtime_id == "ParamAngleX")?;
        projection
            .rows
            .iter()
            .filter(|row| row.channel.as_ref() == Some(&channel.id))
            .find_map(|row| {
                let control = self.fields.get(&Field::RowValue(row.id.clone()))?;
                let bounds = control.input.read(cx).input_bounds();
                (bounds.size.width > px(0.) && bounds.size.height > px(0.))
                    .then(|| (bounds.center(), control.number.clone()))
            })
    }

    pub(super) fn template(&self, field: &Field, cx: &App) -> Option<TimelineCommand> {
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
    pub(super) fn ensure_field(
        &mut self,
        field: &Field,
        value: String,
        numeric_value: f64,
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
                    field.number_options(numeric_value, range, fps, disabled),
                    window,
                    cx,
                )
                .format_sliding_value(field.value_format())
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
                field.number_options(numeric_value, range, fps, disabled),
                cx,
            )
        });
    }
    pub(in crate::panels::timeline) fn number(
        &mut self,
        field: Field,
        value: f64,
        range: (f64, f64),
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = (field.value_format())(value);
        self.ensure_field(&field, text, value, range, disabled, window, cx);
        div()
            .id(SharedString::from(format!("timeline-number-{field:?}")))
            .test_support()
            .w_full()
            .child(self.fields[&field].number.clone())
            .into_any_element()
    }
    pub(super) fn number_change(
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
                .update(cx, |owner, _| {
                    let token = owner
                        .editor
                        .begin_timeline_edit_for(target.binding, "Edit motion value");
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
                drag: None,
            });
            self.sync_status_bar(cx);
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
            let edit = self.number_edit.take().unwrap();
            self.sync_capture_hint();
            let document = self.context.as_ref().map(|target| target.document);
            let _ = self.owner.update(cx, |owner, cx| {
                if let Some(document) = document {
                    owner.editor.cancel_timeline_edit_for(document, edit.token);
                }
                owner.redraw(cx);
            });
        }
    }
    pub(in crate::panels::timeline) fn finish_number_outside(
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
    pub(in crate::panels::timeline) fn finish_number(&mut self, cx: &mut Context<Self>) {
        if let Some(edit) = self.number_edit.take() {
            self.sync_capture_hint();
            self.finish_token(edit.token, cx);
        }
    }
}

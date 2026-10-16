//! Channel values, sliders and key-list navigation.
use super::*;

impl Timeline {
    pub(super) fn render_selected_channel<E: ParentElement>(
        &mut self,
        mut content: E,
        projection: &TimelineProjection,
        state: &TimelineViewState,
        time: f64,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> E {
        if let Some(row) = state
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
                                    if let Some(edit) = &this.number_edit {
                                        this.claim_number_drag(edit.token, window, cx);
                                    }
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
                        compact::button(Button::new("timeline-key-list"))
                            .label(t!("timeline.key_list"))
                            .dropdown_menu(move |mut menu, _, _| {
                                menu = crate::ui::compact::menu(menu);
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
        content
    }
}

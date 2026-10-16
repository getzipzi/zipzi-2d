//! Event editing and event-list navigation.
use super::*;

impl Timeline {
    pub(super) fn render_selected_event<E: ParentElement>(
        &mut self,
        mut content: E,
        projection: &TimelineProjection,
        state: &TimelineViewState,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> E {
        if let Some(id) = state.events.first().filter(|_| state.events.len() == 1) {
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
                self.ensure_field(
                    &field,
                    event.value.clone(),
                    0.,
                    (0., 1.),
                    disabled,
                    window,
                    cx,
                );
                content = content
                    .child(div().child(t!("timeline.event_value")))
                    .child(
                        div().id("timeline-event-value").test_support().child(
                            compact::input(Input::new(&self.fields[&field].input))
                                .small()
                                .disabled(disabled),
                        ),
                    );
            }
        }
        content
    }
    pub(super) fn render_event_list<E: ParentElement>(
        &mut self,
        mut content: E,
        projection: &TimelineProjection,
        state: &TimelineViewState,
        cx: &mut Context<Self>,
    ) -> E {
        if state.channels.contains(&TimelineRowId::Events) || !state.events.is_empty() {
            let events = projection.events.clone();
            let weak = cx.weak_entity();
            content = content.child(
                compact::button(Button::new("timeline-event-list"))
                    .label(t!("timeline.event_list"))
                    .dropdown_menu(move |mut menu, _, _| {
                        menu = crate::ui::compact::menu(menu);
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
        content
    }
}

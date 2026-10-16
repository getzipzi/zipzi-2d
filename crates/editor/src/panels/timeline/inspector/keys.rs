//! Key values and outgoing interpolation.
use super::*;

impl Timeline {
    pub(super) fn render_selected_key<E: ParentElement>(
        &mut self,
        mut content: E,
        projection: &TimelineProjection,
        state: &TimelineViewState,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> E {
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
        content
    }
    pub(super) fn render_interpolation<E: ParentElement>(
        &mut self,
        mut content: E,
        projection: &TimelineProjection,
        state: &TimelineViewState,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> E {
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
            let selected = state
                .keys
                .iter()
                .filter_map(|key| projection.key(key))
                .filter_map(|key| key.outgoing)
                .all(|outgoing| outgoing == kind);
            choices = choices.child(
                compact::button(
                    Button::new(SharedString::from(format!("timeline-interpolation-{id}")))
                        .selected(selected),
                )
                .w(compact::rem(94.))
                .label(label)
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
        content
    }
}

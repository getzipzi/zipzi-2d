//! Clip timing and fade properties.
use super::*;

impl Timeline {
    pub(super) fn render_clip_properties<E: ParentElement>(
        &mut self,
        mut content: E,
        clip: kasane_core::document::MotionClip,
        scale: f64,
        disabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> E {
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
                        compact::button(Button::new(SharedString::from(format!(
                            "timeline-fade-custom-{field:?}"
                        ))))
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
                        compact::button(Button::new(SharedString::from(format!(
                            "timeline-fade-default-{field:?}"
                        ))))
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
            compact::toggle(Button::new("timeline-asset-loop"), clip.looping)
                .label(t!("timeline.asset_loop"))
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
        content
    }
}

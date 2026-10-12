use crate::{EditorView, t};
use editor_core::CommandId;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};

pub struct PlaybackBar {
    owner: WeakEntity<EditorView>,
    _observe: Subscription,
}
impl PlaybackBar {
    pub fn new(owner: WeakEntity<EditorView>, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            _observe: cx.observe(&owner.upgrade().unwrap(), |_, _, cx| cx.notify()),
            owner,
        })
    }
}
impl Render for PlaybackBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use gpui_kit::component::WindowExt;
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let dialog = window.has_active_dialog(cx);
        let view = owner.read(cx);
        let animation = view.editor.animation();
        let modal = view.modal || dialog;
        let playing = animation.is_some_and(|a| a.playing);
        let resource = view.editor.document().and_then(|d| {
            d.motion_resources()
                .into_iter()
                .find(|r| animation.is_some_and(|a| a.motion.as_ref() == Some(&r.selection)))
        });
        // The accepted GPU frame owns the displayed time.
        let time = animation
            .and_then(|a| a.presented.as_ref())
            .map_or(0., |s| s.time);
        let label = resource.as_ref().map_or_else(
            || t!("workspace.select_motion").into_owned(),
            |r| {
                if r.looping && r.playable() {
                    format!(
                        "{:.2} / {:.2} s · {} {}",
                        time % r.duration,
                        r.duration,
                        t!("workspace.cycle"),
                        (time / r.duration).floor() as u64 + 1
                    )
                } else {
                    format!("{time:.2} / {:.2} s", r.duration)
                }
            },
        );
        let mut bar = div()
            .id("animation-playback-bar")
            .test_support()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap_3()
            .h(px(40.))
            .px_3()
            .border_t_1()
            .border_color(cx.theme().border);
        for (id, command, label) in [
            (
                "animation-play-pause",
                CommandId::PlayPause,
                if playing {
                    t!("workspace.pause")
                } else {
                    t!("workspace.play")
                },
            ),
            (
                "animation-rewind",
                CommandId::RewindAnimation,
                t!("workspace.rewind"),
            ),
        ] {
            let owner = self.owner.clone();
            bar = bar.child(
                Button::new(id)
                    .small()
                    .ghost()
                    .label(label)
                    .disabled(modal || !view.editor.command_available(view.editor.command(command)))
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |view, cx| view.run_command(command, window, cx));
                    }),
            );
        }
        bar.child(
            div()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .into_any_element()
    }
}

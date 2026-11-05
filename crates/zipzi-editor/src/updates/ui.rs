//! Quiet window chrome; progress, release notes and failures live in this panel.
use super::{Phase, Updater};
use crate::{t, ui::compact};
use gpui_kit::component::{
    ActiveTheme, Disableable, WindowExt,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
};
use gpui_kit::{prelude::*, *};

pub(crate) fn show(window: &mut Window, cx: &mut App, check: bool) {
    let updater = super::init(cx);
    if check {
        updater.update(cx, |updater, cx| updater.check(cx));
    }
    let panel = cx.new(|cx| {
        let subscription = cx.observe(&updater, |_, _, cx| cx.notify());
        UpdatePanel {
            updater,
            _subscription: subscription,
        }
    });
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title(t!("updates.title"))
            .width(px(480.))
            .child(panel.clone())
    });
}

struct UpdatePanel {
    updater: Entity<Updater>,
    _subscription: Subscription,
}

impl Render for UpdatePanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let updater = self.updater.read(cx);
        let phase = updater.phase.clone();
        let automatic = updater.automatic;
        let detail = updater.detail.clone();
        let version = updater
            .release
            .as_ref()
            .map(|release| release.TargetFullRelease.Version.clone());
        let notes = updater
            .release
            .as_ref()
            .map(|release| {
                release
                    .TargetFullRelease
                    .NotesMarkdown
                    .chars()
                    .take(4096)
                    .collect::<String>()
            })
            .filter(|notes| !notes.trim().is_empty());
        let status = match phase {
            Phase::Unavailable => t!("updates.unavailable"),
            Phase::Idle => t!("updates.idle"),
            Phase::Checking => t!("updates.checking"),
            Phase::Current => t!("updates.current"),
            Phase::Available => t!(
                "updates.available",
                version = version.as_deref().unwrap_or_default()
            ),
            Phase::Downloading(progress) => t!("updates.downloading", progress = progress),
            Phase::Ready => t!(
                "updates.ready",
                version = version.as_deref().unwrap_or_default()
            ),
            Phase::Applying => t!("updates.applying"),
            Phase::Failed => t!("updates.failed"),
        }
        .into_owned();
        let (label, enabled) = match phase {
            Phase::Available => (t!("updates.download"), true),
            Phase::Ready => (t!("updates.restart"), true),
            Phase::Unavailable => (t!("updates.download_page"), true),
            _ => (t!("updates.check"), !phase.busy()),
        };
        let action_updater = self.updater.clone();
        let preference_updater = self.updater.clone();
        div()
            .id("updates-panel")
            .test_support()
            .flex()
            .flex_col()
            .gap_3()
            .text_size(compact::rem(compact::BODY))
            .child(
                div()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("updates.installed", version = env!("CARGO_PKG_VERSION"))),
            )
            .child(div().id("updates-status").test_support().child(status))
            .when_some(detail, |panel, detail| {
                panel.child(
                    div()
                        .id("updates-detail")
                        .test_support()
                        .text_color(cx.theme().muted_foreground)
                        .child(detail.chars().take(2048).collect::<String>()),
                )
            })
            .when_some(notes, |panel, notes| {
                panel.child(
                    div()
                        .id("updates-release-notes")
                        .max_h(px(200.))
                        .overflow_y_scroll()
                        .child(notes),
                )
            })
            .child(
                Checkbox::new("updates-automatic")
                    .label(t!("updates.automatic").into_owned())
                    .checked(automatic)
                    .disabled(phase == Phase::Unavailable || phase == Phase::Applying)
                    .on_click(move |value, _, cx| {
                        preference_updater
                            .update(cx, |updater, cx| updater.set_automatic(*value, cx));
                    }),
            )
            .child(
                div()
                    .text_size(compact::rem(compact::CAPTION))
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("updates.safe_exit")),
            )
            .child(
                div().flex().justify_end().child(
                    Button::new("updates-primary")
                        .primary()
                        .label(label.into_owned())
                        .disabled(!enabled)
                        .on_click(move |_, window, cx| {
                            let phase = action_updater.read(cx).phase.clone();
                            match phase {
                                Phase::Unavailable => cx.open_url(super::DOWNLOADS),
                                Phase::Ready => super::restart(window, cx),
                                Phase::Available => {
                                    action_updater.update(cx, |updater, cx| updater.download(cx))
                                }
                                _ => action_updater.update(cx, |updater, cx| updater.check(cx)),
                            }
                        }),
                ),
            )
    }
}

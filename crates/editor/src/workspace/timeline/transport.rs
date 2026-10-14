use crate::{EditorView, t};
use editor_core::{AnimationPreviewMode, CommandId};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};

pub struct TimelineTransport {
    owner: WeakEntity<EditorView>,
    _observe: Subscription,
}
impl TimelineTransport {
    pub fn new(owner: WeakEntity<EditorView>, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            _observe: cx.observe(&owner.upgrade().unwrap(), |_, _, cx| cx.notify()),
            owner,
        })
    }
}
impl Render for TimelineTransport {
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
        let preview_loop = animation.is_some_and(|animation| animation.preview_loop);
        let preview_mode = animation.map_or(AnimationPreviewMode::Editing, |a| a.preview_mode);
        let document_id = view.editor.active_id();
        let resource = view.editor.document().and_then(|d| {
            d.animation_catalog()
                .motions
                .iter()
                .find(|r| animation.is_some_and(|a| a.motion.as_ref() == Some(&r.selection)))
                .cloned()
        });
        // Local time and loop number come from the accepted SDK frame.
        let displayed = view.displayed_animation();
        let position = displayed.and_then(|frame| {
            frame.motion_positions.iter().find(|position| {
                resource
                    .as_ref()
                    .is_some_and(|r| position.motion_id == r.selection.clip_id)
            })
        });
        let time = position.map_or_else(
            || displayed.map_or(0., |frame| frame.time),
            |position| position.local_time,
        );
        let time = if playing {
            time
        } else {
            animation.map_or(time, |a| a.edit_time)
        };
        let label = resource.as_ref().map_or_else(
            || format!("{time:.2} s"),
            |r| {
                if r.looping && preview_mode == AnimationPreviewMode::Result {
                    format!(
                        "{time:.2} / {:.2} s · {} {}",
                        r.duration,
                        t!("workspace.cycle"),
                        position.map_or(1, |position| position.loop_index + 1)
                    )
                } else {
                    format!("{time:.2} / {:.2} s", r.duration)
                }
            },
        );
        let available = [CommandId::PlayPause, CommandId::RewindAnimation]
            .map(|command| !modal && view.editor.command_available(view.editor.command(command)));
        let mut controls = div().w_full().min_w_0().flex().items_center().gap_1();
        let bar = div()
            .id("timeline-transport")
            .test_support()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap_1()
            .h(px(32.))
            .px_3()
            .border_t_1()
            .border_color(cx.theme().border);
        for ((id, command, icon), available) in [
            (
                "animation-play-pause",
                CommandId::PlayPause,
                if playing {
                    IconName::Pause
                } else {
                    IconName::Play
                },
            ),
            (
                "animation-rewind",
                CommandId::RewindAnimation,
                IconName::RotateCcw,
            ),
        ]
        .into_iter()
        .zip(available)
        {
            let owner = self.owner.clone();
            controls = controls.child(
                Button::new(id)
                    .small()
                    .ghost()
                    .icon(icon)
                    .tooltip(if command == CommandId::PlayPause {
                        t!("workspace.play_hint")
                    } else {
                        t!("workspace.rewind_hint")
                    })
                    .disabled(!available)
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |view, cx| view.run_command(command, window, cx));
                    }),
            );
        }
        for (id, mode, label) in [
            (
                "animation-edit-preview",
                AnimationPreviewMode::Editing,
                t!("workspace.edit_preview"),
            ),
            (
                "animation-result-preview",
                AnimationPreviewMode::Result,
                t!("workspace.result_preview"),
            ),
        ] {
            let owner = self.owner.clone();
            controls = controls.child(
                Button::new(id)
                    .small()
                    .ghost()
                    .label(label)
                    .selected(preview_mode == mode)
                    .disabled(modal || document_id.is_none())
                    .on_click(move |_, window, cx| {
                        if window.has_active_dialog(cx) {
                            return;
                        }
                        let _ = owner.update(cx, |view, cx| {
                            if view.modal {
                                return;
                            }
                            let result = view
                                .editor
                                .set_animation_preview_mode(document_id.unwrap(), mode);
                            view.animation_anchor = None;
                            view.complete_edit(result, cx);
                        });
                    }),
            );
        }
        let owner = self.owner.clone();
        controls = controls.child(
            Button::new("animation-preview-loop")
                .small()
                .ghost()
                .label(t!("workspace.loop_preview"))
                .selected(preview_loop)
                .disabled(
                    modal || document_id.is_none() || preview_mode != AnimationPreviewMode::Editing,
                )
                .on_click(move |_, window, cx| {
                    if window.has_active_dialog(cx) {
                        return;
                    }
                    let _ = owner.update(cx, |view, cx| {
                        if view.modal {
                            return;
                        }
                        let result = view
                            .editor
                            .set_animation_preview_loop(document_id.unwrap(), !preview_loop);
                        view.animation_anchor = None;
                        view.complete_edit(result, cx);
                    });
                }),
        );
        let bar = bar.child(
            controls.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_right()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(label),
            ),
        );
        bar.into_any_element()
    }
}

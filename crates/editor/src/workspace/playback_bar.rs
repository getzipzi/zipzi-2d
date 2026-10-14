use crate::{EditorView, t};
use editor_core::{AnimationPreviewMode, CommandId, DocumentId, MotionSelection, WorkspaceKind};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    slider::{Slider, SliderEvent, SliderState, SliderValue},
    tooltip::Tooltip,
};
use gpui_kit::{prelude::*, *};

pub struct PlaybackBar {
    owner: WeakEntity<EditorView>,
    _observe: Subscription,
    _activation: Subscription,
    seek: Option<SeekControl>,
}
#[derive(Clone, PartialEq)]
struct SeekKey {
    document: DocumentId,
    workspace_revision: u64,
    model_generation: u64,
    motion: MotionSelection,
    preview_mode: AnimationPreviewMode,
}
struct SeekControl {
    key: SeekKey,
    state: Entity<SliderState>,
    dragging: bool,
    pending: Option<f32>,
    _events: Subscription,
}
impl PlaybackBar {
    pub fn new(owner: WeakEntity<EditorView>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx: &mut Context<Self>| Self {
            _observe: cx.observe_in(
                &owner.upgrade().unwrap(),
                window,
                |this, owner, window, cx| {
                    use gpui_kit::component::WindowExt;
                    let dialog = window.has_active_dialog(cx);
                    let view = owner.read(cx);
                    let valid = !dialog
                        && !view.modal
                        && !view.editor.is_busy()
                        && !view.editor.exiting()
                        && view.editor.workspace() == WorkspaceKind::Animation
                        && view.editor.canvas_metrics().visible()
                        && this.seek.as_ref().is_none_or(|seek| {
                            view.editor.active_id() == Some(seek.key.document)
                                && view.editor.workspace_revision() == seek.key.workspace_revision
                                && view.editor.animation().is_some_and(|a| {
                                    a.model_generation == seek.key.model_generation
                                        && a.preview_mode == seek.key.preview_mode
                                        && a.motion.as_ref() == Some(&seek.key.motion)
                                })
                        });
                    if !valid {
                        this.cancel_seek(window, cx);
                    }
                    cx.notify();
                },
            ),
            _activation: cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.cancel_seek(window, cx);
                    cx.notify();
                }
            }),
            seek: None,
            owner,
        })
    }
    fn cancel_seek(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.seek.take().is_some_and(|seek| seek.dragging) {
            cx.stop_active_drag(window);
        }
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
        let enabled = !modal
            && !view.editor.is_busy()
            && !view.editor.exiting()
            && view.editor.workspace() == WorkspaceKind::Animation
            && view.editor.canvas_metrics().visible()
            && resource.as_ref().is_some_and(|r| r.playable());
        let key = enabled.then(|| SeekKey {
            document: view.editor.active_id().unwrap(),
            workspace_revision: view.editor.workspace_revision(),
            model_generation: animation.unwrap().model_generation,
            motion: resource.as_ref().unwrap().selection.clone(),
            preview_mode,
        });
        let failed = animation.is_some_and(|a| a.error.is_some());
        let target_time = animation.map_or(0., |a| a.edit_time);
        let available = [CommandId::PlayPause, CommandId::RewindAnimation]
            .map(|command| !modal && view.editor.command_available(view.editor.command(command)));
        let duration = resource.as_ref().map_or(1., |r| r.duration);
        if self.seek.as_ref().map(|s| &s.key) != key.as_ref() {
            self.cancel_seek(window, cx);
            self.seek = key.map(|key| {
                let state = cx.new(|_| SliderState::new().min(0.).max(1.).step(0.0001));
                let target = key.clone();
                let events =
                    cx.subscribe_in(&state, window, move |this, source, event, window, cx| {
                        use gpui_kit::component::WindowExt;
                        if window.has_active_dialog(cx) {
                            return;
                        }
                        let Some(seek) = this.seek.as_mut().filter(|s| &s.state == source) else {
                            return;
                        };
                        let (SliderEvent::Change(SliderValue::Single(value))
                        | SliderEvent::Release(SliderValue::Single(value))) = event
                        else {
                            return;
                        };
                        seek.dragging = matches!(event, SliderEvent::Change(_));
                        let time = (value * duration).clamp(0., duration);
                        seek.pending = Some(time);
                        let _ = this.owner.update(cx, |view, cx| {
                            if view.modal
                                || view.editor.workspace_revision() != target.workspace_revision
                                || !view.editor.animation().is_some_and(|a| {
                                    a.model_generation == target.model_generation
                                        && a.preview_mode == target.preview_mode
                                        && a.motion.as_ref() == Some(&target.motion)
                                })
                            {
                                return;
                            }
                            let result = if matches!(event, SliderEvent::Change(_)) {
                                view.editor.scrub_timeline(target.document, time)
                            } else {
                                view.editor.seek_timeline(target.document, time)
                            };
                            view.animation_anchor = None;
                            view.complete_edit(result, cx);
                        });
                    });
                SeekControl {
                    key,
                    state,
                    dragging: false,
                    pending: None,
                    _events: events,
                }
            });
        }
        if let Some(seek) = &mut self.seek {
            if seek.pending.is_some_and(|pending| pending != target_time) {
                seek.pending = None;
                seek.dragging = false;
            }
            if playing || failed || seek.pending == Some(time) {
                seek.pending = None;
            }
            if !seek.dragging && seek.pending.is_none() {
                let progress = (time / duration).clamp(0., 1.);
                if seek.state.read(cx).value() != SliderValue::Single(progress) {
                    seek.state
                        .update(cx, |state, cx| state.set_value(progress, window, cx));
                }
            }
        }
        let mut controls = div().w_full().min_w_0().flex().items_center().gap_1();
        let bar = div()
            .id("animation-playback-bar")
            .test_support()
            .flex()
            .flex_col()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap_1()
            .h(px(64.))
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
        bar.child(
            div()
                .id("animation-seek")
                .test_support()
                .w_full()
                .h(px(16.))
                .flex()
                .items_center()
                .min_w(px(36.))
                .tooltip(move |window, cx| {
                    Tooltip::new(
                        (if enabled {
                            t!("workspace.seek_hint")
                        } else {
                            t!("workspace.no_seek_hint")
                        })
                        .into_owned(),
                    )
                    .build(window, cx)
                })
                .when(self.seek.is_none(), |e| {
                    e.child(div().w_full().h(px(3.)).bg(cx.theme().border))
                })
                .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    if event.button != MouseButton::Left || window.has_active_dialog(cx) {
                        return;
                    }
                    let Some(seek) = &mut this.seek else {
                        return;
                    };
                    seek.dragging = true;
                    let _ = this.owner.update(cx, |view, cx| {
                        if view.modal {
                            return;
                        }
                        view.editor.pause_animation();
                        view.animation_anchor = None;
                        view.redraw(cx);
                    });
                }))
                .capture_any_mouse_up(cx.listener(|this, _: &MouseUpEvent, _, cx| {
                    if let Some(seek) = &mut this.seek {
                        seek.dragging = false;
                    }
                    cx.notify();
                }))
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseUpEvent, _, cx| {
                        if let Some(seek) = &mut this.seek {
                            seek.dragging = false;
                        }
                        cx.notify();
                    }),
                )
                .children(
                    self.seek
                        .as_ref()
                        .map(|seek| Slider::new(&seek.state).w_full()),
                ),
        )
        .into_any_element()
    }
}

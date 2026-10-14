use super::*;
use editor_core::CommandId;
use gpui_kit::component::Icon;

impl Timeline {
    pub(super) fn toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let owner = self.owner.upgrade().unwrap();
        let view = owner.read(cx);
        let animation = view.editor.animation();
        let document = view.editor.active_id();
        let playing = animation.is_some_and(|a| a.playing);
        let mode = animation.map_or(AnimationPreviewMode::Editing, |a| a.preview_mode);
        let looping = animation.is_some_and(|a| a.preview_loop);
        let resource = view.editor.document().and_then(|document| {
            document
                .animation_catalog()
                .motions
                .iter()
                .find(|r| animation.is_some_and(|a| a.motion.as_ref() == Some(&r.selection)))
                .cloned()
        });
        let snapshot = view.displayed_animation();
        let position = snapshot.and_then(|frame| {
            frame.motion_positions.iter().find(|position| {
                resource
                    .as_ref()
                    .is_some_and(|r| position.motion_id == r.selection.clip_id)
            })
        });
        let time = if playing {
            position.map_or_else(|| snapshot.map_or(0., |s| s.time), |p| p.local_time)
        } else {
            animation.map_or(0., |a| a.edit_time)
        };
        let play_enabled = view
            .editor
            .command_available(view.editor.command(CommandId::PlayPause));
        let name = resource.as_ref().map_or_else(
            || t!("timeline.select_motion").into_owned(),
            |r| r.name.clone(),
        );
        let time_label = resource.as_ref().map_or_else(
            || format!("{time:.2} s"),
            |r| format!("{time:.2} / {:.2} s", r.duration),
        );
        let cycle = position.map_or(1, |p| p.loop_index + 1);
        let time_hint = if mode == AnimationPreviewMode::Result
            && resource.as_ref().is_some_and(|r| r.looping)
        {
            format!("{time_label} · {} {cycle}", t!("workspace.cycle"))
        } else {
            time_label.clone()
        };
        let asset_loop = resource.as_ref().is_some_and(|r| r.looping);
        let allowed = self.allowed(window, cx);
        let seek_enabled = allowed && resource.as_ref().is_some_and(|r| r.playable());
        let mut transport = div()
            .id("timeline-transport")
            .test_support()
            .flex()
            .items_center()
            .gap(px(2.))
            .flex_shrink_0();
        for (id, command, icon, hint) in [
            (
                "timeline-start",
                "start",
                IconName::SkipBack,
                t!("timeline.jump_start"),
            ),
            (
                "timeline-previous-key",
                "previous",
                IconName::ChevronsLeft,
                t!("timeline.previous_key"),
            ),
            (
                "timeline-previous-frame",
                "left",
                IconName::ChevronLeft,
                t!("timeline.previous_frame"),
            ),
            (
                "animation-play-pause",
                "play",
                if playing {
                    IconName::Pause
                } else {
                    IconName::Play
                },
                t!("workspace.play_hint"),
            ),
            (
                "timeline-next-frame",
                "right",
                IconName::ChevronRight,
                t!("timeline.next_frame"),
            ),
            (
                "timeline-next-key",
                "next",
                IconName::ChevronsRight,
                t!("timeline.next_key"),
            ),
            (
                "timeline-end",
                "end",
                IconName::SkipForward,
                t!("timeline.jump_end"),
            ),
        ] {
            if self.compact && matches!(command, "left" | "right") {
                continue;
            }
            transport = transport.child(
                Button::new(id)
                    .small()
                    .when(self.compact, |button| button.xsmall())
                    .ghost()
                    .icon(icon)
                    .accessibility_label(hint.clone().into_owned())
                    .tooltip(hint)
                    .disabled(if command == "play" {
                        !allowed || !play_enabled
                    } else {
                        !seek_enabled
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.finish_number(cx);
                        window.focus(&this.focus, cx);
                        if command == "play" {
                            let _ = this.owner.update(cx, |owner, cx| {
                                owner.run_command(CommandId::PlayPause, window, cx)
                            });
                        } else {
                            this.key_command(command, window, cx);
                        }
                    })),
            );
        }
        let weak = self.owner.clone();
        transport = transport.child(
            Button::new("animation-preview-loop")
                .small()
                .when(self.compact, |button| button.xsmall())
                .ghost()
                .icon(IconName::Repeat)
                .selected(if mode == AnimationPreviewMode::Editing {
                    looping
                } else {
                    asset_loop
                })
                .accessibility_label(t!("timeline.loop_playback").into_owned())
                .tooltip(t!(if mode == AnimationPreviewMode::Editing {
                    "timeline.loop_hint"
                } else {
                    "timeline.asset_loop_hint"
                }))
                .disabled(!allowed || document.is_none() || mode != AnimationPreviewMode::Editing)
                .on_click(move |_, window, cx| {
                    if window.has_active_dialog(cx) {
                        return;
                    }
                    let _ = weak.update(cx, |owner, cx| {
                        let result = owner
                            .editor
                            .set_animation_preview_loop(document.unwrap(), !looping);
                        owner.animation_anchor = None;
                        owner.complete_edit(result, cx);
                    });
                }),
        );
        let weak = self.owner.clone();
        transport = transport.child(
            Button::new("animation-preview-mode")
                .small()
                .when(self.compact, |button| button.xsmall())
                .ghost()
                .icon(if mode == AnimationPreviewMode::Editing {
                    IconName::EyeOff
                } else {
                    IconName::Eye
                })
                .selected(mode == AnimationPreviewMode::Result)
                .accessibility_label(t!("timeline.preview_effect").into_owned())
                .tooltip(format!(
                    "{}\n{}",
                    t!("timeline.preview_effect"),
                    t!(if mode == AnimationPreviewMode::Editing {
                        "timeline.editing_hint"
                    } else {
                        "timeline.result_hint"
                    })
                ))
                .disabled(!allowed || document.is_none())
                .dropdown_menu(move |mut menu, _, _| {
                    for (id, choice, label) in [
                        (
                            "animation-edit-preview",
                            AnimationPreviewMode::Editing,
                            t!("timeline.editing_effect"),
                        ),
                        (
                            "animation-result-preview",
                            AnimationPreviewMode::Result,
                            t!("timeline.result_effect"),
                        ),
                    ] {
                        let label = label.into_owned();
                        let weak = weak.clone();
                        menu = menu.item(
                            PopupMenuItem::element(move |_, _| {
                                div().id(id).test_support().child(label.clone())
                            })
                            .checked(mode == choice)
                            .on_click(move |_, window, cx| {
                                if window.has_active_dialog(cx) {
                                    return;
                                }
                                let _ = weak.update(cx, |owner, cx| {
                                    let result = owner
                                        .editor
                                        .set_animation_preview_mode(document.unwrap(), choice);
                                    owner.animation_anchor = None;
                                    owner.complete_edit(result, cx);
                                });
                            }),
                        );
                    }
                    menu
                }),
        );
        let tools = self.tools(window, cx);
        div()
            .id("timeline-toolbar")
            .test_support()
            .h(px(32.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_1()
            .px_1()
            .child(transport)
            .child(
                div()
                    .h(px(18.))
                    .border_l_1()
                    .border_color(cx.theme().border),
            )
            .child(tools)
            .child(
                div()
                    .id("timeline-motion-name")
                    .test_support()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .when(!self.compact, |label| label.child(name)),
            )
            .when_some(self.error.clone(), |toolbar, error| {
                toolbar.child(
                    div()
                        .id("timeline-error")
                        .test_support()
                        .flex_shrink_0()
                        .text_color(cx.theme().danger)
                        .tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(error.clone())
                                .build(window, cx)
                        })
                        .child(Icon::new(IconName::TriangleAlert).small()),
                )
            })
            .child(
                div()
                    .id("timeline-time")
                    .test_support()
                    .flex_shrink_0()
                    .text_xs()
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(time_hint.clone())
                            .build(window, cx)
                    })
                    .child(time_label),
            )
            .into_any_element()
    }
}

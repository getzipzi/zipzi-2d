use super::*;
use gpui_kit::component::Icon;
use zipzi_editor_core::CommandId;

impl Timeline {
    pub(super) fn toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let allowed = self.allowed(window, cx);
        let owner = self.owner.upgrade().unwrap();
        let view = owner.read(cx);
        let animation = view.editor.animation();
        let document = view.editor.active_id();
        let playing = animation.is_some_and(|a| a.playing && !a.physics_preview.manual);
        let mode = animation.map_or(AnimationPreviewMode::default(), |a| a.preview_mode);
        let looping = animation.is_none_or(|a| a.preview_loop);
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
        let mut transport = Vec::new();
        for (id, command, icon, hint) in [
            (
                "timeline-start",
                CommandId::TimelineStart,
                IconName::SkipBack,
                t!("timeline.jump_start"),
            ),
            (
                "timeline-previous-key",
                CommandId::TimelinePreviousKey,
                IconName::ChevronsLeft,
                t!("timeline.previous_key"),
            ),
            (
                "timeline-previous-frame",
                CommandId::TimelinePreviousFrame,
                IconName::ChevronLeft,
                t!("timeline.previous_frame"),
            ),
            (
                "animation-play-pause",
                CommandId::PlayPause,
                if playing {
                    IconName::Pause
                } else {
                    IconName::Play
                },
                t!("workspace.play_hint"),
            ),
            (
                "timeline-next-frame",
                CommandId::TimelineNextFrame,
                IconName::ChevronRight,
                t!("timeline.next_frame"),
            ),
            (
                "timeline-next-key",
                CommandId::TimelineNextKey,
                IconName::ChevronsRight,
                t!("timeline.next_key"),
            ),
            (
                "timeline-end",
                CommandId::TimelineEnd,
                IconName::SkipForward,
                t!("timeline.jump_end"),
            ),
        ] {
            if (self.header_tiny && command != CommandId::PlayPause)
                || (self.header_compact
                    && matches!(
                        command,
                        CommandId::TimelinePreviousFrame | CommandId::TimelineNextFrame
                    ))
            {
                continue;
            }
            transport.push(
                compact::header_button(
                    Button::new(id).when(command == CommandId::PlayPause, |button| {
                        button.selected(playing).toggled(playing)
                    }),
                )
                .icon(icon)
                .accessibility_label(hint.clone().into_owned())
                .tooltip(hint)
                .disabled(
                    !allowed
                        || !owner
                            .read(cx)
                            .editor
                            .command_state(owner.read(cx).editor.command(command))
                            .enabled,
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.finish_number(cx);
                    window.focus(&this.focus, cx);
                    if command == CommandId::PlayPause {
                        let _ = this.owner.update(cx, |owner, cx| {
                            owner.run_command(CommandId::PlayPause, window, cx)
                        });
                    } else {
                        this.user_command(command, window, cx);
                    }
                })),
            );
        }
        let weak = self.owner.clone();
        if !self.header_tiny {
            transport.push(
                compact::header_button(compact::toggle(
                    Button::new("animation-preview-loop"),
                    if mode == AnimationPreviewMode::Editing {
                        looping
                    } else {
                        asset_loop
                    },
                ))
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
        }
        let weak = self.owner.clone();
        let preview = (!self.header_tiny).then(|| {
            compact::header_button(
                Button::new("animation-preview-mode")
                    .selected(mode == AnimationPreviewMode::Result),
            )
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
                menu = crate::ui::compact::menu(menu);
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
                let weak = weak.clone();
                menu.item(
                    PopupMenuItem::element(|_, _| {
                        div()
                            .id("animation-reset")
                            .test_support()
                            .child(t!("workspace.reset_preview"))
                    })
                    .on_click(move |_, window, cx| {
                        let _ = weak.update(cx, |owner, cx| {
                            owner.run_command(CommandId::ResetAnimation, window, cx)
                        });
                    }),
                )
            })
            .into_any_element()
        });
        let transport = compact::group(
            "timeline-transport",
            t!("timeline.playback_group"),
            Axis::Horizontal,
            transport,
        );
        let tools = self.tools(window, cx);
        let time_display = div()
            .id("timeline-time")
            .test_support()
            .flex_shrink_0()
            .text_size(compact::rem(compact::CAPTION))
            .font_family(".AppleSystemUIFontMonospaced")
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(time_hint.clone()).build(window, cx)
            })
            .child(if self.header_compact {
                format!("{time:.2}")
            } else {
                time_label
            });
        let left = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(compact::rem(6.))
            .when(!self.header_compact, |left| {
                left.child(
                    div()
                        .id("timeline-motion-name")
                        .test_support()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(name),
                )
            })
            .when_some(self.error.clone(), |left, error| {
                left.child(
                    div()
                        .id("timeline-error")
                        .test_support()
                        .flex_shrink_0()
                        .text_color(cx.theme().danger)
                        .tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(error.clone())
                                .build(window, cx)
                        })
                        .child(Icon::new(IconName::TriangleAlert).xsmall()),
                )
            })
            .children(preview)
            .child(time_display)
            .child(self.render_playback_speed(allowed));
        let center = div()
            .flex_shrink_0()
            .flex()
            .items_center()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(transport);
        let right = div()
            .flex_1()
            .flex()
            .items_center()
            .justify_end()
            .gap(compact::rem(6.))
            .child(tools);
        div()
            .id("timeline-toolbar")
            .test_support()
            .h(compact::rem(compact::HEADER_CONTROL))
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(compact::rem(6.))
            .px_0()
            .child(left)
            .child(center)
            .child(right)
            .into_any_element()
    }
}

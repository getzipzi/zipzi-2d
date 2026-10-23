use super::*;

impl Timeline {
    pub(super) fn tools(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let owner = self.owner.upgrade().unwrap();
        let state = owner
            .read(cx)
            .editor
            .timeline_view()
            .cloned()
            .unwrap_or_default();
        let preview_mode = owner
            .read(cx)
            .editor
            .animation()
            .map_or(AnimationPreviewMode::default(), |a| a.preview_mode);
        let looping = owner
            .read(cx)
            .editor
            .animation()
            .is_some_and(|a| a.preview_loop);
        let allowed = self.allowed(window, cx);
        let editable = self.editable(window, cx);
        let mut actions = [
            (
                "timeline-previous-frame",
                IconName::ChevronLeft,
                t!("timeline.previous_frame").into_owned(),
                false,
                allowed,
            ),
            (
                "timeline-next-frame",
                IconName::ChevronRight,
                t!("timeline.next_frame").into_owned(),
                false,
                allowed,
            ),
            (
                "timeline-view",
                IconName::ChartLine,
                t!("timeline.curves").into_owned(),
                state.mode == TimelineDisplayMode::Curves,
                allowed,
            ),
            (
                "timeline-snap",
                IconName::Magnet,
                t!("timeline.snap_hint").into_owned(),
                state.snap,
                allowed,
            ),
            (
                "timeline-fit",
                IconName::Scan,
                t!("timeline.fit").into_owned(),
                false,
                allowed,
            ),
            (
                "timeline-seconds",
                if self.seconds {
                    IconName::Clock
                } else {
                    IconName::Film
                },
                t!("timeline.units").into_owned(),
                self.seconds,
                allowed,
            ),
            (
                "timeline-normalized",
                IconName::Percent,
                t!("timeline.normalize_hint").into_owned(),
                state.normalized,
                allowed && state.mode == TimelineDisplayMode::Curves,
            ),
            (
                "timeline-add-event",
                IconName::MessageSquarePlus,
                t!("timeline.add_event").into_owned(),
                false,
                editable,
            ),
        ];
        for (id, _, _, _, enabled) in &mut actions {
            if let Some(command) = tool_command(id) {
                *enabled = allowed
                    && owner
                        .read(cx)
                        .editor
                        .command_state(owner.read(cx).editor.command(command))
                        .enabled;
            }
        }
        let mut edit_buttons = Vec::new();
        let mut view_buttons = Vec::new();
        let mut tools = div()
            .id("timeline-tools")
            .test_support()
            .flex()
            .items_center()
            .gap(compact::rem(compact::GAP * 3.))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .flex_shrink_0();
        for (id, icon, label, selected, enabled) in &actions {
            if !self.header_compact
                && matches!(*id, "timeline-previous-frame" | "timeline-next-frame")
            {
                continue;
            }
            if self.header_compact {
                continue;
            }
            let id = *id;
            let button = compact::header_button(Button::new(id).selected(*selected).when(
                !matches!(
                    id,
                    "timeline-previous-frame"
                        | "timeline-next-frame"
                        | "timeline-fit"
                        | "timeline-add-event"
                ),
                |button| button.toggled(*selected),
            ))
            .icon(tool_icon(id, *icon))
            .accessibility_label(label.clone())
            .tooltip(label.clone())
            .selected(*selected)
            .disabled(!enabled)
            .on_click(cx.listener(move |this, _, window, cx| this.activate_tool(id, window, cx)));
            if matches!(id, "timeline-snap" | "timeline-add-event") {
                edit_buttons.push(button);
            } else {
                view_buttons.push(button);
            }
        }
        tools = tools
            .child(compact::group(
                "timeline-edit-tools",
                t!("timeline.edit_group"),
                Axis::Horizontal,
                edit_buttons,
            ))
            .child(compact::group(
                "timeline-view-tools",
                t!("timeline.view_group"),
                Axis::Horizontal,
                view_buttons,
            ));
        if self.header_compact {
            let weak = cx.weak_entity();
            let tiny = self.header_tiny;
            tools = tools.child(
                compact::header_button(Button::new("timeline-more"))
                    .icon(IconName::Ellipsis)
                    .accessibility_label(t!("timeline.more_tools").into_owned())
                    .tooltip(t!("timeline.more_tools"))
                    .disabled(!allowed)
                    .dropdown_menu(move |mut menu, _, cx| {
                        menu = crate::ui::compact::menu(menu);
                        for (id, icon, label, selected, enabled) in &actions {
                            let id = *id;
                            let label = label.clone();
                            let weak = weak.clone();
                            menu = menu.item(
                                PopupMenuItem::element(move |_, _| {
                                    div()
                                        .id(SharedString::from(format!(
                                            "timeline-menu-{}",
                                            id.trim_start_matches("timeline-")
                                        )))
                                        .test_support()
                                        .child(label.clone())
                                })
                                .icon(tool_icon(id, *icon))
                                .checked(*selected)
                                .disabled(!enabled)
                                .on_click(move |_, window, cx| {
                                    let _ = weak
                                        .update(cx, |this, cx| this.activate_tool(id, window, cx));
                                }),
                            );
                        }
                        if tiny {
                            for (id, label, command) in [
                                ("timeline-start", t!("timeline.jump_start"), "start"),
                                (
                                    "timeline-previous-key",
                                    t!("timeline.previous_key"),
                                    "previous",
                                ),
                                ("timeline-next-key", t!("timeline.next_key"), "next"),
                                ("timeline-end", t!("timeline.jump_end"), "end"),
                                (
                                    "animation-preview-loop",
                                    t!("timeline.loop_playback"),
                                    "loop",
                                ),
                                (
                                    "animation-preview-mode",
                                    t!("timeline.preview_effect"),
                                    "preview",
                                ),
                            ] {
                                let weak = weak.clone();
                                let label = label.into_owned();
                                menu = menu.item(
                                    PopupMenuItem::element(move |_, _| {
                                        div().id(id).test_support().child(label.clone())
                                    })
                                    .checked(
                                        (command == "loop" && looping)
                                            || (command == "preview"
                                                && preview_mode == AnimationPreviewMode::Result),
                                    )
                                    .disabled(
                                        !allowed
                                            || tool_command(command).is_some_and(|id| {
                                                !owner
                                                    .read(cx)
                                                    .editor
                                                    .command_state(
                                                        owner.read(cx).editor.command(id),
                                                    )
                                                    .enabled
                                            })
                                            || (command == "loop"
                                                && preview_mode != AnimationPreviewMode::Editing),
                                    )
                                    .on_click(
                                        move |_, window, cx| {
                                            _ = weak.update(cx, |this, cx| {
                                                this.activate_tool(command, window, cx)
                                            });
                                        },
                                    ),
                                );
                            }
                        }
                        menu
                    }),
            );
        }
        tools.into_any_element()
    }

    #[cfg(test)]
    pub(crate) fn toggle_pan_tool_for_test(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.activate_tool("timeline-pan", window, cx);
    }

    fn activate_tool(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.allowed(window, cx) {
            return;
        }
        self.finish_number(cx);
        window.focus(&self.focus, cx);
        if let Some(command) = tool_command(id) {
            self.user_command(command, window, cx);
            return;
        }
        match id {
            "loop" | "preview" => {
                _ = self.owner.update(cx, |owner, cx| {
                    let Some(document) = owner.editor.active_id() else {
                        return;
                    };
                    let Some(animation) = owner.editor.animation() else {
                        return;
                    };
                    if id == "loop" && animation.preview_mode != AnimationPreviewMode::Editing {
                        return;
                    }
                    let result = if id == "loop" {
                        owner
                            .editor
                            .set_animation_preview_loop(document, !animation.preview_loop)
                    } else {
                        let mode = if animation.preview_mode == AnimationPreviewMode::Editing {
                            AnimationPreviewMode::Result
                        } else {
                            AnimationPreviewMode::Editing
                        };
                        owner.editor.set_animation_preview_mode(document, mode)
                    };
                    owner.animation_anchor = None;
                    owner.complete_edit(result, cx);
                });
            }
            "timeline-view" => self.change_view(
                |view| {
                    view.mode = if view.mode == TimelineDisplayMode::Curves {
                        TimelineDisplayMode::Overview
                    } else {
                        TimelineDisplayMode::Curves
                    }
                },
                cx,
            ),
            "timeline-pan" => {
                self.cancel(window, cx);
                self.pan_tool = !self.pan_tool;
                self.sync_status_bar(cx);
            }
            "timeline-snap" => self.change_view(|view| view.snap = !view.snap, cx),
            "timeline-fit" => self.fit(cx),
            "timeline-seconds" => self.seconds = !self.seconds,
            "timeline-normalized" => {
                self.change_view(|view| view.normalized = !view.normalized, cx)
            }
            "timeline-add-event" => self.add_event(window, cx),
            _ => unreachable!(),
        }
        cx.notify();
    }

    pub(super) fn row(
        &mut self,
        row: &TimelineRow,
        state: &TimelineViewState,
        projection: &TimelineProjection,
        axis_reference: Option<&editor_core::TimelineChannelId>,
        time: f64,
        top: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = row.id.clone();
        let selected = state.channels.contains(&id);
        let channel = row.channel.clone();
        let focused = state.mode == TimelineDisplayMode::Curves
            && channel.is_some()
            && channel.as_ref() == axis_reference;
        let group = row.channel.is_none() && row.id != TimelineRowId::Events
            || row
                .channel
                .as_ref()
                .is_some_and(|id| projection.channel(id).is_some_and(|c| c.tracks.len() > 1))
                && row.track_id.is_none();
        let expanded = !state.collapsed.contains(&id);
        let name = match &id {
            TimelineRowId::Group(None) => t!("parameter.ungrouped").into_owned(),
            TimelineRowId::Category(TimelineCategory::Model) => t!("timeline.model").into_owned(),
            TimelineRowId::Category(TimelineCategory::Parts) => t!("timeline.parts").into_owned(),
            TimelineRowId::Category(TimelineCategory::Unresolved) => {
                t!("timeline.unresolved").into_owned()
            }
            TimelineRowId::Events => t!("timeline.events").into_owned(),
            _ => row.name.clone(),
        };
        let mut element = div()
            .id(SharedString::from(format!("timeline-row-{id:?}")))
            .test_support()
            .absolute()
            .top(px(top))
            .left_0()
            .w_full()
            .h(px(self.row_height()))
            .flex()
            .items_center()
            .gap_1()
            .pl(compact::rem(6. + row.depth as f32 * 12.))
            .pr_1()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.4))
            .when(selected, |e| e.bg(cx.theme().primary.opacity(0.18)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if !this.allowed(window, cx) {
                        return;
                    }
                    window.focus(&this.focus, cx);
                    let id = id.clone();
                    let channel = channel.clone();
                    let additive = event.modifiers.shift || event.modifiers.secondary();
                    this.finish_number(cx);
                    let _ = this.owner.update(cx, |owner, cx| {
                        if let Some(document) = owner.editor.active_id() {
                            let _ = owner.editor.inspect_timeline_selection(document);
                            cx.notify();
                        }
                    });
                    this.change_view(
                        move |view| {
                            if !additive {
                                if view.mode != TimelineDisplayMode::Curves
                                    || !view.channels.contains(&id)
                                {
                                    view.channels.clear();
                                }
                                view.keys.clear();
                                view.events.clear();
                            }
                            view.curve_focus = if additive { None } else { channel };
                            if !view.channels.insert(id.clone()) && additive {
                                view.channels.remove(&id);
                            }
                        },
                        cx,
                    );
                }),
            );
        if group {
            let id = row.id.clone();
            element = element.child(
                Button::new(SharedString::from(format!("timeline-toggle-{id:?}")))
                    .ghost()
                    .xsmall()
                    .icon(if expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if this.allowed(window, cx) {
                            let id = id.clone();
                            this.change_view(
                                move |view| {
                                    if !view.collapsed.remove(&id) {
                                        view.collapsed.insert(id);
                                    }
                                },
                                cx,
                            );
                        }
                    })),
            );
        }
        let mut title = div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .text_xs()
                    .when(
                        state.mode == TimelineDisplayMode::Curves && selected,
                        |title| title.line_height(compact::rem(12.)),
                    )
                    .when(focused, |title| {
                        title.text_color(drawing::channel_color(
                            projection,
                            row.channel.as_ref().unwrap(),
                        ))
                    })
                    .child(name),
            );
        if state.mode == TimelineDisplayMode::Curves && selected {
            if let Some((min, max)) = row
                .channel
                .as_ref()
                .and_then(|id| projection.channel(id))
                .and_then(|channel| channel.range)
            {
                title = title.child(
                    div()
                        .text_size(compact::rem(9.))
                        .line_height(compact::rem(10.))
                        .text_color(cx.theme().muted_foreground)
                        .child(format!(
                            "{} … {}",
                            axis::number(f64::from(min)),
                            axis::number(f64::from(max))
                        )),
                );
            }
        }
        element = element.child(title);
        if row.id == TimelineRowId::Events {
            let fired = self
                .owner
                .upgrade()
                .map(|owner| {
                    owner
                        .read(cx)
                        .displayed_animation()
                        .map(|frame| {
                            frame
                                .fired_events
                                .iter()
                                .filter(|event| event.motion_id == projection.clip_id)
                                .map(|event| event.value.clone())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                })
                .unwrap_or_default();
            if !fired.is_empty() {
                let hint = fired.join("\n");
                element = element.child(
                    div()
                        .id("timeline-fired-events")
                        .test_support()
                        .flex_shrink_0()
                        .text_color(cx.theme().primary)
                        .child(t!("workspace.frame_events_count", count = fired.len()))
                        .tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(hint.clone())
                                .build(window, cx)
                        }),
                );
            }
        }
        if let Some(channel) = row.channel.as_ref().and_then(|id| projection.channel(id)) {
            if row.track_id.is_some() || channel.tracks.len() <= 1 {
                let value = projection
                    .raw_value(&channel.id, row.track_id.as_deref(), time)
                    .unwrap_or(channel.default_value);
                element = element.child(
                    div().w(compact::rem(62.)).flex_shrink_0().child(
                        self.number(
                            values::Field::RowValue(row.id.clone()),
                            value as f64,
                            channel
                                .range
                                .map(|(a, b)| (a as f64, b as f64))
                                .unwrap_or((-1e9, 1e9)),
                            !channel.editable || !self.editable(window, cx),
                            window,
                            cx,
                        ),
                    ),
                );
                let row = row.clone();
                let current = row
                    .keys
                    .iter()
                    .find(|key| {
                        projection
                            .key(key)
                            .is_some_and(|key| key.time == f64::from(time as f32))
                    })
                    .cloned();
                let enabled = channel.editable && self.editable(window, cx);
                element = element.child(
                    Button::new(SharedString::from(format!("timeline-insert-{:?}", row.id)))
                        .xsmall()
                        .ghost()
                        .label(if current.is_some() { "◆" } else { "◇" })
                        .disabled(!enabled)
                        .tooltip(t!("timeline.insert"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.execute(
                                TimelineCommand::InsertKey {
                                    channel: row.channel.clone().unwrap(),
                                    track_id: row.track_id.clone(),
                                    selected_key: current.clone(),
                                },
                                window,
                                cx,
                            );
                        })),
                );
            }
            if !channel.diagnostics.is_empty() {
                let diagnostic = channel
                    .diagnostics
                    .iter()
                    .map(|d| {
                        t!(match d {
                            TimelineDiagnostic::OpaqueClip => "timeline.opaque",
                            TimelineDiagnostic::UnresolvedTarget => "timeline.unresolved_hint",
                            TimelineDiagnostic::UnsupportedModelChannel => "timeline.unsupported",
                            TimelineDiagnostic::PartOpacityWithoutPose => "timeline.no_pose",
                            TimelineDiagnostic::NonMonotonicBezier => "timeline.non_monotonic",
                        })
                        .into_owned()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                element = element.child(
                    Button::new(SharedString::from(format!(
                        "timeline-diagnostic-{:?}",
                        row.id
                    )))
                    .xsmall()
                    .ghost()
                    .label("!")
                    .tooltip(diagnostic),
                );
            }
        }
        element.into_any_element()
    }
}

fn tool_icon(id: &str, fallback: IconName) -> gpui_kit::component::Icon {
    if id == "timeline-view" {
        gpui_kit::component::Icon::default().data(include_bytes!(
            "../../../../../assets/editor/icons/bezier-curve.svg"
        ))
    } else {
        gpui_kit::component::Icon::new(fallback)
    }
}

fn tool_command(id: &str) -> Option<editor_core::CommandId> {
    use editor_core::CommandId::*;
    Some(match id {
        "start" => TimelineStart,
        "previous" => TimelinePreviousKey,
        "next" => TimelineNextKey,
        "end" => TimelineEnd,
        "timeline-previous-frame" => TimelinePreviousFrame,
        "timeline-next-frame" => TimelineNextFrame,
        _ => return None,
    })
}

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
        let allowed = self.allowed(window, cx);
        let editable = self.editable(window, cx);
        let actions = [
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
                if state.mode == TimelineDisplayMode::Curves {
                    IconName::ChartLine
                } else {
                    IconName::List
                },
                t!(if state.mode == TimelineDisplayMode::Curves {
                    "timeline.curves"
                } else {
                    "timeline.overview"
                })
                .into_owned(),
                state.mode == TimelineDisplayMode::Curves,
                allowed,
            ),
            (
                "timeline-pan",
                IconName::Hand,
                t!("timeline.pan_hint").into_owned(),
                self.pan_tool,
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
                "timeline-properties",
                IconName::Settings,
                t!("timeline.properties").into_owned(),
                self.show_properties,
                allowed,
            ),
            (
                "timeline-add-event",
                IconName::MessageSquarePlus,
                t!("timeline.add_event").into_owned(),
                false,
                editable,
            ),
            (
                "timeline-delete",
                IconName::Trash,
                t!("timeline.delete").into_owned(),
                false,
                editable && (!state.keys.is_empty() || !state.events.is_empty()),
            ),
            (
                "timeline-inspector-toggle",
                IconName::PanelRight,
                t!("timeline.inspector").into_owned(),
                self.inspector_visible(),
                allowed,
            ),
        ];
        let mut tools = div()
            .id("timeline-tools")
            .test_support()
            .flex()
            .items_center()
            .gap(px(2.))
            .flex_shrink_0();
        for (id, icon, label, selected, enabled) in &actions {
            if !self.compact && matches!(*id, "timeline-previous-frame" | "timeline-next-frame") {
                continue;
            }
            if self.compact && !matches!(*id, "timeline-pan" | "timeline-inspector-toggle") {
                continue;
            }
            let id = *id;
            tools = tools.child(
                Button::new(id)
                    .small()
                    .when(self.compact, |button| button.xsmall())
                    .ghost()
                    .icon(*icon)
                    .accessibility_label(label.clone())
                    .tooltip(label.clone())
                    .selected(*selected)
                    .disabled(!enabled)
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.activate_tool(id, window, cx)),
                    ),
            );
        }
        if self.compact {
            let weak = cx.weak_entity();
            tools = tools.child(
                Button::new("timeline-more")
                    .xsmall()
                    .ghost()
                    .icon(IconName::Ellipsis)
                    .accessibility_label(t!("timeline.more_tools").into_owned())
                    .tooltip(t!("timeline.more_tools"))
                    .disabled(!allowed)
                    .dropdown_menu(move |mut menu, _, _| {
                        for (id, icon, label, selected, enabled) in &actions {
                            if matches!(*id, "timeline-pan" | "timeline-inspector-toggle") {
                                continue;
                            }
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
                                .icon(*icon)
                                .checked(*selected)
                                .disabled(!enabled)
                                .on_click(move |_, window, cx| {
                                    let _ = weak
                                        .update(cx, |this, cx| this.activate_tool(id, window, cx));
                                }),
                            );
                        }
                        menu
                    }),
            );
        }
        tools.into_any_element()
    }

    fn activate_tool(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !self.allowed(window, cx) {
            return;
        }
        self.finish_number(cx);
        window.focus(&self.focus, cx);
        match id {
            "timeline-previous-frame" => self.key_command("left", window, cx),
            "timeline-next-frame" => self.key_command("right", window, cx),
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
            }
            "timeline-snap" => self.change_view(|view| view.snap = !view.snap, cx),
            "timeline-fit" => self.fit(cx),
            "timeline-seconds" => self.seconds = !self.seconds,
            "timeline-normalized" => {
                self.change_view(|view| view.normalized = !view.normalized, cx)
            }
            "timeline-properties" => {
                self.show_properties = !self.show_properties;
                self.inspector_open = Some(true);
            }
            "timeline-inspector-toggle" => self.inspector_open = Some(!self.inspector_visible()),
            "timeline-add-event" => self.add_event(window, cx),
            "timeline-delete" => self.key_command("delete", window, cx),
            _ => unreachable!(),
        }
        cx.notify();
    }

    pub(super) fn row(
        &mut self,
        row: &TimelineRow,
        state: &TimelineViewState,
        projection: &TimelineProjection,
        time: f64,
        top: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = row.id.clone();
        let selected = state.channels.contains(&id);
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
            .h(px(ROW))
            .flex()
            .items_center()
            .gap_1()
            .pl(px(6. + row.depth as f32 * 12.))
            .pr_1()
            .border_b_1()
            .border_color(cx.theme().border.opacity(0.4))
            .when(selected, |e| e.bg(cx.theme().accent.opacity(0.18)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if !this.allowed(window, cx) {
                        return;
                    }
                    window.focus(&this.focus, cx);
                    let id = id.clone();
                    let additive = event.modifiers.shift || event.modifiers.platform;
                    this.finish_number(cx);
                    this.change_view(
                        move |view| {
                            if !additive {
                                view.channels.clear();
                                view.keys.clear();
                                view.events.clear();
                            }
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
                    .small()
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
        element = element.child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .text_xs()
                .child(name),
        );
        if let Some(channel) = row.channel.as_ref().and_then(|id| projection.channel(id)) {
            if row.track_id.is_some() || channel.tracks.len() <= 1 {
                let value = projection
                    .raw_value(&channel.id, row.track_id.as_deref(), time)
                    .unwrap_or(channel.default_value);
                element = element.child(
                    div().w(px(62.)).flex_shrink_0().child(
                        self.number(
                            inspector::Field::RowValue(row.id.clone()),
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
                        .small()
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
                    .small()
                    .ghost()
                    .label("!")
                    .tooltip(diagnostic),
                );
            }
        }
        element.into_any_element()
    }
}

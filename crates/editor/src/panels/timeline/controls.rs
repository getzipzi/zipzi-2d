use super::*;

impl Timeline {
    pub(super) fn tools(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let owner = self.owner.upgrade().unwrap();
        let view = owner.read(cx);
        let state = view.editor.timeline_view().cloned().unwrap_or_default();
        let document = view.editor.active_id();
        let current = view.editor.animation().and_then(|a| a.motion.as_ref());
        let motions = view
            .editor
            .document()
            .map(|d| d.animation_catalog().motions.clone())
            .unwrap_or_default();
        let name = motions
            .iter()
            .find(|motion| Some(&motion.selection) == current)
            .map(|motion| motion.name.clone())
            .unwrap_or_else(|| t!("timeline.select_motion").into_owned());
        let selected_motion = current.cloned();
        let allowed = self.allowed(window, cx);
        let compact_motions = motions.clone();
        let compact_owner = self.owner.clone();
        let mut tools = div()
            .h(px(34.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .overflow_hidden();
        let weak = self.owner.clone();
        tools = tools
            .child(
                Button::new("timeline-motion")
                    .small()
                    .ghost()
                    .label(name.clone())
                    .tooltip(name)
                    .max_w(px(if self.compact { 130. } else { 220. }))
                    .overflow_hidden()
                    .disabled(!allowed)
                    .dropdown_menu(move |mut menu, _, _| {
                        for motion in &motions {
                            let weak = weak.clone();
                            let selection = motion.selection.clone();
                            menu = menu.item(PopupMenuItem::new(motion.name.clone()).on_click(
                                move |_, window, cx| {
                                    if window.has_active_dialog(cx) {
                                        return;
                                    }
                                    let _ = weak.update(cx, |owner, cx| {
                                        if let Some(document) = document {
                                            let result = owner
                                                .editor
                                                .select_motion(document, selection.clone());
                                            owner.animation_anchor = None;
                                            owner.complete_edit(result, cx);
                                        }
                                    });
                                },
                            ));
                        }
                        menu
                    }),
            )
            .child(
                Button::new("timeline-new-motion")
                    .small()
                    .ghost()
                    .icon(IconName::Plus)
                    .tooltip(t!("timeline.new_motion"))
                    .disabled(!allowed)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if !this.allowed(window, cx) {
                            return;
                        }
                        let Some(target) = this.target(cx) else {
                            return;
                        };
                        let _ = this.owner.update(cx, |owner, cx| {
                            let result = owner
                                .editor
                                .create_timeline_motion(target.document)
                                .map(|_| ());
                            owner.complete_edit(result, cx);
                        });
                    })),
            );
        let inspector = Button::new("timeline-inspector-toggle")
            .small()
            .ghost()
            .icon(IconName::PanelRight)
            .selected(self.inspector_visible())
            .tooltip(t!("timeline.inspector"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.finish_number(cx);
                this.inspector_open = Some(!this.inspector_visible());
                cx.notify();
            }));
        if self.compact {
            let weak = cx.weak_entity();
            let seconds = self.seconds;
            let properties = self.show_properties;
            return div()
                .h(px(32.))
                .flex()
                .items_center()
                .gap_1()
                .px_1()
                .flex_shrink_0()
                .child(inspector)
                .child(
                    Button::new("timeline-more")
                        .small()
                        .ghost()
                        .icon(IconName::Ellipsis)
                        .tooltip(t!("timeline.more_tools"))
                        .disabled(!allowed)
                        .dropdown_menu(move |mut menu, window, cx| {
                            let motions = compact_motions.clone();
                            let selected = selected_motion.clone();
                            let owner = compact_owner.clone();
                            menu = menu.submenu(
                                t!("timeline.select_motion").into_owned(),
                                window,
                                cx,
                                move |mut menu, _, _| {
                                    for motion in &motions {
                                        let selection = motion.selection.clone();
                                        let owner = owner.clone();
                                        menu = menu.item(
                                            PopupMenuItem::new(motion.name.clone())
                                                .checked(selected.as_ref() == Some(&selection))
                                                .on_click(move |_, window, cx| {
                                                    if window.has_active_dialog(cx) {
                                                        return;
                                                    }
                                                    let _ = owner.update(cx, |owner, cx| {
                                                        if let Some(document) = document {
                                                            let result =
                                                                owner.editor.select_motion(
                                                                    document,
                                                                    selection.clone(),
                                                                );
                                                            owner.animation_anchor = None;
                                                            owner.complete_edit(result, cx);
                                                        }
                                                    });
                                                }),
                                        );
                                    }
                                    menu
                                },
                            );
                            for (id, label, checked, enabled) in [
                                (
                                    "overview",
                                    t!("timeline.overview"),
                                    state.mode == TimelineDisplayMode::Overview,
                                    true,
                                ),
                                (
                                    "curves",
                                    t!("timeline.curves"),
                                    state.mode == TimelineDisplayMode::Curves,
                                    true,
                                ),
                                ("snap", t!("timeline.snap"), state.snap, true),
                                ("units", t!("timeline.units"), seconds, true),
                                ("fit", t!("timeline.fit"), false, true),
                                (
                                    "normalize",
                                    t!("timeline.normalize"),
                                    state.normalized,
                                    state.mode == TimelineDisplayMode::Curves,
                                ),
                                ("properties", t!("timeline.properties"), properties, true),
                                ("new-motion", t!("timeline.new_motion"), false, true),
                            ] {
                                let weak = weak.clone();
                                let label = label.into_owned();
                                menu = menu.item(
                                    PopupMenuItem::element(move |_, _| {
                                        div()
                                            .id(SharedString::from(format!("timeline-menu-{id}")))
                                            .test_support()
                                            .child(label.clone())
                                    })
                                    .checked(checked)
                                    .disabled(!enabled)
                                    .on_click(
                                        move |_, window, cx| {
                                            let _ = weak.update(cx, |this, cx| {
                                                if !this.allowed(window, cx) {
                                                    return;
                                                }
                                                this.finish_number(cx);
                                                match id {
                                                    "overview" => this.change_view(
                                                        |view| {
                                                            view.mode =
                                                                TimelineDisplayMode::Overview
                                                        },
                                                        cx,
                                                    ),
                                                    "curves" => this.change_view(
                                                        |view| {
                                                            view.mode = TimelineDisplayMode::Curves
                                                        },
                                                        cx,
                                                    ),
                                                    "snap" => this.change_view(
                                                        |view| view.snap = !view.snap,
                                                        cx,
                                                    ),
                                                    "units" => this.seconds = !this.seconds,
                                                    "fit" => this.fit(cx),
                                                    "normalize" => this.change_view(
                                                        |view| view.normalized = !view.normalized,
                                                        cx,
                                                    ),
                                                    "properties" => {
                                                        this.show_properties =
                                                            !this.show_properties;
                                                        this.inspector_open = Some(true);
                                                    }
                                                    "new-motion" => {
                                                        if let Some(target) = this.target(cx) {
                                                            let _ = this.owner.update(
                                                                cx,
                                                                |owner, cx| {
                                                                    let result = owner
                                                                        .editor
                                                                        .create_timeline_motion(
                                                                            target.document,
                                                                        )
                                                                        .map(|_| ());
                                                                    owner.complete_edit(result, cx);
                                                                },
                                                            );
                                                        }
                                                    }
                                                    _ => unreachable!(),
                                                }
                                                cx.notify();
                                            });
                                        },
                                    ),
                                );
                            }
                            menu
                        }),
                )
                .into_any_element();
        }
        for (id, mode, label) in [
            (
                "timeline-overview",
                TimelineDisplayMode::Overview,
                t!("timeline.overview"),
            ),
            (
                "timeline-curves",
                TimelineDisplayMode::Curves,
                t!("timeline.curves"),
            ),
        ] {
            tools = tools.child(
                Button::new(id)
                    .small()
                    .ghost()
                    .label(label)
                    .selected(state.mode == mode)
                    .disabled(!allowed || document.is_none())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if this.allowed(window, cx) {
                            this.change_view(|view| view.mode = mode, cx);
                        }
                    })),
            );
        }
        tools = tools
            .child(
                Button::new("timeline-snap")
                    .small()
                    .ghost()
                    .label(t!("timeline.snap"))
                    .selected(state.snap)
                    .disabled(!allowed)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.allowed(window, cx) {
                            this.change_view(|view| view.snap = !view.snap, cx);
                        }
                    })),
            )
            .child(
                Button::new("timeline-seconds")
                    .small()
                    .ghost()
                    .label(if self.seconds { "s" } else { "f" })
                    .tooltip(t!("timeline.units"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.seconds = !this.seconds;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("timeline-fit")
                    .small()
                    .ghost()
                    .label(t!("timeline.fit"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.allowed(window, cx) {
                            this.fit(cx);
                        }
                    })),
            )
            .child(
                Button::new("timeline-normalized")
                    .small()
                    .ghost()
                    .label(t!("timeline.normalize"))
                    .selected(state.normalized)
                    .disabled(state.mode != TimelineDisplayMode::Curves)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.allowed(window, cx) {
                            this.change_view(|view| view.normalized = !view.normalized, cx);
                        }
                    })),
            )
            .child(
                Button::new("timeline-properties")
                    .small()
                    .ghost()
                    .icon(IconName::Settings)
                    .selected(self.show_properties)
                    .tooltip(t!("timeline.properties"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_properties = !this.show_properties;
                        this.inspector_open = Some(true);
                        cx.notify();
                    })),
            )
            .child(inspector);
        tools.into_any_element()
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

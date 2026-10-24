//! Timeline layout, header and shared input attachment.
use super::*;

impl Timeline {
    // Header controls are siblings of the content now. Both regions must use
    // the same edit guards, commit-on-click behavior and keyboard actions.
    fn interactive_frame<E: StatefulInteractiveElement>(
        &self,
        frame: E,
        cx: &mut Context<Self>,
    ) -> E {
        let pointer_target = self.target(cx);
        let release_target = pointer_target.clone();
        frame
            .capture_any_mouse_down(cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                if this.target(cx) != pointer_target {
                    cx.stop_propagation();
                    return;
                }
                this.finish_number_outside(event.position, cx);
            }))
            .capture_any_mouse_up(cx.listener(move |this, _: &MouseUpEvent, _, cx| {
                let same = this
                    .target(cx)
                    .as_ref()
                    .zip(release_target.as_ref())
                    .is_some_and(|(a, b)| {
                        a.document == b.document
                            && a.binding == b.binding
                            && a.workspace == b.workspace
                            && a.clip == b.clip
                            && a.mode == b.mode
                    });
                if !same {
                    cx.stop_propagation();
                }
            }))
            .capture_key_down(cx.listener(Self::escape))
            .capture_action(
                cx.listener(|this, action: &crate::actions::RunCommand, window, cx| {
                    if !this.allowed(window, cx) {
                        cx.propagate();
                        return;
                    }
                    if let Some(command) = action.invocation {
                        if this
                            .owner
                            .upgrade()
                            .is_none_or(|owner| !owner.read(cx).editor.command_available(command))
                        {
                            cx.stop_propagation();
                            return;
                        }
                    }
                    match action.id {
                        zipzi_editor_core::CommandId::CancelProperty
                            if this.number_edit.is_some() || this.gesture.is_some() =>
                        {
                            this.cancel(window, cx);
                            window.focus(&this.focus, cx);
                            cx.stop_propagation();
                        }
                        zipzi_editor_core::CommandId::CommitProperty
                            if this.number_edit.is_some() =>
                        {
                            this.finish_number(cx);
                            window.focus(&this.focus, cx);
                            cx.stop_propagation();
                        }
                        _ => cx.propagate(),
                    }
                }),
            )
            .capture_action(cx.listener(
                |this, _: &gpui_kit::component::input::Escape, window, cx| {
                    if this.allowed(window, cx)
                        && (this.gesture.is_some() || this.number_edit.is_some())
                    {
                        this.cancel(window, cx);
                        window.focus(&this.focus, cx);
                        cx.stop_propagation();
                    } else {
                        cx.propagate();
                    }
                },
            ))
            .on_action(
                cx.listener(|this, action: &crate::actions::RunCommand, window, cx| {
                    if action.id.spec().scope == zipzi_editor_core::CommandScope::Timeline {
                        let command = action.invocation.or_else(|| {
                            this.target(cx).and_then(|target| {
                                this.owner.upgrade().map(|owner| {
                                    owner.read(cx).editor.command_for_timeline(
                                        action.id,
                                        target.binding,
                                        zipzi_editor_core::CommandSource::Direct,
                                    )
                                })
                            })
                        });
                        if let Some(command) = command {
                            this.user_invocation(command, window, cx);
                        }
                    } else {
                        cx.propagate();
                    }
                }),
            )
    }
}

impl crate::docking::header::PanelHeader for Timeline {
    fn header_filtered(&self, cx: &App) -> bool {
        !self.search.read(cx).value().is_empty()
    }

    fn header_min_width(&self) -> Pixels {
        px(120.)
    }

    fn header_resized(&mut self, width: Pixels, cx: &mut Context<Self>) {
        let compact = width < compact::scaled(COMPACT_WIDTH, cx);
        let tiny = width < crate::ui::compact::scaled(360., cx);
        if self.header_compact != compact || self.header_tiny != tiny {
            self.header_tiny = tiny;
            self.header_compact = compact;
            cx.notify();
        }
    }

    fn render_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.sync(window, cx);
        let toolbar = self.toolbar(window, cx);
        self.interactive_frame(
            div()
                .id("timeline-header")
                .test_support()
                .flex()
                .items_center()
                .w_full()
                .h_full()
                .track_focus(&self.focus)
                .key_context("ZipziTimeline"),
            cx,
        )
        .child(toolbar)
        .into_any_element()
    }
}

impl Render for Timeline {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(any(test, feature = "test-support"))]
        {
            self.renders += 1;
        }
        self.sync(window, cx);
        if self.graph.get().size.height > px(0.) {
            self.body_height = self.graph.get().size.height.as_f32();
        }
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let projection = owner.read(cx).editor.timeline_projection();
        let mut state = owner
            .read(cx)
            .editor
            .timeline_view()
            .cloned()
            .unwrap_or_default();
        let rows = owner
            .read(cx)
            .editor
            .timeline_visible_rows()
            .unwrap_or_else(|| Arc::from([]));
        state.scroll = state
            .scroll
            .min((rows.len() as f64 * self.row_height() as f64 - self.body_height as f64).max(0.));
        let time = owner.read(cx).editor.animation().map_or(0., |a| {
            if a.playing {
                a.presented
                    .as_ref()
                    .and_then(|frame| {
                        frame
                            .motion_positions
                            .iter()
                            .find(|p| a.motion.as_ref().is_some_and(|m| m.clip_id == p.motion_id))
                    })
                    .map_or(a.edit_time, |p| p.local_time)
            } else {
                a.edit_time
            }
        });
        let panel_entity = cx.weak_entity();
        let left_width = (if self.compact { COMPACT_LEFT } else { LEFT }) * self.ui_scale;
        let panel = div()
            .id("animation-timeline")
            .test_support()
            .relative()
            .size_full()
            .min_h_0()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_xs()
            .track_focus(&self.focus)
            .key_context("ZipziTimeline")
            .on_prepaint(move |bounds, window, cx| {
                let compact = bounds.size.width < compact::scaled(COMPACT_WIDTH, cx);
                if panel_entity
                    .upgrade()
                    .is_some_and(|panel| panel.read(cx).compact != compact)
                {
                    window.defer(cx, move |_, cx| {
                        let _ = panel_entity.update(cx, |panel, cx| {
                            if panel.compact != compact {
                                panel.compact = compact;
                                cx.notify();
                            }
                        });
                    });
                }
            });
        let mut panel = self.interactive_frame(panel, cx);
        panel = panel.child(self.input_region.marker());
        let weak = cx.weak_entity();
        panel = panel.child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let pressing = weak.clone();
                    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture
                            && event.button == MouseButton::Left
                            && !bounds.contains(&event.position)
                        {
                            let _ = pressing.update(cx, |this, cx| {
                                if this.allowed(window, cx) {
                                    this.finish_number_outside(event.position, cx);
                                }
                            });
                        }
                    });
                    let moving = weak.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ =
                                moving.update(cx, |this, cx| this.move_pointer(event, window, cx));
                        }
                    });
                    let releasing = weak.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture
                            && matches!(event.button, MouseButton::Left | MouseButton::Middle)
                        {
                            let _ =
                                releasing.update(cx, |this, cx| this.release_pointer(window, cx));
                        }
                    });
                },
            )
            .absolute()
            .inset_0()
            .size_full(),
        );
        let Some(projection) = projection else {
            return panel
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(t!("timeline.empty")),
                )
                .into_any_element();
        };
        self.update_range(&projection, &state);
        if let Some(reveal) = &state.reveal_row {
            if let Some(index) = rows.iter().position(|row| &row.id == reveal) {
                let top = index as f64 * self.row_height() as f64;
                let scroll = if top < state.scroll {
                    top
                } else if top + self.row_height() as f64 > state.scroll + self.body_height as f64 {
                    (top + self.row_height() as f64 - self.body_height as f64).max(0.)
                } else {
                    state.scroll
                };
                let document = self.context.as_ref().map(|c| c.document);
                let weak = self.owner.clone();
                let reveal = reveal.clone();
                cx.defer(move |cx| {
                    let _ = weak.update(cx, |owner, cx| {
                        if let Some(document) = document {
                            let _ = owner
                                .editor
                                .acknowledge_timeline_reveal(document, &reveal, scroll);
                            owner.redraw(cx);
                        }
                    });
                });
            }
        }
        let ruler_bounds = self.ruler.clone();
        let mut left = div()
            .id("timeline-rows")
            .test_support()
            .w(px(left_width))
            .h_full()
            .relative()
            .overflow_hidden()
            .flex_shrink_0()
            .border_r_1()
            .border_color(cx.theme().border);
        let first = (state.scroll as f32 / self.row_height()).floor() as usize;
        let count = (self.body_height / self.row_height()).ceil() as usize + 2;
        let axis_channels = axis::channels(&projection, &state);
        let axis_reference = axis::reference(&axis_channels, &state);
        for (index, row) in rows.iter().enumerate().skip(first).take(count) {
            left = left.child(self.row(
                row,
                &state,
                &projection,
                axis_reference.map(|channel| &channel.id),
                f64::from(time),
                index as f32 * self.row_height() - state.scroll as f32,
                window,
                cx,
            ));
        }
        let range = self.value_range;
        let head = time;
        let seconds = self.seconds;
        let head_range = state.time_range;
        let marquee = self.gesture.as_ref().and_then(|gesture| match gesture {
            input::Gesture::Marquee {
                origin, current, ..
            } => Some((*origin, *current)),
            _ => None,
        });
        let primary = cx.theme().primary;
        let graph_bounds = self.graph.clone();
        self.tracks.update(cx, |tracks, cx| {
            tracks.configure(
                projection.clone(),
                state.clone(),
                rows.clone(),
                range,
                seconds,
                graph_bounds,
                cx,
            )
        });
        let ruler_projection = projection.clone();
        let ruler_state = state.clone();
        let axis_caption = axis_reference.map_or_else(
            || {
                if state.normalized {
                    t!("timeline.normalized_axis").into_owned()
                } else {
                    t!("timeline.value").into_owned()
                }
            },
            |channel| channel.name.clone(),
        );
        let axis_color = axis_reference.map_or(cx.theme().muted_foreground, |channel| {
            drawing::channel_color(&projection, &channel.id)
        });
        let graph = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("animation-seek")
                    .test_support()
                    .h(compact::rem(RULER))
                    .w_full()
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("timeline-ruler")
                            .test_support()
                            .size_full()
                            .cursor(CursorStyle::ResizeLeftRight)
                            .on_mouse_down(MouseButton::Left, cx.listener(Self::start_seek))
                            .child(
                                canvas(
                                    move |bounds, _, _| {
                                        ruler_bounds.set(bounds);
                                    },
                                    move |bounds, _, window, cx| {
                                        drawing::paint_ruler(
                                            bounds,
                                            &ruler_projection,
                                            &ruler_state,
                                            head,
                                            seconds,
                                            window,
                                            cx,
                                        );
                                    },
                                )
                                .size_full(),
                            ),
                    ),
            )
            .child(
                div()
                    .id("timeline-graph")
                    .test_support()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .overflow_hidden()
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::start_graph))
                    .on_mouse_down(MouseButton::Middle, cx.listener(Self::start_pan))
                    .when(self.pan_tool, |graph| graph.cursor(CursorStyle::ClosedHand))
                    .on_scroll_wheel(cx.listener(Self::wheel))
                    .child(
                        self.tracks
                            .clone()
                            .cached(StyleRefinement::default().size_full()),
                    )
                    .when(
                        state.mode == TimelineDisplayMode::Curves && !axis_channels.is_empty(),
                        |graph| {
                            graph.child(
                                div()
                                    .id("timeline-axis-units")
                                    .test_support()
                                    .absolute()
                                    .top(compact::rem(4.))
                                    .right(compact::rem(8.))
                                    .max_w(compact::rem(180.))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_size(compact::rem(10.))
                                    .text_color(axis_color)
                                    .bg(cx.theme().background)
                                    .child(axis_caption),
                            )
                        },
                    )
                    .child(
                        div()
                            .id("timeline-overlay")
                            .test_support()
                            .absolute()
                            .inset_0()
                            .size_full()
                            .child(
                                canvas(
                                    |_, _, _| (),
                                    move |bounds, _, window, _| {
                                        drawing::paint_head(
                                            bounds, head_range, head, marquee, primary, window,
                                        )
                                    },
                                )
                                .size_full(),
                            ),
                    ),
            );
        let body = div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .overflow_hidden()
            .child(
                div()
                    .w(px(left_width))
                    .flex_shrink_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .h(compact::rem(RULER))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_1()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .child(
                                crate::panels::toolbar::PanelToolbarSearch::new(&self.search)
                                    .id("timeline-search")
                                    .label(t!("timeline.search").into_owned()),
                            )
                            .child(
                                Button::new("timeline-filter")
                                    .small()
                                    .ghost()
                                    .icon(IconName::ListFilter)
                                    .selected(state.filter == TimelineFilter::Animated)
                                    .tooltip(if state.filter == TimelineFilter::Animated {
                                        t!("timeline.animated")
                                    } else {
                                        t!("timeline.all")
                                    })
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        if this.allowed(window, cx) {
                                            this.change_view(
                                                |view| {
                                                    view.filter =
                                                        if view.filter == TimelineFilter::Animated {
                                                            TimelineFilter::All
                                                        } else {
                                                            TimelineFilter::Animated
                                                        }
                                                },
                                                cx,
                                            );
                                        }
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .on_scroll_wheel(cx.listener(Self::wheel))
                            .child(left),
                    ),
            )
            .child(graph);
        panel = panel.child(body);
        panel.into_any_element()
    }
}

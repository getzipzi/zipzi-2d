//! Resource browsing shares the toolbar and compact rows used by Modeling panels.
use super::*;
use crate::panels::group_header;
use crate::panels::toolbar::{PanelToolbar, PanelToolbarSearch};
use editor_core::{CommandId, DocumentId, MotionSelection};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Icon,
    input::{InputEvent, InputState},
};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
struct ViewState {
    query: String,
    collapsed: HashSet<Option<String>>,
    cursor: Option<usize>,
    scroll: ScrollHandle,
}

pub(super) struct ResourceList {
    search: Entity<InputState>,
    placeholder: String,
    _search_subscription: Subscription,
    document: Option<DocumentId>,
    view: ViewState,
    documents: HashMap<DocumentId, ViewState>,
    feedback: (bool, bool, Vec<String>),
}

impl ResourceList {
    pub fn new(
        kind: AnimationPanelKind,
        window: &mut Window,
        cx: &mut Context<AnimationPanel>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(search_label(kind)));
        let subscription = cx.subscribe(&search, |panel, search, event, cx| {
            if matches!(event, InputEvent::Change) {
                let query = search.read(cx).value().to_string();
                if panel.resources.view.query != query {
                    panel.resources.view.query = query;
                    panel.resources.view.cursor = None;
                    panel.resources.view.scroll = ScrollHandle::new();
                    cx.notify();
                }
            }
        });
        Self {
            search,
            placeholder: search_label(kind),
            _search_subscription: subscription,
            document: None,
            view: ViewState::default(),
            documents: HashMap::new(),
            feedback: Default::default(),
        }
    }
}

fn search_label(kind: AnimationPanelKind) -> String {
    match kind {
        AnimationPanelKind::Motions => t!("workspace.search_motions"),
        _ => t!("workspace.search_expressions"),
    }
    .into_owned()
}

#[derive(Clone)]
enum Choice {
    Motion(MotionSelection),
    Expression(String),
}

impl AnimationPanel {
    /// Presentation ticks only redraw these lists when their discrete feedback changes.
    pub(crate) fn refresh_feedback(
        &mut self,
        playing: bool,
        failed: bool,
        active: &[String],
        cx: &mut Context<Self>,
    ) {
        if self.resources.feedback.0 != playing
            || self.resources.feedback.1 != failed
            || self.resources.feedback.2 != active
        {
            self.resources.feedback = (playing, failed, active.to_vec());
            cx.notify();
        }
    }

    fn choose(
        &mut self,
        document: DocumentId,
        choice: Choice,
        play: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        window.focus(&self.focus, cx);
        let _ = self.owner.update(cx, |owner, cx| {
            if owner.modal || owner.editor.is_busy() || owner.editor.active_id() != Some(document) {
                return;
            }
            // A playing row pauses via the same command as the transport. Every
            // other inline Play starts a fresh run, including a paused selection.
            if play
                && let Choice::Motion(selection) = &choice
                && owner
                    .editor
                    .animation()
                    .is_some_and(|a| a.playing && a.motion.as_ref() == Some(selection))
            {
                owner.run_command(CommandId::PlayPause, window, cx);
                return;
            }
            let result = match choice {
                Choice::Motion(selection) => owner.editor.select_motion(document, selection),
                Choice::Expression(id) => owner.editor.select_expression(document, id),
            };
            let start = play && result.is_ok();
            owner.animation_anchor = None;
            owner.complete_edit(result, cx);
            if start {
                owner.run_command(CommandId::PlayPause, window, cx);
            }
        });
        cx.notify();
    }

    pub(super) fn render_resources(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let document = owner.read(cx).editor.active_id();
        if self.resources.document != document {
            let old = std::mem::take(&mut self.resources.view);
            if let Some(id) = self.resources.document {
                self.resources.documents.insert(id, old);
            }
            self.resources.view = document
                .and_then(|id| self.resources.documents.remove(&id))
                .unwrap_or_default();
            self.resources.document = document;
            let query = self.resources.view.query.clone();
            self.resources
                .search
                .update(cx, |input, cx| input.set_value(query, window, cx));
        }
        let placeholder = search_label(self.kind);
        if self.resources.placeholder != placeholder {
            self.resources.placeholder = placeholder.clone();
            self.resources.search.update(cx, |input, cx| {
                input.set_placeholder(placeholder, window, cx)
            });
        }
        let dialog = window.has_active_dialog(cx);
        let state = owner.read(cx);
        let open_documents = state.editor.documents();
        self.resources
            .documents
            .retain(|id, _| open_documents.iter().any(|doc| doc.id == *id));
        let Some(id) = document else {
            return div()
                .p_3()
                .text_xs()
                .child(t!("editor.welcome"))
                .into_any_element();
        };
        let catalog = state.editor.document().unwrap().animation_catalog();
        let animation = state.editor.animation().unwrap();
        let disabled = state.modal || state.editor.is_busy() || dialog;
        let motion_panel = matches!(self.kind, AnimationPanelKind::Motions);
        let prefix = if motion_panel {
            "animation-motion"
        } else {
            "animation-expression"
        };
        let theme = cx.theme().clone();
        let query = self.resources.view.query.trim().to_lowercase();
        let filtering = !query.is_empty();
        let active = state
            .displayed_animation()
            .map(|s| s.active_expressions.as_slice())
            .unwrap_or_default();
        let panel = cx.weak_entity();
        let search = PanelToolbarSearch::new(&self.resources.search)
            .id(SharedString::from(format!("{prefix}-search")))
            .label(search_label(self.kind))
            .disabled(disabled)
            .reset_action(
                SharedString::from(format!("{prefix}-search-clear")),
                t!("workspace.clear_search"),
                filtering,
                move |_, window, cx| {
                    let _ = panel.update(cx, |panel, cx| {
                        panel.resources.view.query.clear();
                        panel.resources.view.cursor = None;
                        panel.resources.view.scroll = ScrollHandle::new();
                        cx.notify();
                        panel
                            .resources
                            .search
                            .update(cx, |input, cx| input.set_value("", window, cx));
                    });
                },
            );
        let mut toolbar = PanelToolbar::new(SharedString::from(format!("{prefix}-toolbar")));
        if !motion_panel {
            let owner = self.owner.clone();
            let unavailable = disabled
                || !state
                    .editor
                    .command_available(state.editor.command(CommandId::ClearExpression));
            toolbar = toolbar.action(move |compact| {
                Button::new("animation-clear-expression")
                    .icon(IconName::X)
                    .ghost()
                    .when(!compact, |button| {
                        button.label(t!("workspace.clear_expression"))
                    })
                    .accessibility_label(t!("workspace.clear_expression"))
                    .tooltip(t!("workspace.clear_hint"))
                    .disabled(unavailable)
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |owner, cx| {
                            if owner.editor.active_id() == Some(id) {
                                owner.run_command(CommandId::ClearExpression, window, cx);
                            }
                        });
                    })
            });
        }
        toolbar = toolbar.search(search);
        let mut list = div()
            .id(SharedString::from(format!("{prefix}-list")))
            .test_support()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_y_scroll()
            .track_scroll(&self.resources.view.scroll)
            .when(!motion_panel, |list| list.p_1());
        let mut choices = Vec::new();
        let mut selected_cursor = None;
        let mut row_index = 0;
        let mut matches = 0;
        if motion_panel {
            let mut previous: Option<Option<String>> = None;
            for (index, resource) in catalog.motions.iter().enumerate() {
                let group = resource
                    .selection
                    .entry
                    .as_ref()
                    .map(|(name, _)| name.clone());
                let group_name = match &group {
                    None => t!("workspace.ungrouped").into_owned(),
                    Some(name) if name.is_empty() => t!("workspace.unnamed_group").into_owned(),
                    Some(name) => name.clone(),
                };
                if filtering
                    && !resource.name.to_lowercase().contains(&query)
                    && !group_name.to_lowercase().contains(&query)
                {
                    continue;
                }
                matches += 1;
                let expanded = filtering || !self.resources.view.collapsed.contains(&group);
                if previous.as_ref() != Some(&group) {
                    let count = catalog
                        .motions
                        .iter()
                        .filter(|r| {
                            r.selection.entry.as_ref().map(|(name, _)| name) == group.as_ref()
                        })
                        .count();
                    let key = group.clone();
                    list = list.child(
                        div()
                            .id(("animation-motion-group-header", index))
                            .test_support()
                            .role(gpui::Role::Group)
                            .aria_label(group_name.clone())
                            .aria_expanded(expanded)
                            .map(|element| group_header::row_style(element, 0, cx))
                            .child(
                                group_header::toggle(
                                    ("animation-motion-group", index),
                                    ("animation-motion-group-count", index),
                                    group_name,
                                    count,
                                    expanded,
                                    0,
                                    cx,
                                )
                                .disabled(disabled || filtering)
                                .on_click(cx.listener(
                                    move |panel, _, window, cx| {
                                        if window.has_active_dialog(cx)
                                            || panel.owner.upgrade().is_none_or(|owner| {
                                                let owner = owner.read(cx);
                                                owner.modal
                                                    || owner.editor.is_busy()
                                                    || owner.editor.active_id() != Some(id)
                                            })
                                        {
                                            return;
                                        }
                                        if !panel.resources.view.collapsed.remove(&key) {
                                            panel.resources.view.collapsed.insert(key.clone());
                                        }
                                        panel.resources.view.cursor = None;
                                        cx.notify();
                                    },
                                )),
                            ),
                    );
                    row_index += 1;
                    previous = Some(group);
                }
                if !expanded {
                    continue;
                }
                let selected = animation.motion.as_ref() == Some(&resource.selection);
                if selected {
                    selected_cursor = Some(choices.len());
                }
                let cursor = choices.len();
                choices.push((row_index, Choice::Motion(resource.selection.clone())));
                row_index += 1;
                let selection = resource.selection.clone();
                let play_selection = selection.clone();
                let playing = selected && animation.playing;
                let hint = format!("{} · {}", resource.name, motion_info(resource).join(" · "));
                let label = if playing {
                    t!("workspace.pause")
                } else {
                    t!("workspace.play_from_start")
                };
                let mut row = self
                    .resource_row(
                        ("animation-motion", index),
                        selected,
                        cursor,
                        disabled,
                        window,
                        cx,
                    )
                    .test_support()
                    .tooltip(move |window, cx| Tooltip::new(hint.clone()).build(window, cx))
                    .child(
                        Button::new(("animation-motion-play", index))
                            .ghost()
                            .xsmall()
                            .icon(if playing {
                                IconName::Pause
                            } else {
                                IconName::Play
                            })
                            .accessibility_label(label.clone())
                            .tooltip(label)
                            .disabled(disabled || !resource.playable())
                            .on_click(cx.listener(move |panel, _, window, cx| {
                                cx.stop_propagation();
                                panel.resources.view.cursor = Some(cursor);
                                panel.choose(
                                    id,
                                    Choice::Motion(play_selection.clone()),
                                    true,
                                    window,
                                    cx,
                                );
                            })),
                    )
                    .when_some(resource.selection.entry.as_ref(), |row, (_, entry)| {
                        row.child(
                            div()
                                .text_color(theme.muted_foreground)
                                .child(format!("{}", entry + 1)),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(resource.name.clone()),
                    );
                if resource.looping {
                    row = row.child(
                        Icon::new(IconName::Repeat)
                            .xsmall()
                            .text_color(theme.muted_foreground),
                    );
                }
                row = row.child(
                    div()
                        .flex_shrink_0()
                        .font_features(FontFeatures(std::sync::Arc::new(vec![("tnum".into(), 1)])))
                        .text_color(theme.muted_foreground)
                        .child(if resource.playable() {
                            format!("{:.2} s", resource.duration)
                        } else {
                            "—".into()
                        }),
                );
                if !resource.playable() || resource.sound.is_some() {
                    let warning = if !resource.playable() {
                        t!("workspace.invalid_duration")
                    } else {
                        t!("workspace.sound_unsupported")
                    };
                    row = row.child(
                        div()
                            .id(("animation-motion-warning", index))
                            .tooltip(move |window, cx| {
                                Tooltip::new(warning.clone().into_owned()).build(window, cx)
                            })
                            .child(
                                Icon::new(IconName::TriangleAlert)
                                    .xsmall()
                                    .text_color(theme.yellow),
                            ),
                    );
                }
                list = list.child(row.on_click(cx.listener(move |panel, _, window, cx| {
                    panel.resources.view.cursor = Some(cursor);
                    panel.choose(id, Choice::Motion(selection.clone()), false, window, cx);
                })));
            }
        } else {
            for (index, resource) in catalog.expressions.iter().enumerate() {
                if filtering && !resource.name.to_lowercase().contains(&query) {
                    continue;
                }
                matches += 1;
                let selected = animation.expression.as_ref() == Some(&resource.id);
                let valid = resource.unresolved_targets.is_empty();
                let is_active = active.contains(&resource.id);
                let cursor = choices.len();
                if selected {
                    selected_cursor = Some(cursor);
                }
                if valid {
                    choices.push((row_index, Choice::Expression(resource.id.clone())));
                }
                row_index += 1;
                let expression = resource.id.clone();
                let hint = if valid {
                    format!("{} · {}", resource.name, expression_info(resource))
                } else {
                    format!(
                        "{}: {}",
                        t!("workspace.unresolved_targets"),
                        resource.unresolved_targets.join(", ")
                    )
                };
                let status = if !valid {
                    t!("workspace.unavailable")
                } else if is_active {
                    t!("workspace.expression_active")
                } else if selected {
                    t!("workspace.expression_selected")
                } else {
                    "".into()
                };
                list = list.child(
                    self.resource_row(
                        ("animation-expression", index),
                        selected,
                        if valid { cursor } else { usize::MAX },
                        disabled || !valid,
                        window,
                        cx,
                    )
                    .test_support()
                    .tooltip(move |window, cx| Tooltip::new(hint.clone()).build(window, cx))
                    .child(
                        Icon::new(if !valid {
                            IconName::TriangleAlert
                        } else if selected {
                            IconName::CircleCheck
                        } else {
                            IconName::Circle
                        })
                        .xsmall()
                        .text_color(if !valid {
                            theme.yellow
                        } else {
                            theme.muted_foreground
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(resource.name.clone()),
                    )
                    .child(
                        div()
                            .id(("animation-expression-status", index))
                            .test_support()
                            .aria_label(status.clone())
                            .flex_shrink_0()
                            .text_color(theme.muted_foreground)
                            .child(status),
                    )
                    .on_click(cx.listener(move |panel, _, window, cx| {
                        if valid {
                            panel.resources.view.cursor = Some(cursor);
                            panel.choose(
                                id,
                                Choice::Expression(expression.clone()),
                                false,
                                window,
                                cx,
                            );
                        }
                    })),
                );
            }
        }
        if matches == 0 {
            let label = if filtering {
                t!("workspace.no_matches")
            } else if motion_panel {
                t!("workspace.no_motions")
            } else {
                t!("workspace.no_expressions")
            };
            list = list.child(div().p_2().text_color(theme.muted_foreground).child(label));
        }
        let feedback = if !motion_panel && animation.expression.is_some() {
            Some(if animation.error.is_some() {
                t!("workspace.failed")
            } else if !animation.playing {
                if animation
                    .expression
                    .as_ref()
                    .is_some_and(|id| active.contains(id))
                {
                    t!("workspace.expression_active_paused")
                } else {
                    t!("workspace.expression_paused_hint")
                }
            } else if animation
                .expression
                .as_ref()
                .is_some_and(|id| active.contains(id))
            {
                t!("workspace.expression_active_hint")
            } else {
                t!("workspace.evaluating")
            })
        } else {
            None
        };
        div()
            .id(self.kind.name())
            .test_support()
            .track_focus(&self.focus)
            .key_context("KasaneDocument")
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .text_xs()
            .child(toolbar)
            .child(list)
            .when_some(feedback, |panel, feedback| {
                panel.child(
                    div()
                        .id("animation-expression-feedback")
                        .test_support()
                        .aria_label(feedback.clone())
                        .flex_shrink_0()
                        .px_2()
                        .py_1()
                        .border_t_1()
                        .border_color(theme.border)
                        .text_color(theme.muted_foreground)
                        .child(feedback),
                )
            })
            .on_key_down(cx.listener(move |panel, event: &KeyDownEvent, window, cx| {
                if disabled || !panel.focus.is_focused(window) || choices.is_empty() {
                    return;
                }
                let current = panel
                    .resources
                    .view
                    .cursor
                    .or(selected_cursor)
                    .filter(|i| *i < choices.len());
                let next = match event.keystroke.key.as_str() {
                    "down" => Some(current.map_or(0, |i| (i + 1).min(choices.len() - 1))),
                    "up" => Some(current.map_or(choices.len() - 1, |i| i.saturating_sub(1))),
                    "enter" => {
                        if let Some(i) = current {
                            panel.choose(id, choices[i].1.clone(), false, window, cx);
                        }
                        cx.stop_propagation();
                        return;
                    }
                    _ => None,
                };
                if let Some(next) = next {
                    panel.resources.view.cursor = Some(next);
                    panel.resources.view.scroll.scroll_to_item(choices[next].0);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .into_any_element()
    }

    fn resource_row(
        &self,
        id: (&'static str, usize),
        selected: bool,
        cursor: usize,
        disabled: bool,
        window: &Window,
        cx: &App,
    ) -> Stateful<Div> {
        let theme = cx.theme();
        div()
            .id(id)
            .w_full()
            .min_w_0()
            .h(px(30.))
            .flex()
            .items_center()
            .gap_2()
            .px_2()
            .rounded_md()
            .border_1()
            .border_color(
                if self.focus.is_focused(window) && self.resources.view.cursor == Some(cursor) {
                    theme.primary.opacity(0.6)
                } else {
                    gpui::transparent_black()
                },
            )
            .bg(if selected {
                theme.primary.opacity(0.15)
            } else {
                gpui::transparent_black()
            })
            .when(!disabled, |row| {
                row.cursor_pointer().hover(|style| {
                    style.bg(if selected {
                        theme.primary.opacity(0.22)
                    } else {
                        theme.muted
                    })
                })
            })
            .when(disabled, |row| row.text_color(theme.muted_foreground))
    }
}

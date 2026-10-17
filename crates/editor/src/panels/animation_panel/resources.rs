//! Resource browsing shares the toolbar and compact rows used by Modeling panels.
use super::*;
use crate::panels::group_header;
use crate::panels::toolbar::PanelToolbar;
use editor_core::{CommandId, DocumentId, MotionSelection};
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
struct ViewState {
    collapsed: HashSet<Option<String>>,
    cursor: Option<usize>,
    scroll: ScrollHandle,
}

#[derive(Default)]
pub(super) struct ResourceList {
    document: Option<DocumentId>,
    view: ViewState,
    documents: HashMap<DocumentId, ViewState>,
}

#[derive(Clone)]
enum Choice {
    Motion(MotionSelection),
    Expression(String),
}

impl AnimationPanel {
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
            if !play
                && let Choice::Motion(selection) = &choice
                && owner
                    .editor
                    .animation()
                    .is_some_and(|animation| animation.motion.as_ref() == Some(selection))
            {
                let _ = owner.editor.inspect_animation_resource(
                    document,
                    editor_core::ObjectRef::new(
                        editor_core::ObjectKind::Motion,
                        &selection.clip_id,
                    ),
                );
                cx.notify();
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

    pub(super) fn render_header_toolbar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return Empty.into_any_element();
        };
        let dialog = window.has_active_dialog(cx);
        let state = owner.read(cx);
        let Some(id) = state.editor.active_id() else {
            return Empty.into_any_element();
        };
        let disabled = state.modal || state.editor.is_busy() || dialog;
        let motion_panel = matches!(self.kind, PanelKind::AnimationMotions);
        let prefix = if motion_panel {
            "animation-motion"
        } else {
            "animation-expression"
        };
        let mut toolbar = PanelToolbar::new(SharedString::from(format!("{prefix}-toolbar")));
        if motion_panel {
            let owner = self.owner.clone();
            toolbar = toolbar.action(move |_| {
                Button::new("animation-new-motion")
                    .icon(IconName::Plus)
                    .ghost()
                    .accessibility_label(t!("timeline.new_motion").into_owned())
                    .tooltip(t!("timeline.new_motion"))
                    .disabled(disabled)
                    .on_click(move |_, window, cx| {
                        if window.has_active_dialog(cx) {
                            return;
                        }
                        let _ = owner.update(cx, |owner, cx| {
                            if owner.modal
                                || owner.editor.is_busy()
                                || owner.editor.active_id() != Some(id)
                            {
                                return;
                            }
                            let result = owner.editor.create_timeline_motion(id).map(|_| ());
                            owner.animation_anchor = None;
                            owner.complete_edit(result, cx);
                        });
                    })
            });
        } else {
            let owner = self.owner.clone();
            let unavailable = disabled
                || !state
                    .editor
                    .command_available(state.editor.command(CommandId::ClearExpression));
            toolbar = toolbar.action(move |_| {
                Button::new("animation-clear-expression")
                    .icon(IconName::Eraser)
                    .ghost()
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
        toolbar.into_any_element()
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
        // The tab remains open while its session is owned by the save worker.
        let Some(document) = state.editor.document() else {
            return div().into_any_element();
        };
        let catalog = document.animation_catalog();
        let animation = state.editor.animation().unwrap();
        let preview_mode = animation.preview_mode;
        let disabled = state.modal || state.editor.is_busy() || dialog;
        let motion_panel = matches!(self.kind, PanelKind::AnimationMotions);
        let prefix = if motion_panel {
            "animation-motion"
        } else {
            "animation-expression"
        };
        let theme = cx.theme().clone();
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
                let expanded = !self.resources.view.collapsed.contains(&group);
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
                                .disabled(disabled)
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
                let playable = resource.playable();
                let mut hint = format!("{} · {}", resource.name, motion_info(resource).join(" · "));
                if playable {
                    hint.push_str(&format!(" · {}", t!("workspace.double_click_to_play")));
                }
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
                list = list.child(row.on_click(cx.listener(
                    move |panel, event: &gpui::ClickEvent, window, cx| {
                        panel.resources.view.cursor = Some(cursor);
                        panel.choose(
                            id,
                            Choice::Motion(selection.clone()),
                            playable && event.click_count() == 2,
                            window,
                            cx,
                        );
                    },
                )));
            }
        } else {
            for (index, resource) in catalog.expressions.iter().enumerate() {
                let selected = animation.expression.as_ref() == Some(&resource.id);
                let valid = resource.unresolved_targets.is_empty();
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
                    .when(!valid, |row| {
                        row.child(
                            Icon::new(IconName::TriangleAlert)
                                .xsmall()
                                .text_color(theme.yellow),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(resource.name.clone()),
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
        if (motion_panel && catalog.motions.is_empty())
            || (!motion_panel && catalog.expressions.is_empty())
        {
            let label = if motion_panel {
                t!("workspace.no_motions")
            } else {
                t!("workspace.no_expressions")
            };
            list = list.child(div().p_2().text_color(theme.muted_foreground).child(label));
        }
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
            .when(
                !motion_panel && preview_mode == editor_core::AnimationPreviewMode::Editing,
                |panel| {
                    panel.child(
                        div()
                            .px_2()
                            .py_1()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(t!("timeline.expression_hint")),
                    )
                },
            )
            .child(list)
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
            .h(crate::ui::compact::rem(crate::ui::compact::ROW))
            .flex()
            .items_center()
            .gap(crate::ui::compact::rem(crate::ui::compact::GAP))
            .px(crate::ui::compact::rem(crate::ui::compact::PADDING))
            .rounded(crate::ui::compact::rem(crate::ui::compact::RADIUS))
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

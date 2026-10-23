//! Resource browsing shares the toolbar and compact rows used by Modeling panels.
use super::*;
use crate::authoring::{
    AuthoringContext,
    animation::{self as authoring, AnimationAction},
};
use crate::panels::toolbar::PanelToolbar;
use crate::panels::{
    group_header,
    sorting::{self, SortItem},
};
use editor_core::{CommandId, DocumentId, MotionSelection};
use editor_core::{ObjectKind, ObjectRef};
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(super) struct ViewState {
    collapsed: HashSet<Option<String>>,
    cursor: Option<usize>,
    focused: Option<ObjectRef>,
    pub(super) scroll: ScrollHandle,
    objects: Vec<ObjectRef>,
}

#[derive(Default)]
pub(super) struct ResourceList {
    document: Option<DocumentId>,
    pub(super) view: ViewState,
    documents: HashMap<DocumentId, ViewState>,
}

#[derive(Clone)]
enum Choice {
    Inspect(ObjectRef),
    Motion(MotionSelection),
    Expression(String),
}

impl Choice {
    fn object(&self) -> ObjectRef {
        match self {
            Self::Inspect(object) => object.clone(),
            Self::Motion(motion) => ObjectRef::new(
                ObjectKind::MotionRegistration,
                motion.entry.as_ref().unwrap(),
            ),
            Self::Expression(id) => ObjectRef::new(ObjectKind::Expression, id),
        }
    }
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
        let focus = self.focus.clone();
        let focused = choice.object();
        let _ = self.owner.update(cx, |owner, cx| {
            if owner.modal || owner.editor.is_busy() || owner.editor.active_id() != Some(document) {
                return;
            }
            if !owner.prepare_property_inputs(document, window, cx) {
                return;
            }
            window.focus(&focus, cx);
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
                Choice::Inspect(object) => {
                    owner.inspect_authoring_object(document, object, false, window, cx)
                }
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
        self.resources.view.focused = Some(focused);
        cx.notify();
    }

    pub(super) fn render_header_toolbar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.kind == PanelKind::AnimationPhysics {
            return self.physics_list_toolbar(window, cx);
        }
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
        let context = AuthoringContext::capture(&state.editor);
        let selection = authoring::selection(&state.editor, motion_panel);
        for kind in if motion_panel {
            vec![ObjectKind::Motion, ObjectKind::MotionGroup]
        } else {
            vec![ObjectKind::Expression]
        } {
            let Some(availability) =
                AnimationAction::Create(kind, selection.clone()).availability(&state.editor)
            else {
                continue;
            };
            let unavailable = disabled || !availability.enabled;
            let hint = availability
                .reason
                .unwrap_or_else(|| authoring::create_label(kind));
            let owner = self.owner.clone();
            let selection = selection.clone();
            toolbar = toolbar.action(move |_| {
                Button::new(match kind {
                    ObjectKind::Motion => "animation-new-motion",
                    ObjectKind::MotionGroup => "animation-new-motion-group",
                    _ => "animation-new-expression",
                })
                .icon(if kind == ObjectKind::MotionGroup {
                    IconName::FolderPlus
                } else {
                    IconName::Plus
                })
                .ghost()
                .accessibility_label(authoring::create_label(kind))
                .tooltip(hint.clone())
                .disabled(unavailable)
                .on_click(move |_, window, cx| {
                    if let Some(context) = context {
                        let _ = owner.update(cx, |owner, cx| {
                            owner.run_animation_action(
                                context,
                                AnimationAction::Create(kind, selection.clone()),
                                window,
                                cx,
                            )
                        });
                    }
                })
            });
        }
        if motion_panel {
            let owner = self.owner.clone();
            let at = selection.clone();
            let availability =
                AnimationAction::AddMotionReference(at.clone()).availability(&state.editor);
            let unavailable = disabled || availability.as_ref().is_none_or(|a| !a.enabled);
            let hint = availability
                .and_then(|a| a.reason)
                .unwrap_or_else(|| t!("authoring.add_motion_reference").into());
            toolbar = toolbar.action(move |_| {
                Button::new("animation-add-motion-reference")
                    .icon(IconName::Link)
                    .ghost()
                    .accessibility_label(t!("authoring.add_motion_reference"))
                    .tooltip(hint.clone())
                    .disabled(unavailable)
                    .on_click(move |_, window, cx| {
                        if let Some(context) = context {
                            let _ = owner.update(cx, |v, cx| {
                                v.run_animation_action(
                                    context,
                                    AnimationAction::AddMotionReference(at.clone()),
                                    window,
                                    cx,
                                )
                            });
                        }
                    })
            });
        }
        if !motion_panel {
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
        let enabled = {
            let state = owner.read(cx);
            !state.modal && !state.editor.is_busy() && !dialog
        };
        self.sorting.update(cx, |sorting, cx| {
            sorting.reconcile(document, enabled, window, cx)
        });
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
        let location = self
            .location
            .take()
            .filter(|(doc, _)| *doc == id)
            .map(|(_, mut object)| {
                let index = document.object_index();
                let mut seen = HashSet::new();
                while !matches!(
                    object.kind,
                    editor_core::ObjectKind::Motion
                        | editor_core::ObjectKind::MotionGroup
                        | editor_core::ObjectKind::MotionRegistration
                        | editor_core::ObjectKind::Expression
                ) && seen.insert(object.clone())
                {
                    let Some(parent) = index
                        .incoming(&object)
                        .find(|r| r.kind == editor_core::RelationKind::Contains)
                        .map(|r| r.source.clone())
                    else {
                        break;
                    };
                    object = parent;
                }
                object
            });
        let catalog = document.animation_catalog();
        let animation = state.editor.animation().unwrap();
        let location = location.map(|object| {
            if object.kind == ObjectKind::Motion {
                let selected = animation.motion.as_ref().filter(|m| m.clip_id == object.id);
                let resource = selected
                    .and_then(|selection| {
                        catalog.motions.iter().find(|r| &r.selection == selection)
                    })
                    .or_else(|| {
                        catalog
                            .motions
                            .iter()
                            .find(|r| r.selection.clip_id == object.id)
                    });
                if let Some(id) = resource.and_then(|r| r.selection.entry.as_ref()) {
                    return ObjectRef::new(ObjectKind::MotionRegistration, id);
                }
            }
            object
        });
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
        list = sorting::scroll_div(list, &self.sorting, &self.resources.view.scroll);
        let mut choices = Vec::new();
        let mut selected_cursor = None;
        let mut row_index = 0;
        if motion_panel {
            // Resolve and expand before emitting any rows: locating a later
            // registration must not omit its preceding siblings for one frame.
            // Clip-only locations choose one registration, preserving its cursor.
            let located_motion = location.as_ref().and_then(|object| {
                catalog.motions.iter().position(|resource| {
                    object.id == resource.selection.clip_id
                        || resource.selection.entry.as_ref() == Some(&object.id)
                })
            });
            if let Some(index) = located_motion {
                self.resources
                    .view
                    .collapsed
                    .remove(&catalog.motions[index].group);
            }
            let groups: Vec<_> = document
                .motion_groups()
                .iter()
                .map(|g| (Some(g.id.clone()), g.name.clone()))
                .chain(std::iter::once((
                    None,
                    t!("workspace.ungrouped").into_owned(),
                )))
                .collect();
            let mut grouped = HashMap::<Option<String>, Vec<_>>::new();
            for (index, resource) in catalog.motions.iter().enumerate() {
                grouped
                    .entry(resource.group_id.clone())
                    .or_default()
                    .push((index, resource));
            }
            for (group_index, (group, group_name)) in groups.into_iter().enumerate() {
                let members = grouped.remove(&group).unwrap_or_default();
                if group.is_none() && members.is_empty() && document.motion_groups().is_empty() {
                    continue;
                }
                let located = location.as_ref().is_some_and(|object| {
                    group
                        .as_ref()
                        .is_some_and(|g| object.kind == ObjectKind::MotionGroup && object.id == *g)
                        || members.iter().any(|(_, r)| {
                            object.id == r.selection.clip_id
                                || r.selection.entry.as_ref() == Some(&object.id)
                        })
                });
                if located {
                    self.resources.view.collapsed.remove(&group);
                }
                let expanded = !self.resources.view.collapsed.contains(&group);
                let key = group.clone();
                let object = group
                    .as_ref()
                    .map(|id| ObjectRef::new(ObjectKind::MotionGroup, id));
                let cursor = choices.len();
                if let Some(object) = &object {
                    if state
                        .editor
                        .active_inspection()
                        .and_then(|i| i.object())
                        .as_ref()
                        == Some(object)
                    {
                        selected_cursor = Some(cursor);
                    }
                    if location.as_ref() == Some(object) {
                        self.resources.view.focused = Some(object.clone());
                        self.resources.view.scroll.scroll_to_top_of_item(row_index);
                    }
                    choices.push((row_index, Choice::Inspect(object.clone())));
                }
                let item = object
                    .clone()
                    .map(SortItem::Resource)
                    .unwrap_or(SortItem::ResourceRoot { motion: true });
                let panel = cx.weak_entity();
                let expand: sorting::ExpandTarget = std::rc::Rc::new(move |item, _, cx| {
                    if let SortItem::Resource(object) = item {
                        let _ = panel.update(cx, |p, cx| {
                            if p.resources.view.collapsed.remove(&Some(object.id.clone())) {
                                cx.notify();
                            }
                        });
                    }
                });
                let mut header = div()
                    .id(("animation-motion-group-header", group_index))
                    .test_support()
                    .role(gpui::Role::Group)
                    .aria_label(group_name.clone())
                    .aria_expanded(expanded)
                    .aria_selected(object.as_ref().is_some_and(|object| {
                        state
                            .editor
                            .active_inspection()
                            .and_then(|i| i.object())
                            .as_ref()
                            == Some(object)
                    }))
                    .map(|e| group_header::row_style(e, 0, cx))
                    .map(|e| sorting::drop_target(e, &self.sorting, item.clone(), Some(expand), cx))
                    .map(|e| {
                        sorting::drag_source(
                            e,
                            &self.sorting,
                            id,
                            item,
                            group_name.clone(),
                            !disabled && object.is_some(),
                        )
                    })
                    .child(
                        Button::new(("animation-motion-group", group_index))
                            .icon(if expanded {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            })
                            .ghost()
                            .xsmall()
                            .disabled(disabled)
                            .accessibility_label(group_name.clone())
                            .on_click(cx.listener(move |panel, _, _, cx| {
                                if !panel.resources.view.collapsed.remove(&key) {
                                    panel.resources.view.collapsed.insert(key.clone());
                                }
                                cx.notify();
                            })),
                    );
                let label = group_name.clone();
                let object_for_click = object.clone();
                header = header
                    .child(
                        Button::new(("animation-motion-group-name", group_index))
                            .ghost()
                            .xsmall()
                            .flex_1()
                            .min_w_0()
                            .justify_start()
                            .disabled(disabled || object.is_none())
                            .label(label)
                            .on_click(cx.listener(move |panel, _, window, cx| {
                                if let Some(object) = &object_for_click {
                                    panel.resources.view.cursor = Some(cursor);
                                    panel.choose(
                                        id,
                                        Choice::Inspect(object.clone()),
                                        false,
                                        window,
                                        cx,
                                    );
                                }
                            })),
                    )
                    .child(
                        div()
                            .id(("animation-motion-group-count", group_index))
                            .test_support()
                            .aria_value(members.len().to_string())
                            .child(members.len().to_string()),
                    );
                if let Some(object) = object {
                    list = list.child(authoring::row_menu(header, self.owner.clone(), id, object));
                } else {
                    list = list.child(header);
                }
                row_index += 1;
                if !expanded {
                    continue;
                }
                for (index, resource) in members {
                    let located = location.as_ref().is_some_and(|object| {
                        object.id == resource.selection.clip_id
                            || resource.selection.entry.as_ref() == Some(&object.id)
                    });
                    let selected = animation.motion.as_ref() == Some(&resource.selection);
                    if selected {
                        selected_cursor = Some(choices.len());
                    }
                    let cursor = choices.len();
                    if located {
                        self.resources.view.cursor = Some(cursor);
                        self.resources.view.focused =
                            Some(Choice::Motion(resource.selection.clone()).object());
                        self.resources.view.scroll.scroll_to_top_of_item(row_index);
                    }
                    choices.push((row_index, Choice::Motion(resource.selection.clone())));
                    row_index += 1;
                    let selection = resource.selection.clone();
                    let playable = resource.playable();
                    let mut hint =
                        format!("{} · {}", resource.name, motion_info(resource).join(" · "));
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
                        .when_some(resource.selection.entry.as_ref(), |row, _| {
                            let entry = resource.entry_index;
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
                            .font_features(FontFeatures(std::sync::Arc::new(vec![(
                                "tnum".into(),
                                1,
                            )])))
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
                    let object = ObjectRef::new(
                        ObjectKind::MotionRegistration,
                        resource.selection.entry.as_ref().unwrap(),
                    );
                    let row = sorting::drop_target(
                        row,
                        &self.sorting,
                        SortItem::Resource(object.clone()),
                        None,
                        cx,
                    );
                    let row = sorting::drag_source(
                        row,
                        &self.sorting,
                        id,
                        SortItem::Resource(object.clone()),
                        resource.name.clone(),
                        !disabled,
                    );
                    list = list.child(authoring::row_menu(
                        row.on_click(cx.listener(
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
                        )),
                        self.owner.clone(),
                        id,
                        object,
                    ));
                }
            }
        } else {
            for (index, resource) in catalog.expressions.iter().enumerate() {
                let selected = animation.expression.as_ref() == Some(&resource.id);
                let active = state
                    .displayed_animation()
                    .is_some_and(|frame| frame.active_expressions.contains(&resource.id));
                let valid = resource.unresolved_targets.is_empty();
                let cursor = choices.len();
                if location
                    .as_ref()
                    .is_some_and(|object| object.id == resource.id)
                {
                    self.resources.view.cursor = Some(cursor);
                    self.resources.view.focused =
                        Some(ObjectRef::new(ObjectKind::Expression, &resource.id));
                    self.resources.view.scroll.scroll_to_top_of_item(row_index);
                }

                if selected {
                    selected_cursor = Some(cursor);
                }
                choices.push((row_index, Choice::Expression(resource.id.clone())));
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
                let row = self
                    .resource_row(
                        ("animation-expression", index),
                        selected,
                        cursor,
                        disabled,
                        window,
                        cx,
                    )
                    .test_support()
                    .tooltip(move |window, cx| Tooltip::new(hint.clone()).build(window, cx))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(resource.name.clone()),
                    )
                    .when(!valid, |row| {
                        row.child(
                            div()
                                .id(("animation-expression-warning", index))
                                .test_support()
                                .flex_shrink_0()
                                .child(
                                    Icon::new(IconName::TriangleAlert)
                                        .xsmall()
                                        .text_color(theme.yellow),
                                ),
                        )
                    })
                    .when(active, |row| {
                        row.child(
                            div()
                                .id(("animation-expression-active", index))
                                .test_support()
                                .flex_shrink_0()
                                .aria_label(t!("workspace.expression_active").into_owned())
                                .tooltip(|window, cx| {
                                    Tooltip::new(t!("workspace.expression_active").into_owned())
                                        .build(window, cx)
                                })
                                .child(Icon::new(IconName::Eye).xsmall().text_color(theme.primary)),
                        )
                    })
                    .on_click(cx.listener(move |panel, _, window, cx| {
                        panel.resources.view.cursor = Some(cursor);
                        panel.choose(
                            id,
                            Choice::Expression(expression.clone()),
                            false,
                            window,
                            cx,
                        );
                    }));
                let object = ObjectRef::new(ObjectKind::Expression, &resource.id);
                let row = sorting::drop_target(
                    row,
                    &self.sorting,
                    SortItem::Resource(object.clone()),
                    None,
                    cx,
                );
                let row = sorting::drag_source(
                    row,
                    &self.sorting,
                    id,
                    SortItem::Resource(object.clone()),
                    resource.name.clone(),
                    !disabled,
                );
                list = list.child(authoring::row_menu(row, self.owner.clone(), id, object));
            }
        }
        self.resources.view.objects = choices.iter().map(|(_, choice)| choice.object()).collect();
        if let Some(focused) = &self.resources.view.focused {
            self.resources.view.cursor = self
                .resources
                .view
                .objects
                .iter()
                .position(|object| object == focused);
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
            .map(|e| sorting::panel_actions(e, &self.sorting))
            .test_support()
            .track_focus(&self.focus)
            .key_context("KasaneDocument KasanePalette")
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
                    "left" | "right" => {
                        if let Some(i) = current {
                            if let Choice::Inspect(object) = &choices[i].1 {
                                let key = Some(object.id.clone());
                                if event.keystroke.key == "left" {
                                    panel.resources.view.collapsed.insert(key);
                                } else {
                                    panel.resources.view.collapsed.remove(&key);
                                }
                                cx.notify();
                                cx.stop_propagation();
                            }
                        }
                        return;
                    }
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
                    panel.resources.view.focused = Some(choices[next].1.object());
                    panel.resources.view.scroll.scroll_to_item(choices[next].0);
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .into_any_element()
    }

    pub(crate) fn movement_object(&self) -> Option<ObjectRef> {
        self.resources
            .view
            .cursor
            .and_then(|i| self.resources.view.objects.get(i))
            .cloned()
    }

    pub(super) fn resource_row(
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

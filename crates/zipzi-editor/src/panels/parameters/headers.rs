//! Parameter group headers and sorting targets.
use super::*;

impl ParametersPanel {
    pub(super) fn group_header(
        &self,
        id: Option<String>,
        name: String,
        depth: usize,
        count: usize,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = id.as_deref().unwrap_or("ungrouped").to_owned();
        let expanded = !self.collapsed_groups.contains(&id);
        let group = id.clone();
        let document_id = self.document_id;
        let cursor = id.as_ref().map_or(ParameterCursor::Ungrouped, |id| {
            ParameterCursor::Object(zipzi_editor_core::ObjectRef::new(
                zipzi_editor_core::ObjectKind::ParameterGroup,
                id,
            ))
        });
        let object = match &cursor {
            ParameterCursor::Object(r) => Some(r),
            _ => None,
        };
        let selected = object.is_some() && self.navigation.inspected.as_ref() == object;
        let focused = self.navigation_focused && self.navigation.cursor.as_ref() == Some(&cursor);
        let inspect_cursor = cursor.clone();
        let panel = cx.weak_entity();
        let expand: sorting::ExpandTarget = std::rc::Rc::new(move |item, _, cx| {
            if let SortItem::Group(group) = item {
                _ = panel.update(cx, |panel, cx| {
                    if panel.collapsed_groups.remove(group) {
                        panel.refresh_visible_rows(cx);
                        cx.notify();
                    }
                });
            }
        });
        let selection: Vec<_> = id
            .as_ref()
            .map(|id| {
                zipzi_editor_core::ObjectRef::new(zipzi_editor_core::ObjectKind::ParameterGroup, id)
            })
            .into_iter()
            .collect();
        let mut menu_actions = vec![
            ParameterAction::CreateAt {
                kind: zipzi_editor_core::ObjectKind::Parameter,
                selection: selection.clone(),
            },
            ParameterAction::CreateAt {
                kind: zipzi_editor_core::ObjectKind::ParameterGroup,
                selection,
            },
        ];
        if let Some(id) = &id {
            menu_actions.extend([
                ParameterAction::EditGroup(id.clone()),
                ParameterAction::DeleteGroup(id.clone()),
            ]);
        }
        let menu_panel = cx.weak_entity();
        let sort_item = SortItem::Group(id.clone());
        div()
            .id(SharedString::from(format!("parameter-group-{key}")))
            .map(|element| {
                sorting::drop_target(element, &self.sorting, sort_item.clone(), Some(expand), cx)
            })
            .when_some(document_id, |element, document| {
                sorting::drag_source(
                    element,
                    &self.sorting,
                    document,
                    sort_item,
                    name.clone(),
                    !disabled && id.is_some(),
                )
            })
            .test_support()
            .role(gpui::Role::Group)
            .aria_label(name.clone())
            .aria_expanded(expanded)
            .aria_selected(selected)
            .map(|element| group_header::row_style(element, depth, cx))
            .when(selected, |row| row.bg(cx.theme().primary.opacity(0.15)))
            .border_1()
            .border_color(if focused {
                cx.theme().primary
            } else {
                cx.theme().border
            })
            .child(
                Button::new(SharedString::from(format!("parameter-group-toggle-{key}")))
                    .icon(if expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .ghost()
                    .small()
                    .ml(crate::ui::compact::rem(depth.min(6) as f32 * 12.))
                    .disabled(disabled)
                    .accessibility_label(t!(
                        if expanded {
                            "parameter.collapse_group"
                        } else {
                            "parameter.expand_group"
                        },
                        name = name.clone()
                    ))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.toggle_group(group.clone(), window, cx);
                    })),
            )
            .child(
                Button::new(SharedString::from(format!("parameter-group-name-{key}")))
                    .icon(IconName::Folder)
                    .ghost()
                    .small()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .justify_start()
                    .disabled(disabled)
                    .accessibility_label(name.clone())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_ellipsis()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(name),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("parameter-group-count-{key}")))
                            .test_support()
                            .aria_value(count.to_string())
                            .text_xs()
                            .child(count.to_string()),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(document) = document_id {
                            this.activate_row(document, cursor.clone(), window, cx);
                        }
                    })),
            )
            .when_some(id, |header, id| {
                header.child(
                    Button::new(SharedString::from(format!("parameter-group-inspect-{id}")))
                        .icon(IconName::Info)
                        .ghost()
                        .small()
                        .disabled(disabled)
                        .accessibility_label(t!("editor.parameter_group_inspect"))
                        .tooltip(t!("editor.parameter_group_inspect"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(document) = document_id {
                                this.activate_row(document, inspect_cursor.clone(), window, cx);
                            }
                        })),
                )
            })
            .map(|element| management::row_menu(element, menu_panel, document_id, menu_actions))
            .into_any_element()
    }
}

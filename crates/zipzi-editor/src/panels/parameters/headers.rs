//! Parameter group headers and sorting targets.
use super::*;
use gpui_kit::component::button::ButtonRounded;

impl ParametersPanel {
    pub(super) fn related_filter_button(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Button {
        crate::ui::compact::header_button(crate::ui::compact::toggle(
            Button::new("parameters-filter-related"),
            self.only_related,
        ))
        .icon(IconName::ListFilter)
        .accessibility_label(t!("parameter.filter_related"))
        .tooltip(t!(if self.only_related {
            "parameter.show_all"
        } else {
            "parameter.filter_related"
        }))
        .disabled(self.document_id.is_none() || !self.slider_interaction_allowed(window, cx))
        .on_click(cx.listener(|this, _, window, cx| {
            if !this.slider_interaction_allowed(window, cx) {
                return;
            }
            let (Some(document), Some(owner)) = (this.document_id, this.owner.upgrade()) else {
                return;
            };
            if !owner.update(cx, |owner, cx| {
                owner.prepare_property_inputs(document, window, cx)
            }) {
                return;
            }
            this.finish_slider_drag(cx);
            this.only_related = !this.only_related;
            this.refresh_visible_rows(cx);
            cx.notify();
        }))
    }

    fn has_collapsed_groups(&self) -> bool {
        self.visible_rows.iter().any(|row| matches!(row,
            ListRow::Group { id: Some(id), .. } if self.collapsed_groups.contains(&Some(id.clone()))
        ))
    }

    pub(super) fn expansion_button(&self, window: &mut Window, cx: &mut Context<Self>) -> Button {
        let expanded = self.has_collapsed_groups();
        let document = self.document_id;
        let disabled = document.is_none()
            || !self.slider_interaction_allowed(window, cx)
            || !self
                .layout
                .rows()
                .iter()
                .any(|row| matches!(row, ListRow::Group { id: Some(_), .. }));
        let label = if expanded {
            t!("editor.tree_unfold_all")
        } else {
            t!("editor.tree_fold_all")
        };
        crate::ui::compact::header_button(Button::new("parameters-toggle-expansion"))
            .icon(if expanded {
                IconName::ChevronRight
            } else {
                IconName::ChevronDown
            })
            .accessibility_label(label.clone())
            .tooltip(label)
            .disabled(disabled)
            .on_click(cx.listener(move |this, _, window, cx| {
                if this.document_id != document || !this.slider_interaction_allowed(window, cx) {
                    return;
                }
                this.finish_slider_drag(cx);
                let expand = this.has_collapsed_groups();
                this.collapsed_groups.clear();
                if !expand {
                    this.collapsed_groups
                        .extend(this.layout.rows().iter().filter_map(|row| match row {
                            ListRow::Group { id: Some(id), .. } => Some(Some(id.clone())),
                            _ => None,
                        }));
                }
                window.focus(&this.focus, cx);
                this.navigation_focused = true;
                this.refresh_visible_rows(cx);
                cx.notify();
            }))
    }

    pub(super) fn group_header(
        &self,
        id: Option<String>,
        name: String,
        depth: usize,
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
            .border_color(if focused {
                cx.theme().primary
            } else {
                cx.theme().border
            })
            .child(
                Button::new(SharedString::from(format!("parameter-group-toggle-{key}")))
                    .rounded(ButtonRounded::None)
                    .icon(if expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .ghost()
                    .xsmall()
                    .h_full()
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
                    .rounded(ButtonRounded::None)
                    .icon(IconName::Folder)
                    .ghost()
                    .xsmall()
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
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(document) = document_id {
                            this.activate_row(document, cursor.clone(), window, cx);
                        }
                    })),
            )
            .map(|element| management::row_menu(element, menu_panel, document_id, menu_actions))
            .into_any_element()
    }
}

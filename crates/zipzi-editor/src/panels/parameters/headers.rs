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
        cx: &Context<Self>,
    ) -> ParameterGroupHeader {
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
        ParameterGroupHeader {
            expanded: !self.collapsed_groups.contains(&id),
            selected: object.is_some() && self.navigation.inspected.as_ref() == object,
            focused: self.navigation_focused && self.navigation.cursor.as_ref() == Some(&cursor),
            id,
            name,
            depth,
            disabled,
            document_id: self.document_id,
            cursor,
            sorting: self.sorting.clone(),
            panel: cx.weak_entity(),
        }
    }
}

#[derive(Clone, gpui_kit::IntoElement)]
pub(super) struct ParameterGroupHeader {
    id: Option<String>,
    name: String,
    depth: usize,
    disabled: bool,
    document_id: Option<DocumentId>,
    cursor: ParameterCursor,
    expanded: bool,
    selected: bool,
    focused: bool,
    sorting: Entity<SortState>,
    panel: WeakEntity<ParametersPanel>,
}
impl RenderOnce for ParameterGroupHeader {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            id,
            name,
            depth,
            disabled,
            document_id,
            cursor,
            expanded,
            selected,
            focused,
            sorting,
            panel,
        } = self;
        let key = id.as_deref().unwrap_or("ungrouped").to_owned();
        let group = id.clone();
        let toggle_panel = panel.clone();
        let select_panel = panel.clone();
        let expand_panel = panel.clone();
        let expand: sorting::ExpandTarget = std::rc::Rc::new(move |item, _, cx| {
            if let SortItem::Group(group) = item {
                _ = expand_panel.update(cx, |panel, cx| {
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
        let menu_panel = panel;
        let sort_item = SortItem::Group(id.clone());
        div()
            .id(SharedString::from(format!("parameter-group-{key}")))
            .map(|element| {
                sorting::drop_target(element, &sorting, sort_item.clone(), Some(expand), cx)
            })
            .when_some(document_id, |element, document| {
                sorting::drag_source(
                    element,
                    &sorting,
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
                    .on_click(move |_, window, cx| {
                        _ = toggle_panel
                            .update(cx, |this, cx| this.toggle_group(group.clone(), window, cx));
                    }),
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
                    .on_click(move |_, window, cx| {
                        if let Some(document) = document_id {
                            _ = select_panel.update(cx, |this, cx| {
                                this.activate_row(document, cursor.clone(), window, cx)
                            });
                        }
                    }),
            )
            .map(|element| management::row_menu(element, menu_panel, document_id, menu_actions))
            .into_any_element()
    }
}

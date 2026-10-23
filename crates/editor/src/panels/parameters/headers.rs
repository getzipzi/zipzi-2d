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
        let owner = self.owner.clone();
        let document_id = self.document_id;
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
            .map(|id| editor_core::ObjectRef::new(editor_core::ObjectKind::ParameterGroup, id))
            .into_iter()
            .collect();
        let mut menu_actions = vec![
            ParameterAction::CreateAt {
                kind: editor_core::ObjectKind::Parameter,
                selection: selection.clone(),
            },
            ParameterAction::CreateAt {
                kind: editor_core::ObjectKind::ParameterGroup,
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
            .map(|element| group_header::row_style(element, depth, cx))
            .child(
                group_header::toggle(
                    SharedString::from(format!("parameter-group-toggle-{key}")),
                    SharedString::from(format!("parameter-group-count-{key}")),
                    name,
                    count,
                    expanded,
                    depth,
                    cx,
                )
                .disabled(disabled)
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !this.slider_interaction_allowed(window, cx) {
                        return;
                    }
                    this.finish_slider_drag(cx);
                    if !this.collapsed_groups.remove(&group) {
                        this.collapsed_groups.insert(group.clone());
                    }
                    this.refresh_visible_rows(cx);
                    cx.notify();
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
                        .on_click(move |_, window, cx| {
                            if window.has_active_dialog(cx) {
                                return;
                            }
                            let (Some(owner), Some(document_id)) = (owner.upgrade(), document_id)
                            else {
                                return;
                            };
                            owner.update(cx, |owner, cx| {
                                if owner.modal
                                    || owner.editor.is_busy()
                                    || owner.editor.active_id() != Some(document_id)
                                {
                                    return;
                                }
                                let result = owner.editor.execute_for(
                                    document_id,
                                    EditorCommand::Navigate(editor_core::ObjectRef::new(
                                        editor_core::ObjectKind::ParameterGroup,
                                        &id,
                                    )),
                                );
                                owner.complete_edit(result, cx);
                            });
                        }),
                )
            })
            .map(|element| management::row_menu(element, menu_panel, document_id, menu_actions))
            .into_any_element()
    }
}

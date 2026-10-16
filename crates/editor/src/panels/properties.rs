use crate::{EditorView, t, ui::compact};
use editor_core::{DocumentId, EditorCommand, ObjectRef};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dock::{BasePanel, Panel, PanelEvent},
    scroll::ScrollableElement,
};
use gpui_kit::{prelude::*, *};
use std::sync::Arc;

#[derive(Clone, PartialEq, Eq)]
enum Row {
    Summary,
    Group { key: String, collapsed: bool },
    Value { field: String, value: String },
}

pub struct PropertiesPanel {
    scroll: ListState,
    scroll_key: Option<(DocumentId, ObjectRef)>,
    rows: Arc<[Row]>,
    #[cfg(test)]
    row_builds: usize,
    collapsed: std::collections::HashSet<String>,
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    _observe: Subscription,
}

impl PropertiesPanel {
    pub fn new(owner: WeakEntity<EditorView>, _: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            scroll: ListState::new(0, ListAlignment::Top, px(24.))
                .with_uniform_item_height(px(24.)),
            scroll_key: None,
            rows: Arc::from([]),
            #[cfg(test)]
            row_builds: 0,
            collapsed: Default::default(),
            focus: cx.focus_handle(),
            _observe: cx.observe(&owner.upgrade().unwrap(), |_, _, cx| cx.notify()),
            owner,
        })
    }
}

impl EventEmitter<PanelEvent> for PropertiesPanel {}
impl Focusable for PropertiesPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for PropertiesPanel {
    fn panel_name(&self) -> &'static str {
        "Properties"
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl Panel for PropertiesPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        t!("editor.properties")
    }
}

impl PropertiesPanel {
    fn summary(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let state = owner.read(cx);
        let Some(doc) = state.editor.document() else {
            return div()
                .child(div().p_2().child(t!("editor.no_document")))
                .into_any_element();
        };
        let Some(document) = state.editor.active_id() else {
            return div().into_any_element();
        };
        let index = doc.object_index();
        let inspected = state
            .editor
            .active_inspection()
            .and_then(|source| source.object())
            .unwrap_or_else(|| index.objects[0].reference.clone());
        let info = index.get(&inspected);
        let doc_ref = index.objects[0].reference.clone();
        let mut body = div()
            .flex()
            .flex_col()
            .gap(crate::ui::compact::rem(compact::GAP))
            .child(
                div()
                    .id("inspect-document")
                    .test_support()
                    .cursor_pointer()
                    .child(t!("editor.document_properties"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if window.has_active_dialog(cx) {
                            return;
                        }
                        let Some(owner) = this.owner.upgrade() else {
                            return;
                        };
                        owner.update(cx, |owner, cx| {
                            if owner.modal
                                || owner.editor.is_busy()
                                || owner.editor.active_id() != Some(document)
                            {
                                return;
                            }
                            let result = owner
                                .editor
                                .execute_for(document, EditorCommand::Navigate(doc_ref.clone()));
                            owner.complete_edit(result, cx);
                        });
                    })),
            )
            .child(format!("{} · {:?}", index.name(&inspected), inspected.kind))
            .when(
                doc.ensure_editable(std::slice::from_ref(&inspected))
                    .is_err(),
                |body| body.child(t!("editor.object_locked")),
            )
            .when_some(
                info.and_then(|info| info.diagnostic.clone()),
                |this, diagnostic| this.child(diagnostic),
            );
        if inspected.kind == editor_core::ObjectKind::ParameterGroup {
            use super::parameters::ParameterAction;
            let actions = [
                ParameterAction::EditGroup(inspected.id.clone()),
                ParameterAction::DeleteGroup(inspected.id.clone()),
            ];
            for (i, action) in actions.into_iter().enumerate() {
                let owner = self.owner.clone();
                body = body.child(
                    compact::button(Button::new(if i == 0 {
                        "property-edit-parameter"
                    } else {
                        "property-delete-parameter"
                    }))
                    .label(action.label())
                    .disabled(
                        state.modal
                            || state.editor.is_busy()
                            || doc
                                .ensure_editable(std::slice::from_ref(&inspected))
                                .is_err(),
                    )
                    .on_click(move |_, window, cx| {
                        let Some(owner) = owner.upgrade() else {
                            return;
                        };
                        if owner.read(cx).editor.active_id() != Some(document) {
                            return;
                        }
                        let panel = owner.read(cx).dock.as_ref().map(|d| d.parameters.clone());
                        if let Some(panel) = panel {
                            panel.update(cx, |p, cx| p.open_management(action.clone(), window, cx));
                        }
                    }),
                );
            }
        }
        body.into_any_element()
    }

    #[cfg(test)]
    pub(crate) fn row_builds(&self) -> usize {
        self.row_builds
    }

    fn render_row(&mut self, row: &Row, cx: &mut Context<Self>) -> AnyElement {
        #[cfg(test)]
        {
            self.row_builds += 1;
        }
        let content = match row {
            Row::Summary => self.summary(cx),
            Row::Group { key, collapsed } => {
                let label = if key.is_empty() {
                    t!("editor.general_properties").into_owned()
                } else {
                    key.clone()
                };
                let key = key.clone();
                Button::new(SharedString::from(format!("property-group-{key}")))
                    .ghost()
                    .xsmall()
                    .h(compact::rem(compact::ROW))
                    .w_full()
                    .justify_start()
                    .accessibility_label(label.clone())
                    .child(
                        div()
                            .flex()
                            .w_full()
                            .items_center()
                            .gap(compact::rem(6.))
                            .child(
                                gpui_kit::component::Icon::new(if *collapsed {
                                    gpui_kit::assets::IconName::ChevronRight
                                } else {
                                    gpui_kit::assets::IconName::ChevronDown
                                })
                                .xsmall(),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(cx.theme().foreground)
                                    .child(label),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.collapsed.remove(&key) {
                            this.collapsed.insert(key.clone());
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            }
            Row::Value { field, value } => {
                let label = field
                    .split_once('.')
                    .map_or(field.as_str(), |(_, label)| label)
                    .to_owned();
                compact::property_row(label, compact::property_value(value.clone()), cx)
                    .id(SharedString::from(format!("object-property-{field}")))
                    .test_support()
                    .into_any_element()
            }
        };
        div()
            .w_full()
            .px(compact::rem(compact::PADDING))
            .pb(compact::rem(compact::GAP))
            .child(content)
            .into_any_element()
    }
}

impl Render for PropertiesPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div()
            .id("properties-panel")
            .size_full()
            .min_h_0()
            .track_focus(&self.focus)
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_xs();
        let Some(owner) = self.owner.upgrade() else {
            return root.into_any_element();
        };
        let state = owner.read(cx);
        let Some(doc) = state.editor.document() else {
            self.scroll_key = None;
            self.rows = Arc::from([]);
            self.scroll.reset(0);
            return root
                .child(div().p_2().child(t!("editor.no_document")))
                .into_any_element();
        };
        let Some(document) = state.editor.active_id() else {
            return root.into_any_element();
        };
        let index = doc.object_index();
        let inspected = state
            .editor
            .active_inspection()
            .and_then(|source| source.object())
            .unwrap_or_else(|| index.objects[0].reference.clone());
        let mut fields = index.get(&inspected).map_or_else(Vec::new, |info| {
            super::document_panel::property_rows(&info.properties)
        });
        fields.sort_by_key(|(field, _)| (field != "name", field.clone()));
        let mut groups = std::collections::BTreeMap::<String, Vec<(String, String)>>::new();
        for (field, value) in fields {
            let group = field.split_once('.').map_or("", |(group, _)| group);
            groups
                .entry(group.to_string())
                .or_default()
                .push((field, value));
        }
        let mut rows = vec![Row::Summary];
        for (key, fields) in groups {
            let collapsed = self.collapsed.contains(&key);
            rows.push(Row::Group { key, collapsed });
            if !collapsed {
                rows.extend(
                    fields
                        .into_iter()
                        .map(|(field, value)| Row::Value { field, value }),
                );
            }
        }
        let key = (document, inspected);
        let changed = self.rows.as_ref() != rows.as_slice();
        if self.scroll_key.as_ref() != Some(&key) {
            self.scroll.reset_with_uniform_height(
                rows.len(),
                compact::scaled(compact::ROW + compact::GAP, cx),
            );
            self.scroll_key = Some(key);
        } else if changed {
            // Keep the scroll anchor and measured heights outside the changed
            // group, rather than jumping to the top when a group is folded.
            let prefix = self
                .rows
                .iter()
                .zip(&rows)
                .take_while(|(a, b)| a == b)
                .count();
            let suffix = self.rows[prefix..]
                .iter()
                .rev()
                .zip(rows[prefix..].iter().rev())
                .take_while(|(a, b)| a == b)
                .count();
            self.scroll.splice(
                prefix..self.rows.len() - suffix,
                rows.len() - prefix - suffix,
            );
        }
        if changed {
            self.rows = rows.into();
        }
        let rows = self.rows.clone();
        let panel = cx.entity();
        root.child(
            div()
                .id("object-properties")
                .test_support()
                .size_full()
                .child(
                    list(self.scroll.clone(), move |index, _, cx| {
                        panel.update(cx, |this, cx| this.render_row(&rows[index], cx))
                    })
                    .size_full()
                    .py(compact::rem(compact::PADDING)),
                )
                .vertical_scrollbar(&self.scroll),
        )
        .into_any_element()
    }
}

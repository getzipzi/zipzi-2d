use crate::{EditorView, t};
use editor_core::EditorCommand;
use gpui_kit::component::{
    ActiveTheme, Disableable, WindowExt,
    button::{Button, ButtonVariants},
    dock::{BasePanel, Panel, PanelEvent},
};
use gpui_kit::{prelude::*, *};

pub struct PropertiesPanel {
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    _observe: Subscription,
}

impl PropertiesPanel {
    pub fn new(owner: WeakEntity<EditorView>, _: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
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
            return root
                .child(div().p_2().child(t!("editor.no_document")))
                .into_any_element();
        };
        let Some(document) = state.editor.active_id() else {
            return root.into_any_element();
        };
        let index = doc.object_index();
        let inspected = doc
            .context()
            .inspected
            .clone()
            .unwrap_or_else(|| index.objects[0].reference.clone());
        let info = index.get(&inspected);
        let mut rows = info.map_or_else(Vec::new, |info| {
            super::document_panel::property_rows(&info.properties)
        });
        rows.sort_by_key(|(field, _)| (field != "name", field.clone()));
        let doc_ref = index.objects[0].reference.clone();
        let mut body = div()
            .id("object-properties")
            .size_full()
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
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
        if matches!(
            inspected.kind,
            editor_core::ObjectKind::Parameter | editor_core::ObjectKind::ParameterGroup
        ) {
            use super::parameters::ParameterAction;
            let actions = if inspected.kind == editor_core::ObjectKind::Parameter {
                [
                    ParameterAction::Edit(inspected.id.clone()),
                    ParameterAction::Delete(inspected.id.clone()),
                ]
            } else {
                [
                    ParameterAction::EditGroup(inspected.id.clone()),
                    ParameterAction::DeleteGroup(inspected.id.clone()),
                ]
            };
            for (i, action) in actions.into_iter().enumerate() {
                let owner = self.owner.clone();
                body = body.child(
                    Button::new(if i == 0 {
                        "property-edit-parameter"
                    } else {
                        "property-delete-parameter"
                    })
                    .ghost()
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
        for (field, value) in rows {
            body = body.child(
                div()
                    .id(SharedString::from(format!("object-property-{field}")))
                    .test_support()
                    .flex()
                    .flex_shrink_0()
                    .flex_col()
                    .gap_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(div().text_color(cx.theme().muted_foreground).child(field))
                    .child(value),
            );
        }
        root.child(body).into_any_element()
    }
}

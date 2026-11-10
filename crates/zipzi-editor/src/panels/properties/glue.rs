use super::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};
use zipzi_editor_core::{CommandId, EditorCommand};

impl PropertiesPanel {
    pub(super) fn glue_actions(&self, object: &ObjectRef, cx: &Context<Self>) -> AnyElement {
        let mut row = div().flex().gap(compact::rem(6.));
        for (id, label, command) in [
            (
                "glue-edit-points",
                "editor.glue_edit_points",
                CommandId::GlueStitch,
            ),
            (
                "glue-paint-weights",
                "editor.glue_advanced",
                CommandId::GlueTool,
            ),
        ] {
            let object = object.clone();
            row = row.child(
                compact::button(Button::new(id))
                    .label(t!(label).into_owned())
                    .disabled(self.disabled)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.finish_text_preview(cx);
                        if let Some(owner) = this.owner.upgrade() {
                            owner.update(cx, |owner, cx| {
                                if let Err(error) = owner
                                    .editor
                                    .execute(EditorCommand::SelectObject(object.clone()))
                                {
                                    owner.set_edit_error(error.to_string(), cx);
                                    return;
                                }
                                let invocation = owner.editor.command(command);
                                owner.run_invocation(invocation, window, cx);
                            });
                        }
                    })),
            );
        }
        row.into_any_element()
    }
    pub(super) fn glue_follow(&self, object: &ObjectRef, cx: &Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let state = owner.read(cx);
        let binding = state.editor.primary_viewport_binding();
        let controls = binding.and_then(|b| state.editor.glue_paint_controls(b));
        let selected = state
            .editor
            .document()
            .and_then(|d| d.context().selection.as_ref())
            == Some(object);
        let Some(controls) = controls.filter(|_| selected) else {
            return div().into_any_element();
        };
        let options = [
            (None, t!("editor.glue_follow_both").into_owned(), [0.5, 0.5]),
            (
                Some(0),
                t!("editor.glue_follow", name = controls.meshes[0]).into_owned(),
                [0., 1.],
            ),
            (
                Some(1),
                t!("editor.glue_follow", name = controls.meshes[1]).into_owned(),
                [1., 0.],
            ),
        ];
        let label = options
            .iter()
            .find(|(_, _, w)| Some(*w) == controls.uniform_weights)
            .map(|(_, label, _)| label.clone())
            .unwrap_or_else(|| t!("editor.glue_advanced").into_owned());
        let weak = owner.downgrade();
        let object = object.clone();
        compact::button(Button::new("property-glue-follow"))
            .label(label)
            .w_full()
            .disabled(self.disabled || controls.diagnostic.is_some())
            .dropdown_menu(move |mut menu, _, _| {
                menu = compact::menu(menu);
                for (side, label, weights) in &options {
                    let side = *side;
                    let weak = weak.clone();
                    let object = object.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label.clone())
                            .checked(Some(*weights) == controls.uniform_weights)
                            .on_click(move |_, window, cx| {
                                if window.has_active_dialog(cx) {
                                    return;
                                }
                                let _ = weak.update(cx, |owner, cx| {
                                    if owner
                                        .editor
                                        .document()
                                        .and_then(|d| d.context().selection.as_ref())
                                        != Some(&object)
                                    {
                                        return;
                                    }
                                    let result =
                                        owner.editor.set_glue_follow(binding.unwrap(), side);
                                    owner.complete_edit(result, cx);
                                });
                            }),
                    );
                }
                menu
            })
            .into_any_element()
    }
}

use super::*;
use crate::{panels::toolbar::PanelToolbar, ui::compact};
use gpui_kit::assets::IconName;
use zipzi_editor_core::{DocumentId, ObjectKind, WorkspaceKind};

impl AnimationPanel {
    pub(super) fn cursor_preview_controls(
        &self,
        toolbar: PanelToolbar,
        document: DocumentId,
        active: bool,
        disabled: bool,
    ) -> PanelToolbar {
        let owner = self.owner.clone();
        let toggle_owner = owner.clone();
        toolbar
            .action(move |_| {
                compact::toggle(Button::new("physics-cursor-preview"), active)
                    .icon(IconName::MousePointer)
                    .tooltip(t!("physics.cursor_preview"))
                    .accessibility_label(t!("physics.cursor_preview"))
                    .disabled(disabled)
                    .on_click(move |_, window, cx| {
                        let _ = toggle_owner.update(cx, |view, cx| {
                            if !preview_controls_available(view, document, window, cx) {
                                return;
                            }
                            let enabled = !view.editor.cursor_physics_active();
                            let result = view.editor.set_physics_cursor(document, enabled);
                            view.animation_anchor = None;
                            view.complete_edit(result, cx);
                        });
                    })
            })
            .action(move |_| {
                Button::new("physics-cursor-mapping")
                    .icon(IconName::Settings2)
                    .ghost()
                    .tooltip(t!("physics.cursor_mapping"))
                    .accessibility_label(t!("physics.cursor_mapping"))
                    .disabled(disabled)
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |view, cx| {
                            if !preview_controls_available(view, document, window, cx) {
                                return;
                            }
                            let Some(object) = view.editor.document().and_then(|doc| {
                                doc.object_index().objects.iter().find_map(|info| {
                                    (info.reference.kind == ObjectKind::Document)
                                        .then(|| info.reference.clone())
                                })
                            }) else {
                                return;
                            };
                            let result =
                                view.inspect_authoring_object(document, object, false, window, cx);
                            if result.is_ok()
                                && let Some(dock) = &view.dock
                            {
                                dock.properties
                                    .update(cx, |panel, cx| panel.reveal_cursor_mapping(cx));
                            }
                            view.complete_edit(result, cx);
                        });
                    })
            })
    }
}

fn preview_controls_available(
    view: &mut EditorView,
    document: DocumentId,
    window: &mut Window,
    cx: &mut Context<EditorView>,
) -> bool {
    !view.modal
        && !window.has_active_dialog(cx)
        && !view.editor.is_busy()
        && view.editor.active_id() == Some(document)
        && view.editor.workspace() == WorkspaceKind::Animation
        && view.prepare_property_inputs(document, window, cx)
}

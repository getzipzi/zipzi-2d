//! The same frame controls in Properties and the owning viewport toolbar.
use crate::{EditorView, authoring::AuthoringContext, t, ui::compact};
use gpui_kit::component::{
    Disableable, WindowExt,
    button::Button,
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::{prelude::*, *};
use zipzi_editor_core::{ObjectRef, ViewportBinding, WarpFrameAction};

pub(crate) fn menu(
    button: Button,
    owner: WeakEntity<EditorView>,
    binding: ViewportBinding,
    object: ObjectRef,
    cx: &App,
) -> AnyElement {
    let Some(view) = owner.upgrade() else {
        return button.disabled(true).into_any_element();
    };
    let view = view.read(cx);
    let context = AuthoringContext::capture(&view.editor);
    let target = view.editor.warp_transform_target_for(binding, &object);
    let info = view.editor.warp_frame_info(binding, &object);
    let disabled = view.modal
        || info.is_none()
        || !view
            .editor
            .document()
            .is_some_and(|d| d.context().selections.contains(&object));
    button
        .disabled(disabled)
        .dropdown_menu(move |mut menu, _, _| {
            menu = compact::menu(menu);
            for (id, label, action, checked) in [
                (
                    "warp-frame-visible",
                    "editor.warp_frame_visible",
                    WarpFrameAction::ToggleVisible,
                    info.is_some_and(|i| i.visible),
                ),
                (
                    "warp-frame-adjust",
                    "editor.warp_frame_adjust",
                    WarpFrameAction::ToggleAdjust,
                    info.is_some_and(|i| i.adjusting),
                ),
                (
                    "warp-frame-align",
                    "editor.warp_frame_align",
                    WarpFrameAction::Align,
                    false,
                ),
                (
                    "warp-frame-reset",
                    "editor.warp_frame_reset",
                    WarpFrameAction::Reset,
                    false,
                ),
            ] {
                let owner = owner.clone();
                let object = object.clone();
                let target = target.clone();
                menu = menu.item(
                    PopupMenuItem::element(move |_, _| {
                        div().id(id).test_support().child(t!(label).into_owned())
                    })
                    .checked(checked)
                    .on_click(move |_, window, cx| {
                        if window.has_active_dialog(cx) {
                            return;
                        }
                        let _ = owner.update(cx, |view, cx| {
                            if view.modal
                                || AuthoringContext::capture(&view.editor) != context
                                || view.editor.warp_transform_target_for(binding, &object) != target
                            {
                                return;
                            }
                            let result =
                                view.editor
                                    .change_warp_frame(binding, object.clone(), action);
                            view.complete_edit(result, cx);
                        });
                    }),
                );
            }
            menu
        })
        .into_any_element()
}

impl EditorView {
    pub(crate) fn warp_property_viewport(&self) -> Option<ViewportBinding> {
        self.focused_viewport
            .and_then(|v| self.editor.viewport_binding(v))
            .filter(|b| b.kind == zipzi_editor_core::ViewportKind::Modeling)
            .or_else(|| self.editor.primary_viewport_binding())
    }
}

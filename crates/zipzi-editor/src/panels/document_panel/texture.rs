use super::*;
use crate::authoring::{ActionAvailability, texture::Control};
use crate::panels::toolbar::PanelToolbar;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Disableable,
    button::{Button, ButtonVariants},
};
impl DocumentPanel {
    pub(super) fn texture_toolbar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let dialog = window.has_active_dialog(cx);
        let Some(owner) = self.owner.upgrade() else {
            return Empty.into_any_element();
        };
        let view = owner.read(cx);
        if view.editor.editing_context() != Some(zipzi_editor_core::EditingContext::Modeling) {
            return Empty.into_any_element();
        }
        let context = AuthoringContext::capture(&view.editor);
        let capabilities = view
            .editor
            .document()
            .map(|d| d.resource_collection_capabilities());
        let Some((capabilities, session)) = capabilities else {
            return Empty.into_any_element();
        };
        let Some(availability) = ActionAvailability::from_core(&capabilities.create) else {
            return Empty.into_any_element();
        };
        let disabled = !availability.enabled
            || session.is_some()
            || view.modal
            || view.editor.is_busy()
            || dialog;
        let mut toolbar = PanelToolbar::new("resources-panel-toolbar");
        for (control, icon) in [
            (Control::New, IconName::Plus),
            (Control::Import, IconName::FolderOpen),
        ] {
            let owner = self.owner.clone();
            toolbar = toolbar.action(move |_| {
                Button::new(SharedString::from(format!("texture-{}", control.key())))
                    .icon(icon)
                    .accessibility_label(control.label())
                    .tooltip(control.label())
                    .disabled(disabled)
                    .on_click(move |_, window, cx| {
                        if let Some(context) = context {
                            let _ = owner.update(cx, |v, cx| {
                                v.run_texture_control(context, None, control, window, cx)
                            });
                        }
                    })
            });
        }
        toolbar.into_any_element()
    }

    pub(super) fn texture_imports(&self, cx: &Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return Empty.into_any_element();
        };
        let view = owner.read(cx);
        let pending = view.pending_texture_imports();
        if pending.is_empty() {
            return Empty.into_any_element();
        }
        let context = AuthoringContext::capture(&view.editor);
        let mut toolbar = div().flex().gap_1().p_1().flex_shrink_0();
        for key in pending {
            let owner = self.owner.clone();
            toolbar = toolbar.child(
                compact::button(Button::new(SharedString::from(format!(
                    "texture-cancel-import-{key}"
                ))))
                .ghost()
                .label(Control::Cancel.label())
                .disabled(view.modal)
                .on_click(move |_, window, cx| {
                    if let Some(context) = context {
                        let _ = owner.update(cx, |v, cx| {
                            v.run_texture_control(
                                context,
                                Some(key.clone()),
                                Control::Cancel,
                                window,
                                cx,
                            )
                        });
                    }
                }),
            );
        }
        toolbar.into_any_element()
    }
}

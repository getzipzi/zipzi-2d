//! Provider-defined navigation, independent of authored property edits.
use super::*;
use gpui_kit::component::menu::{DropdownMenu, PopupMenuItem};

impl PropertiesPanel {
    pub(super) fn render_navigation(&self, cx: &Context<Self>) -> AnyElement {
        let (Some(navigation), Some(target)) =
            (self.definition.navigation.clone(), self.target.clone())
        else {
            return Empty.into_any_element();
        };
        let context = self.context;
        let epoch = self.epoch;
        let weak = cx.weak_entity();
        compact::button(Button::new("property-navigation"))
            .label(t!(navigation.label))
            .w_full()
            .dropdown_menu(move |mut menu, _, _| {
                menu = compact::menu(menu);
                for (index, choice) in navigation.choices.iter().enumerate() {
                    let value = choice.value.clone();
                    let label = choice.label.clone();
                    let target = target.clone();
                    let weak = weak.clone();
                    let select = navigation.select;
                    menu = menu.item(
                        PopupMenuItem::element(move |_, _| {
                            div()
                                .id(("property-navigation-choice", index))
                                .test_support()
                                .child(label.clone())
                        })
                        .on_click(move |_, window, cx| {
                            if window.has_active_dialog(cx) {
                                return;
                            }
                            let _ = weak.update(cx, |this, cx| {
                                let Some(owner) = this.owner.upgrade() else {
                                    return;
                                };
                                let state = owner.read(cx);
                                if !this.visible
                                    || this.epoch != epoch
                                    || state.modal
                                    || state.editor.is_busy()
                                    || state.editor.exiting()
                                    || state.editor.editing_context_token() != context
                                    || this.resolved_target(&state.editor).as_ref() != Some(&target)
                                {
                                    return;
                                }
                                this.finish_text_preview(cx);
                                owner.update(cx, |state, cx| {
                                    let result = select(&mut state.editor, &target, &value);
                                    state.complete_edit(result, cx);
                                });
                            });
                        }),
                    );
                }
                menu
            })
            .into_any_element()
    }
}

//! The stored-form editor remains usable for pending axes and larger grids.
use super::*;
use crate::authoring::{AuthoringContext, binding::Control};
use gpui_kit::component::{
    Disableable,
    button::{Button, ButtonVariants},
};
impl DocumentPanel {
    pub(super) fn binding_manage(
        &self,
        source: &ObjectRef,
        control: Control,
        cx: &Context<Self>,
    ) -> AnyElement {
        let owner = self.owner.clone();
        let source = source.clone();
        let context = owner
            .upgrade()
            .and_then(|o| AuthoringContext::capture(&o.read(cx).editor));
        compact::button(Button::new("keyform-manage-binding"))
            .ghost()
            .label(control.label())
            .on_click(move |_, window, cx| {
                if let Some(context) = context {
                    let _ = owner.update(cx, |v, cx| {
                        v.run_binding_control(context, source.clone(), control, window, cx)
                    });
                }
            })
            .into_any_element()
    }
    pub(super) fn keyform_list(
        &self,
        index: Arc<ObjectIndex>,
        reference: &ObjectRef,
        document: DocumentId,
        disabled: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let reference = reference.clone();
        let count = index.binding(&reference).map_or(0, |b| b.points.len());
        let owner = self.owner.clone();
        let context = owner
            .upgrade()
            .and_then(|v| AuthoringContext::capture(&v.read(cx).editor));
        uniform_list("stored-keyforms", count, move |range, _, _| {
            let points = &index.binding(&reference).unwrap().points;
            range
                .map(|i| {
                    let owner = owner.clone();
                    let binding = reference.clone();
                    compact::button(Button::new(SharedString::from(format!(
                        "stored-keyform-{i}"
                    ))))
                    .ghost()
                    .label(format!("{}: {:?}", t!("editor.stored_keyform"), points[i]))
                    .disabled(disabled)
                    .w_full()
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |v, cx| {
                            if AuthoringContext::capture(&v.editor) != context
                                || !v.prepare_property_inputs(document, window, cx)
                            {
                                return;
                            }
                            let result = v.editor.execute_for(
                                document,
                                EditorCommand::PreviewKeyform {
                                    binding: binding.clone(),
                                    point: i,
                                },
                            );
                            v.complete_edit(result, cx);
                            v.redraw(cx);
                        });
                    })
                })
                .collect()
        })
        .h(compact::rem(180.))
        .into_any_element()
    }
}

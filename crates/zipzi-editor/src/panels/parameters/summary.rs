//! Binding-summary presentation and navigation actions.
use super::*;

impl ParametersPanel {
    pub(super) fn binding_summary(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let state = owner.read(cx);
        let (Some(doc), Some(id)) = (state.editor.document(), state.editor.active_id()) else {
            return div().into_any_element();
        };
        let Some(focus) = doc.context().parameter_focus.as_ref() else {
            return div().into_any_element();
        };
        let index = doc.object_index();
        let bindings = index.parameter(&focus.id);
        let mut body = div()
            .id("parameter-binding-summary")
            .max_h(px(150.))
            .overflow_y_scroll()
            .flex_shrink_0()
            .text_xs()
            .p_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(format!(
                "{} · {} {} · {} {} · BlendShape {}",
                index.name(focus),
                t!("editor.direct_bindings"),
                bindings.direct_count(&index),
                t!("editor.indirect_influences"),
                bindings.indirect_count(),
                bindings.blend.len()
            ));
        for reference in bindings.regular.iter().chain(&bindings.blend) {
            if let Some(b) = index.binding(reference) {
                let label = format!(
                    "{} · {:?} · {} · {}",
                    index.name(&b.target),
                    b.target.kind,
                    b.axes
                        .iter()
                        .map(|a| index
                            .name(&zipzi_editor_core::ObjectRef::new(
                                zipzi_editor_core::ObjectKind::Parameter,
                                &a.parameter_id
                            ))
                            .to_owned())
                        .collect::<Vec<_>>()
                        .join(" × "),
                    b.layout()
                );
                let reference = reference.clone();
                let owner = self.owner.clone();
                body = body.child(
                    div()
                        .id(SharedString::from(format!(
                            "parameter-binding-{:?}-{}",
                            reference.kind, reference.id
                        )))
                        .test_support()
                        .py_1()
                        .cursor_pointer()
                        .hover(|s| s.bg(cx.theme().muted))
                        .child(label)
                        .on_click(cx.listener(move |_, _, window, cx| {
                            if window.has_active_dialog(cx) {
                                return;
                            }
                            if let Some(owner) = owner.upgrade() {
                                owner.update(cx, |owner, cx| {
                                    if owner.modal
                                        || owner.editor.is_busy()
                                        || owner.editor.active_id() != Some(id)
                                    {
                                        return;
                                    }
                                    let result = owner.editor.execute_for(
                                        id,
                                        EditorCommand::OpenBinding(reference.clone()),
                                    );
                                    if result.is_ok() {
                                        owner.reveal_keyforms(window, cx);
                                    }
                                    owner.complete_edit(result, cx);
                                });
                            }
                        })),
                );
            }
        }
        body = body.child(
            div()
                .id("parameter-indirect-toggle")
                .py_1()
                .cursor_pointer()
                .child(t!("editor.indirect_influences"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.show_indirect = !this.show_indirect;
                    cx.notify();
                })),
        );
        if self.show_indirect {
            for path in &bindings.indirect {
                let reference = path.target.clone();
                let label = path
                    .path
                    .iter()
                    .map(|r| index.name(r).to_owned())
                    .collect::<Vec<_>>()
                    .join(" → ");
                let owner = self.owner.clone();
                body = body.child(
                    div()
                        .id(SharedString::from(format!("indirect-path-{label}")))
                        .py_1()
                        .cursor_pointer()
                        .child(label)
                        .on_click(cx.listener(move |_, _, window, cx| {
                            if window.has_active_dialog(cx) {
                                return;
                            }
                            if let Some(owner) = owner.upgrade() {
                                owner.update(cx, |owner, cx| {
                                    if owner.modal
                                        || owner.editor.is_busy()
                                        || owner.editor.active_id() != Some(id)
                                    {
                                        return;
                                    }
                                    let result = owner.editor.execute_for(
                                        id,
                                        EditorCommand::Navigate(reference.clone()),
                                    );
                                    owner.complete_edit(result, cx);
                                });
                            }
                        })),
                );
            }
        }
        body.into_any_element()
    }
}

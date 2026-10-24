//! Expression member actions use the same coordinator as the resource palettes.
use super::*;
use crate::authoring::{
    AuthoringContext,
    animation::{AnimationAction, create_label},
};
use zipzi_editor_core::ObjectKind;

impl PropertiesPanel {
    pub(super) fn animation_actions(
        &self,
        object: &ObjectRef,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return Empty.into_any_element();
        };
        let context = AuthoringContext::capture(&owner.read(cx).editor);
        let mut actions = vec![];
        if matches!(
            object.kind,
            ObjectKind::Expression | ObjectKind::ExpressionEntry
        ) {
            actions.push((
                "new",
                gpui_kit::assets::IconName::Plus,
                create_label(ObjectKind::ExpressionEntry),
                AnimationAction::Create(ObjectKind::ExpressionEntry, Some(object.clone())),
            ));
        }
        if object.kind == ObjectKind::ExpressionEntry {
            actions.extend([
                (
                    "up",
                    gpui_kit::assets::IconName::ArrowUp,
                    t!("authoring.move_up").into(),
                    AnimationAction::Step(object.clone(), false),
                ),
                (
                    "down",
                    gpui_kit::assets::IconName::ArrowDown,
                    t!("authoring.move_down").into(),
                    AnimationAction::Step(object.clone(), true),
                ),
                (
                    "remove",
                    gpui_kit::assets::IconName::Trash,
                    t!("authoring.remove_expression_entry").into(),
                    AnimationAction::RemoveEntry(object.clone()),
                ),
            ]);
        }
        let mut row = div().w_full().flex().items_center().gap_1();
        if object.kind == ObjectKind::ExpressionEntry {
            row = row.child(
                div()
                    .flex_1()
                    .text_ellipsis()
                    .child(t!("authoring.expression_member")),
            );
        }
        for (key, icon, label, action) in actions {
            let Some(availability) = action.availability(&owner.read(cx).editor) else {
                continue;
            };
            let hint = availability.reason.unwrap_or_else(|| label.clone());
            let disabled = self.disabled || !availability.enabled;
            let owner = self.owner.clone();
            let id: SharedString = format!("expression-action-{key}-{}", object.id).into();
            row = row.child(
                Button::new(id)
                    .icon(icon)
                    .ghost()
                    .xsmall()
                    .accessibility_label(label.clone())
                    .tooltip(hint)
                    .disabled(disabled)
                    .on_click(move |_, window, cx| {
                        if let Some(context) = context {
                            let _ = owner.update(cx, |owner, cx| {
                                owner.run_animation_action(context, action.clone(), window, cx)
                            });
                        }
                    }),
            );
        }
        row.into_any_element()
    }
}

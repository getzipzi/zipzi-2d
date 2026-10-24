use super::*;
use crate::authoring::{AuthoringContext, business};
use gpui_kit::assets::IconName;
use zipzi_editor_core::{BusinessAction as A, ObjectKind as K};
impl PropertiesPanel {
    pub(super) fn business_actions(
        &self,
        object: &ObjectRef,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return Empty.into_any_element();
        };
        let view = owner.read(cx);
        let context = AuthoringContext::capture(&view.editor);
        let mut actions = vec![];
        for kind in match object.kind {
            K::Pose => vec![K::PoseGroup],
            K::PoseGroup => vec![K::PoseEntry],
            K::PoseEntry | K::PoseLink => vec![K::PoseLink],
            K::Physics => vec![K::PhysicsGroup],
            _ => vec![],
        } {
            actions.push((
                "new",
                IconName::Plus,
                business::create_label(kind),
                A::Create(kind, Some(object.clone())),
            ));
        }
        actions.extend([
            (
                "copy",
                IconName::Copy,
                t!("authoring.copy_business").into(),
                A::Copy(object.clone()),
            ),
            (
                "delete",
                IconName::Trash,
                t!("authoring.delete_business").into(),
                A::Delete(object.clone()),
            ),
            (
                "up",
                IconName::ArrowUp,
                t!("authoring.move_up").into(),
                A::Step(object.clone(), false),
            ),
            (
                "down",
                IconName::ArrowDown,
                t!("authoring.move_down").into(),
                A::Step(object.clone(), true),
            ),
        ]);
        let mut row = div().flex().items_center().gap_1().w_full();
        for (id, icon, label, action) in actions {
            let Some(availability) = business::availability(&view.editor, &action) else {
                continue;
            };
            let hint = availability.reason.unwrap_or(label.clone());
            let owner = self.owner.clone();
            row = row.child(
                compact::header_button(
                    Button::new(SharedString::from(format!(
                        "business-action-{id}-{}",
                        object.id
                    )))
                    .icon(icon)
                    .ghost()
                    .tooltip(hint)
                    .accessibility_label(label),
                )
                .disabled(self.disabled || !availability.enabled)
                .on_click(move |_, window, cx| {
                    if let Some(context) = context {
                        let _ = owner.update(cx, |v, cx| {
                            v.run_business_action(context, action.clone(), window, cx)
                        });
                    }
                }),
            );
        }
        if matches!(object.kind, K::PoseGroup | K::PoseEntry | K::PoseLink) {
            let owner = self.owner.clone();
            let object = object.clone();
            let migrate = object.kind == K::PoseGroup;
            row = row.child(
                compact::header_button(
                    Button::new("business-move-to")
                        .icon(IconName::Folder)
                        .ghost()
                        .tooltip(t!(if migrate {
                            "authoring.migrate_pose_group"
                        } else {
                            "authoring.move_to"
                        })),
                )
                .disabled(self.disabled)
                .on_click(move |_, window, cx| {
                    if let Some(context) = context {
                        let _ = owner.update(cx, |v, cx| {
                            v.open_business_destination(
                                context,
                                object.clone(),
                                migrate,
                                window,
                                cx,
                            )
                        });
                    }
                }),
            );
        }
        row.into_any_element()
    }
}

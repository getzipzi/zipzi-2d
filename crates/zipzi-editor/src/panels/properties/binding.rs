use super::*;
use crate::authoring::{ActionAvailability, AuthoringContext, binding::Control};
use zipzi_editor_core::{EditingContext, ObjectKind as K, ObjectRef};

pub(super) fn rows(doc: &zipzi_editor_core::Document, target: &PropertyTarget) -> Vec<Row> {
    if target.context != EditingContext::Modeling || target.timeline.is_some() {
        return vec![];
    }
    let source = &target.object;
    if !matches!(
        source.kind,
        K::Mesh
            | K::Deformer
            | K::Part
            | K::MeshBinding
            | K::SceneBinding
            | K::BindingAxis
            | K::BindingSample
    ) {
        return vec![];
    }
    let mut rows = vec![];
    let mut add = |object: ObjectRef, control: Control, label: SharedString| {
        rows.push(Row::BindingAction {
            object,
            control,
            label,
        })
    };
    let binding = doc.ordinary_binding(source);
    if matches!(source.kind, K::Mesh | K::Deformer | K::Part) {
        if binding.is_none() {
            add(source.clone(), Control::Create, Control::Create.label());
        }
        return rows;
    }
    let Some(b) = binding else { return rows };
    add(
        b.reference.clone(),
        Control::Target,
        Control::Target.label(),
    );
    if matches!(source.kind, K::MeshBinding | K::SceneBinding) {
        add(
            b.reference.clone(),
            Control::AddAxis,
            Control::AddAxis.label(),
        );
        add(
            b.reference.clone(),
            Control::Delete,
            Control::Delete.label(),
        );
        if b.target.kind == K::Part && b.axes.iter().any(|a| !a.parameter_id.is_empty()) {
            add(
                b.reference.clone(),
                Control::PartDrawOrder,
                Control::PartDrawOrder.label(),
            );
        }
        let index = doc.object_index();
        for a in &b.axes {
            let parameter = if a.parameter_id.is_empty() {
                t!("binding.unconfigured").into_owned()
            } else {
                index
                    .name(&ObjectRef::new(K::Parameter, &a.parameter_id))
                    .to_owned()
            };
            // Navigation is inspection-only; children retain stable UUID addresses.
            rows.push(Row::BindingNavigate {
                object: ObjectRef::new(K::BindingAxis, &a.id),
                label: format!(
                    "{} · {} ({})",
                    t!("binding.axis"),
                    parameter,
                    a.samples.len()
                )
                .into(),
            });
        }
    } else if source.kind == K::BindingAxis {
        add(
            source.clone(),
            Control::Parameter,
            Control::Parameter.label(),
        );
        add(
            source.clone(),
            Control::InsertSample,
            Control::InsertSample.label(),
        );
        add(
            source.clone(),
            Control::RemoveAxis,
            Control::RemoveAxis.label(),
        );
        for s in b
            .axes
            .iter()
            .filter(|a| a.id == source.id)
            .flat_map(|a| &a.samples)
        {
            rows.push(Row::BindingNavigate {
                object: ObjectRef::new(K::BindingSample, &s.id),
                label: format!("{}: {}", t!("binding.value"), s.value).into(),
            });
        }
    } else if source.kind == K::BindingSample {
        add(
            source.clone(),
            Control::MoveSample,
            Control::MoveSample.label(),
        );
        add(
            source.clone(),
            Control::DeleteSample,
            Control::DeleteSample.label(),
        );
        let axis = b
            .axes
            .iter()
            .find(|a| a.samples.iter().any(|s| s.id == source.id))
            .unwrap();
        rows.push(Row::BindingNavigate {
            object: ObjectRef::new(K::BindingAxis, &axis.id),
            label: Control::Parameter.label(),
        });
    }
    rows
}
impl PropertiesPanel {
    pub(super) fn binding_action(
        &self,
        object: &ObjectRef,
        control: Control,
        label: &SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(owner) = self.owner.upgrade() else {
            return Empty.into_any_element();
        };
        let view = owner.read(cx);
        let context = AuthoringContext::capture(&view.editor);
        let navigation = matches!(control, Control::Open | Control::Target | Control::Inspect);
        let availability = view.editor.document().and_then(|d| {
            if control == Control::Create {
                d.ordinary_binding_creation_capability(object)
                    .ok()
                    .and_then(|c| ActionAvailability::from_core(&c))
            } else if navigation {
                Some(ActionAvailability {
                    enabled: true,
                    reason: None,
                })
            } else {
                ActionAvailability::for_object(
                    d,
                    object,
                    if control == Control::Delete {
                        zipzi_editor_core::AuthoringAction::Delete
                    } else {
                        zipzi_editor_core::AuthoringAction::Patch
                    },
                )
            }
        });
        let Some(availability) = availability else {
            return Empty.into_any_element();
        };
        let owner = self.owner.clone();
        let source = object.clone();
        compact::button(Button::new(SharedString::from(format!(
            "binding-{}-{}",
            control.key(),
            object.id
        ))))
        .ghost()
        .label(label.clone())
        .tooltip(availability.reason.unwrap_or(label.clone()))
        .disabled((!navigation && self.disabled) || !availability.enabled)
        .w_full()
        .justify_start()
        .on_click(move |_, window, cx| {
            if let Some(context) = context {
                let _ = owner.update(cx, |v, cx| {
                    v.run_binding_control(context, source.clone(), control, window, cx)
                });
            }
        })
        .into_any_element()
    }
}

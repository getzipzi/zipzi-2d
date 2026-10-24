use super::support;
use zipzi_editor_core::{ObjectKind, ObjectRef, PropertyScope};
use zipzi_sdk::{
    BindingAuthoringEdit as E, MeshGridEdit as G, ParameterRangeEdit, ParameterRangePolicy,
};

#[test]
fn zero_axis_creation_projects_and_preserves_base_property_editing() -> anyhow::Result<()> {
    let (mut d, _source) = support::sample_document()?;
    let mesh_id = d.selected_id().unwrap().to_string();
    let object = ObjectRef::new(ObjectKind::Mesh, &mesh_id);
    let id = uuid::Uuid::new_v4().to_string();
    let plan = d.plan_binding_authoring(E::MeshGrid(G::Create {
        id: id.clone(),
        mesh_id: mesh_id.clone(),
    }))?;
    let result = d.execute_binding_authoring(&plan)?;
    assert_eq!(
        result.suggested_selection,
        vec![ObjectRef::new(ObjectKind::MeshBinding, &id)]
    );
    assert!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::MeshBinding, &id))
            .is_some()
    );
    assert!(matches!(d.property_scope(&object), PropertyScope::Object));
    d.undo()?;
    assert!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::MeshBinding, &id))
            .is_none()
    );
    d.redo()?;
    assert!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::MeshBinding, &id))
            .is_some()
    );
    let target = tempfile::tempdir()?;
    d.save(target.path())?;
    Ok(())
}
#[test]
fn editor_consumes_joint_range_receipt_and_undo_restores_parameter_visuals() -> anyhow::Result<()> {
    let (mut d, _source) = support::sample_document_with_binding(true)?;
    let parameter = d.parameters().remove(0);
    let plan = d.plan_binding_authoring(E::ParameterRange(ParameterRangeEdit {
        parameter_id: parameter.id.clone(),
        minimum: 10.0,
        maximum: 20.0,
        default_value: 15.0,
        policy: ParameterRangePolicy::MapKeyValues,
    }))?;
    let result = d.execute_binding_authoring(&plan)?;
    assert!(result.receipt.changed);
    assert!(result.suggested_selection.is_empty());
    assert_eq!(
        d.parameter_visuals()[&parameter.id]
            .keys
            .iter()
            .map(|k| k.value)
            .collect::<Vec<_>>(),
        vec![10.0, 20.0]
    );
    d.undo()?;
    assert_eq!(
        d.parameter_visuals()[&parameter.id]
            .keys
            .iter()
            .map(|k| k.value)
            .collect::<Vec<_>>(),
        vec![0.0, 1.0]
    );
    d.redo()?;
    assert_eq!(
        d.parameter_visuals()[&parameter.id]
            .keys
            .iter()
            .map(|k| k.value)
            .collect::<Vec<_>>(),
        vec![10.0, 20.0]
    );
    Ok(())
}

#[test]
fn pending_axes_keep_stable_rows_and_properties_target_active_samples() -> anyhow::Result<()> {
    use zipzi_core::BindingAxis;
    use zipzi_sdk::{BindingGridEdit, GridOperation};
    let (mut d, _) = support::sample_document_with_binding(true)?;
    let target = ObjectRef::new(ObjectKind::Mesh, d.selected_id().unwrap());
    let index = d.object_index();
    let binding = index.bindings.iter().find(|b| b.target == target).unwrap();
    let binding_id = binding.reference.id.clone();
    let old_axis = binding.axes[0].clone();
    let pending = BindingAxis::new("", vec![0., 1.]);
    let plan = d.plan_binding_authoring(E::Grid(BindingGridEdit::Edit {
        owner: binding_id.clone(),
        operation: GridOperation::AddAxis {
            axis: pending.clone(),
        },
    }))?;
    d.execute_binding_authoring(&plan)?;
    let index = d.object_index();
    let row = ObjectRef::new(ObjectKind::BindingAxis, &pending.id);
    assert_eq!(
        index.get(&row).unwrap().properties["fields"]["parameter_id"],
        ""
    );
    assert!(
        index
            .get(&ObjectRef::new(
                ObjectKind::BindingSample,
                &pending.samples[0].id
            ))
            .is_some()
    );
    d.set_preview_parameter(&old_axis.parameter_id, 0.)?;
    assert!(
        matches!(d.property_scope(&target), PropertyScope::MeshKeyform { samples, .. } if samples == vec![old_axis.samples[0].id.clone()])
    );
    let plan = d.plan_binding_authoring(E::Grid(BindingGridEdit::Edit {
        owner: binding_id.clone(),
        operation: GridOperation::ReorderAxes {
            axis_ids: vec![pending.id.clone(), old_axis.id.clone()],
        },
    }))?;
    d.execute_binding_authoring(&plan)?;
    assert!(
        matches!(d.property_scope(&target), PropertyScope::MeshKeyform { samples, .. } if samples == vec![old_axis.samples[0].id.clone()])
    );
    d.undo()?;
    d.undo()?;
    assert!(d.object_index().get(&row).is_none());
    d.redo()?;
    assert!(d.object_index().get(&row).is_some());
    Ok(())
}

#[test]
fn editor_binding_authoring_uses_modeling_context_and_rechecks_execution() -> anyhow::Result<()> {
    let (d, _source) = support::sample_document()?;
    let mesh = d.selected_id().unwrap().to_string();
    let mut editor = support::editor_from_document(d);
    let document = editor.active_id().unwrap();
    let intent = E::MeshGrid(G::Create {
        id: uuid::Uuid::new_v4().to_string(),
        mesh_id: mesh,
    });
    let plan = editor.plan_binding_authoring(document, intent.clone())?;
    let history = editor.document().unwrap().history_lengths();
    editor.set_workspace(zipzi_editor_core::WorkspaceKind::Animation)?;
    assert!(editor.plan_binding_authoring(document, intent).is_err());
    assert!(editor.execute_binding_authoring(document, &plan).is_err());
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    editor.set_workspace(zipzi_editor_core::WorkspaceKind::Modeling)?;
    assert!(
        editor
            .execute_binding_authoring(document, &plan)?
            .receipt
            .changed
    );
    Ok(())
}

#[test]
fn cancelled_binding_plan_keeps_editor_projection_history_and_redo() -> anyhow::Result<()> {
    let (mut d, _) = support::sample_document_with_binding(true)?;
    let binding = d.object_index().bindings[0].reference.id.clone();
    let axis = d.object_index().bindings[0].axes[0].clone();
    let history = d.history_lengths();
    let source = d.object_index();
    let control = zipzi_sdk::ConversionControl::default();
    let plan = d.plan_binding_authoring_with_control(
        E::Grid(zipzi_sdk::BindingGridEdit::Edit {
            owner: binding.clone(),
            operation: zipzi_sdk::GridOperation::InsertSample {
                axis_id: axis.id,
                id: uuid::Uuid::new_v4().to_string(),
                value: 0.5,
            },
        }),
        &control,
    )?;
    assert!(!plan.report().generated.is_empty());
    control.cancel();
    assert!(d.execute_binding_authoring(&plan).is_err());
    assert_eq!(d.history_lengths(), history);
    assert!(std::sync::Arc::ptr_eq(&d.object_index(), &source));
    Ok(())
}

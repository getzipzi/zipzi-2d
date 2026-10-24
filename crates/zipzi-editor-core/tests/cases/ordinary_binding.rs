use super::support;
use zipzi_editor_core::{
    Document, ObjectKind as K, ObjectRef as R, OrdinaryBindingAction as A, OrdinaryBindingPlan,
};
use zipzi_sdk::{SceneContentTarget as T, SceneContentValue as V, ShapeTarget};
fn apply(d: &mut Document, a: A) -> anyhow::Result<OrdinaryBindingPlan> {
    let p = d.plan_ordinary_binding(&a)?;
    d.execute_ordinary_binding(&p)?;
    Ok(p)
}
fn fixture(
    (mut d, assets): (Document, tempfile::TempDir),
) -> anyhow::Result<(Document, tempfile::TempDir)> {
    d.create_parameter(zipzi_editor_core::NewParameter {
        name: "Drive".into(),
        runtime_id: "Drive".into(),
        minimum: 0.,
        maximum: 1.,
        default_value: 0.,
        ..Default::default()
    })?;
    let saved = d.save(&assets.path().join("with-parameter.zipzi"))?;
    let mut reopened = Document::open(&saved.manifest)?;
    reopened.navigate(R::new(K::Mesh, &reopened.layers()[0].id))?;
    Ok((reopened, assets))
}
#[test]
fn mesh_binding_workflow_edits_stored_shape_interpolates_and_roundtrips() -> anyhow::Result<()> {
    let (mut d, assets) = fixture(support::sample_document()?)?;
    let mesh = R::new(K::Mesh, d.selected_id().unwrap());
    let parameter = d.parameters()[0].id.clone();
    let p = apply(&mut d, A::Create(mesh.clone()))?;
    let binding = p.binding;
    assert!(d.ordinary_binding(&mesh).unwrap().axes.is_empty());
    let p = apply(
        &mut d,
        A::AddAxis {
            binding: binding.clone(),
            parameter: parameter.clone(),
        },
    )?;
    let axis = p.selection;
    let b = d.ordinary_binding(&binding).unwrap();
    assert_eq!(axis.id, b.axes[0].id);
    let last = b.axes[0].samples.last().unwrap().clone();
    let target = ShapeTarget::Ordinary {
        binding_id: binding.id.clone(),
        samples: vec![last.id.clone()],
    };
    let address = d.scene_content_address(
        &mesh.id,
        T::VertexPosition {
            vertex_id: 0,
            shape: target.clone(),
        },
    )?;
    let V::Point(base) = address.expected() else {
        unreachable!()
    };
    let changed = zipzi_core::Vec2::new(base.x + 20., base.y + 10.);
    let p = d.plan_scene_content_patch(&address, V::Point(changed))?;
    d.execute_scene_content(&p)?;
    d.set_preview_parameter(&parameter, last.value)?;
    assert!(
        matches!(d.property_scope(&mesh), zipzi_editor_core::PropertyScope::MeshKeyform { samples, .. } if samples == vec![last.id.clone()])
    );
    let after = d.snapshot()?.viewport_frame.drawables.clone();
    d.set_preview_parameter(&parameter, b.axes[0].samples[0].value)?;
    assert_ne!(after, d.snapshot()?.viewport_frame.drawables);
    d.set_preview_parameter(&parameter, last.value)?;
    d.undo()?;
    assert_ne!(after, d.snapshot()?.viewport_frame.drawables);
    d.redo()?;
    assert_eq!(after, d.snapshot()?.viewport_frame.drawables);
    let saved = d.save(&assets.path().join("ordinary.zipzi"))?;
    let mut reopened = Document::open(&saved.manifest)?;
    reopened.set_preview_parameter(&parameter, last.value)?;
    assert_eq!(
        reopened.ordinary_binding(&mesh).unwrap().axes[0].id,
        axis.id
    );
    assert_eq!(after, reopened.snapshot()?.viewport_frame.drawables);
    Ok(())
}
#[test]
fn pending_axis_parameter_and_samples_use_stable_identity() -> anyhow::Result<()> {
    let (mut d, _) = fixture(support::sample_document()?)?;
    let mesh = R::new(K::Mesh, d.selected_id().unwrap());
    let parameter = d.parameters()[0].id.clone();
    let b = apply(&mut d, A::Create(mesh))?.binding;
    let axis = apply(
        &mut d,
        A::AddAxis {
            binding: b.clone(),
            parameter: String::new(),
        },
    )?
    .selection;
    assert!(d.ordinary_binding(&axis).unwrap().points.is_empty());
    apply(
        &mut d,
        A::SetParameter {
            axis: axis.clone(),
            parameter,
            reduction: None,
        },
    )?;
    let sample = apply(
        &mut d,
        A::InsertSample {
            axis: axis.clone(),
            value: 0.5,
        },
    )?
    .selection;
    apply(
        &mut d,
        A::MoveSample {
            sample: sample.clone(),
            value: 0.4,
        },
    )?;
    let info = d.ordinary_binding(&axis).unwrap();
    let shared = d.ordinary_binding(&axis).unwrap();
    assert_eq!(shared.axes.as_ptr(), info.axes.as_ptr());
    assert_eq!(shared.points.as_ptr(), info.points.as_ptr());
    assert_eq!(info.axes[0].id, axis.id);
    assert!(
        info.axes[0]
            .samples
            .iter()
            .any(|s| s.id == sample.id && s.value == 0.4)
    );
    let delete = d.plan_ordinary_binding(&A::DeleteSample(sample.clone()))?;
    assert!(delete.report.as_ref().unwrap().deleted.iter().any(|a| matches!(&a.element, zipzi_core::document::ConversionElement::Sample(id) if id == &sample.id)));
    d.execute_ordinary_binding(&delete)?;
    d.undo()?;
    assert!(d.object_index().get(&sample).is_some());
    assert!(d.execute_ordinary_binding(&delete).is_err());
    Ok(())
}
#[test]
fn reduction_is_explicit_and_binding_delete_preserves_target_and_parameter() -> anyhow::Result<()> {
    let (mut d, _) = support::sample_document_with_binding(true)?;
    let mesh = R::new(K::Mesh, d.selected_id().unwrap());
    let b = d.ordinary_binding(&mesh).unwrap();
    let axis = R::new(K::BindingAxis, &b.axes[0].id);
    assert!(
        d.plan_ordinary_binding(&A::RemoveAxis {
            axis: axis.clone(),
            reduction: None
        })
        .is_err()
    );
    apply(
        &mut d,
        A::RemoveAxis {
            axis,
            reduction: Some(0.),
        },
    )?;
    assert!(d.ordinary_binding(&mesh).unwrap().axes.is_empty());
    d.undo()?;
    let p = d.plan_ordinary_binding(&A::Delete(b.reference.clone()))?;
    assert!(!p.report.as_ref().unwrap().deleted.is_empty());
    d.execute_ordinary_binding(&p)?;
    assert!(d.ordinary_binding(&mesh).is_none());
    assert!(d.object_index().get(&mesh).is_some());
    assert!(
        d.parameters()
            .iter()
            .any(|p| p.id == b.axes[0].parameter_id)
    );
    d.undo()?;
    assert_eq!(d.ordinary_binding(&mesh).unwrap().reference, b.reference);
    assert!(d.execute_ordinary_binding(&p).is_err());
    d.redo()?;
    assert!(d.ordinary_binding(&mesh).is_none());
    Ok(())
}
#[test]
fn invalid_duplicate_keys_locks_and_stale_plans_do_not_mutate_history() -> anyhow::Result<()> {
    let (mut d, _) = support::sample_document_with_binding(true)?;
    let mesh = R::new(K::Mesh, d.selected_id().unwrap());
    let b = d.ordinary_binding(&mesh).unwrap();
    let axis = R::new(K::BindingAxis, &b.axes[0].id);
    let history = d.history_lengths();
    for value in [f32::NAN, f32::INFINITY, 0., 2.] {
        assert!(
            d.plan_ordinary_binding(&A::InsertSample {
                axis: axis.clone(),
                value
            })
            .is_err()
        );
    }
    assert!(d.plan_ordinary_binding(&A::Create(mesh.clone())).is_err());
    assert_eq!(d.history_lengths(), history);
    let p = d.plan_ordinary_binding(&A::InsertSample { axis, value: 0.5 })?;
    d.set_objects_locked(&[mesh], true)?;
    assert!(d.execute_ordinary_binding(&p).is_err());
    assert!(d.plan_ordinary_binding(&A::Delete(b.reference)).is_err());
    Ok(())
}
#[test]
fn scene_targets_create_correct_tracks_and_part_edits_capture_stored_form() -> anyhow::Result<()> {
    let (mut d, _) = fixture(support::sample_document_with_scene_objects()?)?;
    let parameter = d.parameters()[0].id.clone();
    for target in [
        support::object(K::Deformer, 501),
        support::object(K::Deformer, 502),
        support::object(K::Part, 102),
    ] {
        let b = apply(&mut d, A::Create(target.clone()))?.binding;
        apply(
            &mut d,
            A::AddAxis {
                binding: b.clone(),
                parameter: parameter.clone(),
            },
        )?;
        assert_eq!(d.ordinary_binding(&b).unwrap().target, target);
        if target.kind == K::Part {
            d.set_preview_parameter(&parameter, 0.)?;
            let plan = d.plan_ordinary_binding(&A::PartDrawOrder {
                binding: b.clone(),
                value: 12.,
            })?;
            d.set_preview_parameter(&parameter, 1.)?;
            assert!(d.execute_ordinary_binding(&plan).is_err());
            d.set_preview_parameter(&parameter, 0.)?;
            d.execute_ordinary_binding(&plan)?;
            assert!(
                !d.property_values(&target)?
                    .contains_key(&zipzi_editor_core::property::part::DRAW_ORDER.key)
            );
            d.undo()?;
            d.redo()?;
        }
    }
    Ok(())
}
#[test]
fn editor_context_rechecks_and_offscreen_coupled_delete_is_rejected() -> anyhow::Result<()> {
    let (mut d, _) = fixture(support::sample_document_with_scene_objects()?)?;
    let target = support::object(K::Part, 101);
    let b = apply(&mut d, A::Create(target))?.binding;
    assert!(d.plan_ordinary_binding(&A::Delete(b.clone())).is_err());
    let p = d.plan_ordinary_binding(&A::AddAxis {
        binding: b,
        parameter: d.parameters()[0].id.clone(),
    })?;
    let mut editor = support::editor_from_document(d);
    let document = editor.active_id().unwrap();
    editor.set_workspace(zipzi_editor_core::WorkspaceKind::Animation)?;
    assert!(editor.execute_ordinary_binding(document, &p).is_err());
    editor.set_workspace(zipzi_editor_core::WorkspaceKind::Modeling)?;
    editor.execute_ordinary_binding(document, &p)?;
    Ok(())
}

#[test]
fn three_axis_stored_form_remains_editable_in_the_canvas_and_capture_can_cancel()
-> anyhow::Result<()> {
    use zipzi_editor_core::{CanvasInput, EditorCommand, ViewportMetrics, ViewportPoint};
    let (mut d, _) = support::sample_document_with_binding(true)?;
    let mesh = R::new(K::Mesh, d.selected_id().unwrap());
    let b = d.ordinary_binding(&mesh).unwrap().reference;
    let base = d.selected_vertex_positions();
    for name in ["Drive2", "Drive3"] {
        let parameter = d.create_parameter(zipzi_editor_core::NewParameter {
            name: name.into(),
            runtime_id: name.into(),
            minimum: 0.,
            maximum: 1.,
            default_value: 0.,
            ..Default::default()
        })?;
        apply(
            &mut d,
            A::AddAxis {
                binding: b.clone(),
                parameter,
            },
        )?;
    }
    let capture = d.capture_ordinary_binding(&A::Delete(b.clone()))?;
    let control = zipzi_sdk::ConversionControl::default();
    control.cancel();
    assert!(capture.plan(&control).is_err());
    let mut editor = support::editor_from_document(d);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    editor.execute(EditorCommand::OpenBinding(b))?;
    support::present_canvas(&mut editor)?;
    let overlay = editor.overlay();
    assert!(overlay.unavailable.is_none(), "{:?}", overlay.unavailable);
    let p = overlay.vertices[0].position;
    let history = editor.document().unwrap().history_lengths();
    support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: p,
            shift: false,
            alt: false,
        },
    )?;
    support::pointer_input(
        &mut editor,
        CanvasInput::Up {
            point: ViewportPoint {
                x: p.x + 12.,
                y: p.y,
            },
        },
    )?;
    assert_eq!(
        editor.document().unwrap().history_lengths(),
        (history.0 + 1, 0)
    );
    assert_eq!(editor.document().unwrap().selected_vertex_positions(), base);
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (history.0, 1));
    Ok(())
}

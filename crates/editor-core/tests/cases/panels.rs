use super::support;
use editor_core::{Document, EditorCommand, ObjectKind as K, ObjectRef, RelationKind as R};
use kasane_core::{
    BindingAxis, Canvas, Parameter, Part, PartKeyform, RotationTransform, SceneBinding, SceneTrack,
    Transform, TransformData,
};
use kasane_sdk::AuthoringSession;
use std::sync::Arc;
use uuid::Uuid;
fn id() -> String {
    Uuid::new_v4().to_string()
}

#[test]
fn parameter_focus_and_binding_navigation_preserve_target_and_preview() -> anyhow::Result<()> {
    let (mut doc, _assets) = support::sample_document_with_binding(true)?;
    let mesh = doc.context().selection.clone().unwrap();
    let p = doc.parameters()[0].id.clone();
    let binding = doc.object_index().bindings[0].reference.clone();
    doc.set_preview_parameter(&p, 0.4)?;
    let index = doc.object_index();
    let history = doc.history_lengths();
    doc.focus_parameter(&p)?;
    assert_eq!(doc.context().selection.as_ref(), Some(&mesh));
    assert_eq!(
        doc.context().inspected,
        Some(ObjectRef::new(K::Parameter, &p))
    );
    doc.navigate(binding.clone())?;
    assert_eq!(doc.context().active_binding.as_ref(), Some(&binding));
    assert_eq!(doc.parameters()[0].value, 0.4);
    doc.preview_keyform(&binding, 1)?;
    assert_eq!(doc.parameters()[0].value, 1.);
    doc.navigate_back()?;
    assert_eq!(doc.context().selection.as_ref(), Some(&mesh));
    assert_eq!(
        doc.context().inspected,
        Some(ObjectRef::new(K::Parameter, &p))
    );
    assert_eq!(doc.parameters()[0].value, 1.);
    assert_eq!(doc.history_lengths(), history);
    assert!(Arc::ptr_eq(&index, &doc.object_index()));
    Ok(())
}
#[test]
fn changing_target_clears_incompatible_binding_without_resetting_parameters() -> anyhow::Result<()>
{
    let (mut doc, _) = support::sample_document_with_bound_and_unbound_layers()?;
    let index = doc.object_index();
    let b = &index.bindings[0];
    let other = doc
        .layers()
        .into_iter()
        .find(|m| m.id != b.target.id)
        .unwrap();
    doc.open_binding(b.reference.clone())?;
    doc.set_preview_parameter(&b.axes[0].parameter_id, 0.7)?;
    doc.select(&other.id)?;
    assert!(doc.context().active_binding.is_none());
    assert_eq!(doc.parameters()[0].value, 0.7);
    assert!(doc.preview_keyform(&b.reference, 0).is_err());
    Ok(())
}
#[test]
fn stored_combinations_and_precision_come_from_one_binding() -> anyhow::Result<()> {
    let (mut doc, _) = support::sample_document_with_nonuniform_parameter_keys()?;
    let index = doc.object_index();
    let b = &index.bindings[0];
    assert_eq!(b.axes.len(), 2);
    assert_eq!(b.points.len(), 9);
    doc.open_binding(b.reference.clone())?;
    doc.preview_keyform(&b.reference, 4)?;
    assert!(doc.parameters().iter().all(|p| p.value == 0.375));
    assert_eq!(doc.context().selection, Some(b.target.clone()));
    assert!(doc.selected_layer().is_none());
    Ok(())
}
#[test]
fn independent_parameter_bindings_stay_separate_and_target_counts_are_deduplicated()
-> anyhow::Result<()> {
    let out = tempfile::tempdir()?;
    let mut sdk = AuthoringSession::new(
        &id(),
        Canvas::new(256., 256., kasane_core::Vec2::new(0., 0.), 1.),
    )?;
    let params = [id(), id()];
    let targets = [id(), id()];
    sdk.edit("independent bindings", None, |edit| {
        for i in 0..2 {
            edit.create_parameter(Parameter {
                id: params[i].clone(),
                ..Default::default()
            })?;
            edit.create_part(Part {
                id: targets[i].clone(),
                ..Default::default()
            })?;
            edit.create_scene_binding(SceneBinding {
                id: id(),
                axes: vec![BindingAxis {
                    id: Default::default(),
                    samples: Default::default(),
                    parameter_id: params[i].clone(),
                    keys: vec![-1., 1.],
                }],
                track: SceneTrack::Part {
                    target_id: targets[i].clone().into(),
                    keyforms: vec![
                        PartKeyform {
                            samples: Default::default(),
                            keys: vec![-1.],
                            draw_order: 0.,
                        },
                        PartKeyform {
                            samples: Default::default(),
                            keys: vec![1.],
                            draw_order: 1.,
                        },
                    ],
                },
            })?;
        }
        Ok(())
    })?;
    let saved = sdk.save_project(&out.path().join("model"), None)?;
    let mut doc = Document::open(&saved.manifest)?;
    let index = doc.object_index();
    assert_eq!(index.bindings.len(), 2);
    for (i, p) in params.iter().enumerate() {
        let summary = index.parameter(p);
        assert_eq!(summary.direct_count(&index), 1);
        assert_eq!(
            index.binding(&summary.regular[0]).unwrap().target.id,
            targets[i]
        );
        assert_eq!(index.binding(&summary.regular[0]).unwrap().axes.len(), 1);
    }
    doc.focus_parameter(&params[0])?;
    assert!(doc.context().active_binding.is_none());
    Ok(())
}
#[test]
fn inherited_influences_are_paths_not_direct_mesh_bindings() -> anyhow::Result<()> {
    let out = tempfile::tempdir()?;
    let (mut source, _assets) = support::sample_document_with_two_layers()?;
    let saved = source.save(&out.path().join("model"))?;
    let mut sdk = AuthoringSession::new(
        &id(),
        Canvas::new(256., 256., kasane_core::Vec2::new(0., 0.), 1.),
    )?;
    sdk.open_project(&saved.manifest, None)?;
    let parent = id();
    let child = id();
    let p = id();
    let binding = id();
    let mesh = sdk.mesh_ids()[0].clone();
    sdk.edit("deformer chain", None, |edit| {
        edit.create_parameter(Parameter {
            id: p.clone(),
            ..Default::default()
        })?;
        for (r, ancestor) in [(&parent, None), (&child, Some(parent.clone().into()))] {
            edit.create_transform(Transform {
                id: r.clone(),
                parent_id: ancestor,
                data: TransformData::Rotation(RotationTransform::default()),
                ..Default::default()
            })?;
        }
        edit.set_deform_parent(&mesh, &child)?;
        edit.create_scene_binding(SceneBinding {
            id: binding.clone(),
            axes: vec![BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: p.clone(),
                keys: vec![-1., 1.],
            }],
            track: SceneTrack::Rotation {
                target_id: parent.clone().into(),
                keyforms: vec![
                    kasane_core::RotationKeyform {
                        samples: Default::default(),
                        keys: vec![-1.],
                        rotation: Default::default(),
                        appearance: Default::default(),
                    },
                    kasane_core::RotationKeyform {
                        samples: Default::default(),
                        keys: vec![1.],
                        rotation: Default::default(),
                        appearance: Default::default(),
                    },
                ],
            },
        })
    })?;
    sdk.save_project(&saved.manifest, None)?;
    let doc = Document::open(&saved.manifest)?;
    let index = doc.object_index();
    let summary = index.parameter(&p);
    let deformer = index.get(&ObjectRef::new(K::Deformer, &parent)).unwrap();
    // Inspector fields retain the actual empty name, while row labels can fall back to IDs.
    assert_eq!(deformer.properties["name"].as_str(), Some(""));
    assert_eq!(summary.direct_count(&index), 1);
    assert_eq!(summary.indirect_count(), 2);
    assert_eq!(
        index.binding(&summary.regular[0]).unwrap().target,
        ObjectRef::new(K::Deformer, &parent)
    );
    let path = summary
        .indirect
        .iter()
        .find(|p| p.target.id == mesh)
        .unwrap();
    assert_eq!(
        path.path,
        vec![
            ObjectRef::new(K::Parameter, &p),
            ObjectRef::new(K::SceneBinding, &binding),
            ObjectRef::new(K::Deformer, &parent),
            ObjectRef::new(K::Deformer, &child),
            ObjectRef::new(K::Mesh, &mesh)
        ]
    );
    assert!(
        index
            .outgoing(&ObjectRef::new(K::Mesh, &mesh))
            .any(|e| e.kind == R::DeformParent && e.target.id == child)
    );
    assert!(
        doc.mesh_tree()
            .iter()
            .any(|n| n.kind == editor_core::TreeNodeKind::Group)
    );
    Ok(())
}
#[test]
fn context_navigation_is_per_document_and_does_not_reevaluate_scene() -> anyhow::Result<()> {
    let (doc, _) = support::sample_document_with_binding(true)?;
    let binding = doc.object_index().bindings[0].reference.clone();
    let p = doc.parameters()[0].id.clone();
    let mut editor = crate::support::editor_from_document(doc);
    let original = editor.active_id().unwrap();
    editor.preview()?;
    let count = editor.evaluation_count();
    let revision = editor.tree_revision();
    editor.execute(EditorCommand::FocusParameter(p.clone()))?;
    editor.execute(EditorCommand::OpenBinding(binding.clone()))?;
    editor.preview()?;
    assert_eq!(editor.evaluation_count(), count);
    assert_eq!(editor.tree_revision(), revision);
    let other = editor.new_document()?;
    assert!(
        editor
            .document()
            .unwrap()
            .context()
            .active_binding
            .is_none()
    );
    editor.activate(original)?;
    assert_eq!(
        editor.document().unwrap().context().active_binding,
        Some(binding)
    );
    assert_eq!(
        editor.document().unwrap().context().parameter_focus,
        Some(ObjectRef::new(K::Parameter, p))
    );
    editor.activate(other)?;
    Ok(())
}
#[test]
fn scalar_property_edits_invalidate_projection_and_use_sdk_history() -> anyhow::Result<()> {
    let (mut doc, _) = support::sample_document_with_nonuniform_parameter_keys()?;
    let index = doc.object_index();
    let b = index.bindings[0].clone();
    doc.open_binding(b.reference.clone())?;
    let before = doc.history_lengths();
    doc.set_object_property(&b.target, "name", serde_json::json!("New Part"))?;
    assert_eq!(doc.object_index().name(&b.target), "New Part");
    assert!(!Arc::ptr_eq(&index, &doc.object_index()));
    assert_eq!(doc.history_lengths().0, before.0 + 1);
    doc.undo()?;
    assert_eq!(doc.context().active_binding, Some(b.reference));
    assert_eq!(doc.history_lengths().0, before.0);
    assert!(
        doc.set_object_property(&b.target, "draw_order", serde_json::json!(50000))
            .is_err()
    );
    assert_eq!(doc.history_lengths().0, before.0);
    Ok(())
}

#[test]
fn cancelled_candidates_never_reuse_stale_projection_content() -> anyhow::Result<()> {
    let (mut doc, _) = support::sample_document()?;
    let mesh = doc.context().selection.clone().unwrap();
    doc.preview_name("First candidate")?;
    let first = doc.object_index();
    assert_eq!(first.name(&mesh), "First candidate");
    doc.cancel_edit();
    doc.preview_name("Second candidate")?;
    let second = doc.object_index();
    assert_eq!(second.name(&mesh), "Second candidate");
    assert!(!Arc::ptr_eq(&first, &second));
    doc.cancel_edit();
    assert_eq!(doc.object_index().name(&mesh), "Diamond");
    Ok(())
}

#[test]
fn special_content_keeps_typed_forward_reverse_and_unresolved_references() -> anyhow::Result<()> {
    use kasane_core::{
        BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, BlendShapeTargetKind,
        DeltaKeyforms, DeltaMeshKeyform, Glue, GlueBinding, GlueKeyform, GlueVertexPair, Offscreen,
        OffscreenKeyform, ParameterKind,
    };
    let out = tempfile::tempdir()?;
    let (mut source, _assets) = support::sample_document_with_two_layers()?;
    let saved = source.save(&out.path().join("model"))?;
    let mut sdk = AuthoringSession::new(
        &id(),
        Canvas::new(256., 256., kasane_core::Vec2::new(0., 0.), 1.),
    )?;
    sdk.open_project(&saved.manifest, None)?;
    let meshes = sdk.mesh_ids().to_vec();
    let parameters = [id(), id(), id()];
    let parts = [id(), id()];
    let glue = id();
    let offscreen = id();
    let table = id();
    let unused_table = id();
    let constraint = id();
    let blend = id();
    let motion = id();
    let expression = id();
    let physics = id();
    let pose = id();
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/animation_cpu");
    let motion_text = std::fs::read_to_string(fixture.join("minimal.motion3.json"))?;
    let physics_text = std::fs::read_to_string(fixture.join("thirty.physics3.json"))?;
    sdk.edit("special content",None,|edit|{
        for (i,r) in parameters.iter().enumerate(){edit.create_parameter(Parameter{id:r.clone(),runtime_id:format!("Param{}",["X","Y","Blend"][i]),kind:if i==2{ParameterKind::BlendShape}else{ParameterKind::Normal},..Default::default()})?;}
        for (i,r) in parts.iter().enumerate(){edit.create_part(Part{id:r.clone(),runtime_id:format!("Part{i}"),..Default::default()})?;}
        edit.set_mesh_part(&meshes[0],&parts[0])?;
        edit.create_glue(Glue{id:glue.clone(),runtime_id:glue.clone(),name:"Seam".into(),mesh_a_id:(meshes[0].clone()).into(),mesh_b_id:(meshes[1].clone()).into(),intensity:0.,pairs:vec![GlueVertexPair{vertex_a:Some(0),vertex_b:Some(0),weight_a:1.,weight_b:1., ..Default::default()
}],binding:Some(GlueBinding{axes:vec![BindingAxis{ id: Default::default(), samples: Default::default(),parameter_id:parameters[0].clone(),keys:vec![-1.,1.]}],keyforms:vec![GlueKeyform{ samples: Default::default(),intensity:0.},GlueKeyform{ samples: Default::default(),intensity:1.}]})})?;
        edit.create_offscreen(Offscreen{enabled: true,id:offscreen.clone(),runtime_id:offscreen.clone(),name:"Composition".into(),part_id:parts[0].clone(),blend_mode:0,flags:0,masks:vec![meshes[1].clone()],part_keyform_indices:vec![],keyforms:vec![OffscreenKeyform{opacity:1.,..Default::default()}]})?;
        for r in [&table,&unused_table]{edit.create_blend_key_table(BlendShapeKeyTable{ samples: Default::default(), base_sample_id: Default::default(),id:r.clone(),parameter_id:parameters[2].clone(),keys:vec![-1.,1.],base_key_idx:0})?;}
        edit.create_blend_constraint(BlendShapeConstraint{ samples: Default::default(),id:constraint.clone(),parameter_id:parameters[1].clone(),keys:vec![-1.,1.],weights:vec![1.,1.]})?;
        edit.create_blend_binding(BlendShapeBinding{id:blend.clone(),target_id:meshes[0].clone(),target_kind:BlendShapeTargetKind::Mesh,key_table_id:table.clone(),constraint_ids:vec![constraint.clone()],keyforms:DeltaKeyforms::Mesh(vec![DeltaMeshKeyform{positions:vec![kasane_core::Vec2::default();4],..Default::default()};2])})?;
        edit.import_motion3(&motion,"Idle",&motion_text)?;
        edit.import_expression3(&expression,"Smile",r#"{"Type":"Live2D Expression","Parameters":[{"Id":"ParamX","Value":0.4,"Blend":"Add"},{"Id":"UnknownParameter","Value":1,"Blend":"Multiply"}]}"#)?;
        edit.import_physics3(&physics,&physics_text)?;
        edit.import_pose3(&pose,r#"{"Type":"Live2D Pose","Groups":[[{"Id":"Part0","Link":["Part1"]},{"Id":"Part1"}]]}"#)?;
        edit.set_package_attachments(vec![kasane_core::document::PackageAttachment{id:uuid::Uuid::new_v4().to_string(),path:"notes.txt".into(),bytes:Some(std::sync::Arc::new(b"Unused attachment".to_vec()))}])?;
        Ok(())
    })?;
    sdk.save_project(&saved.manifest, None)?;
    let mut doc = Document::open(&saved.manifest)?;
    let index = doc.object_index();
    let contains_edge = |source: ObjectRef, kind: R, target: ObjectRef| {
        index
            .outgoing(&source)
            .any(|e| e.kind == kind && e.target == target)
            && index
                .incoming(&target)
                .any(|e| e.kind == kind && e.source == source)
    };
    assert!(contains_edge(
        ObjectRef::new(K::Glue, &glue),
        R::GlueA,
        ObjectRef::new(K::Mesh, &meshes[0])
    ));
    assert!(contains_edge(
        ObjectRef::new(K::Glue, &glue),
        R::GlueB,
        ObjectRef::new(K::Mesh, &meshes[1])
    ));
    assert_eq!(index.parameter(&parameters[0]).indirect_count(), 2);
    assert!(contains_edge(
        ObjectRef::new(K::BlendBinding, &blend),
        R::KeyTable,
        ObjectRef::new(K::BlendKeyTable, &table)
    ));
    assert!(contains_edge(
        ObjectRef::new(K::BlendBinding, &blend),
        R::Constraint,
        ObjectRef::new(K::BlendConstraint, &constraint)
    ));
    assert!(
        index
            .incoming(&ObjectRef::new(K::BlendKeyTable, &unused_table))
            .all(|e| e.kind != R::KeyTable)
    );
    assert!(index.objects.iter().any(|i| {
        i.reference.kind == K::MotionTrack
            && index
                .outgoing(&i.reference)
                .any(|e| e.kind == R::MotionWrites && e.target.id == parameters[0])
    }));
    assert!(contains_edge(
        ObjectRef::new(K::Expression, &expression),
        R::ExpressionWrites,
        ObjectRef::new(K::Parameter, &parameters[0])
    ));
    assert!(
        index
            .relations
            .iter()
            .any(|e| e.kind == R::PhysicsInput && e.target.id == parameters[0])
    );
    assert!(
        index
            .relations
            .iter()
            .any(|e| e.kind == R::PhysicsOutput && e.target.id == parameters[1])
    );
    assert!(
        index
            .relations
            .iter()
            .any(|e| e.kind == R::PoseLink && e.target.id == parts[1])
    );
    let unknown = index
        .objects
        .iter()
        .find(|i| i.reference.kind == K::Unresolved && i.name == "UnknownParameter")
        .unwrap();
    assert!(unknown.diagnostic.is_some());
    for info in &index.objects {
        if editor_core::editable_property(info.reference.kind, "name") {
            assert!(info.properties["name"].is_string(), "{:?}", info.reference);
        }
    }
    doc.navigate(unknown.reference.clone())?;
    assert_eq!(doc.context().inspected.as_ref(), Some(&unknown.reference));
    let attachment = index
        .objects
        .iter()
        .find(|o| o.reference.kind == K::Attachment && o.name == "notes.txt")
        .unwrap()
        .reference
        .clone();
    assert!(index.get(&attachment).is_some());
    assert_eq!(index.incoming(&attachment).count(), 0);
    assert!(doc.mesh_tree().iter().any(|p| {
        p.children
            .iter()
            .any(|n| n.reference() == Some(ObjectRef::new(K::Offscreen, &offscreen)))
    }));
    Ok(())
}

#[test]
fn repeated_keyform_preview_and_unchanged_scalar_edits_are_noops() -> anyhow::Result<()> {
    let (doc, _) = support::sample_document_with_binding(true)?;
    let binding = doc.object_index().bindings[0].reference.clone();
    let mut editor = crate::support::editor_from_document(doc);
    editor.execute(EditorCommand::OpenBinding(binding.clone()))?;
    editor.execute(EditorCommand::PreviewKeyform {
        binding: binding.clone(),
        point: 1,
    })?;
    editor.preview()?;
    let evaluations = editor.evaluation_count();
    editor.execute(EditorCommand::PreviewKeyform { binding, point: 1 })?;
    editor.preview()?;
    assert_eq!(editor.evaluation_count(), evaluations);
    let mut blank = Document::new_blank()?;
    let index = blank.object_index();
    let object = index.objects[0].reference.clone();
    blank.set_object_property(
        &object,
        "export_settings.padding_pixels",
        serde_json::json!(blank.export_settings().padding_pixels),
    )?;
    assert!(!blank.modified());
    assert_eq!(blank.history_lengths(), (0, 0));
    assert!(Arc::ptr_eq(&index, &blank.object_index()));
    Ok(())
}

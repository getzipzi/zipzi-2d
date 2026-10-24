use crate::support;
use zipzi_editor_core::{
    AuthoringAction as A, AuthoringCollection as Collection, Document, Editor, ObjectKind as K,
    ObjectRef, PaletteProjection, SceneObjectType,
};
use zipzi_sdk::{CapabilityState as C, ReferenceCollection as RC, ReferenceEdit as R};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[test]
fn every_projected_object_is_inspectable_without_mutating_history_or_preview() -> anyhow::Result<()>
{
    for (d, _assets) in [
        support::combined_animation_document()?,
        support::sample_document_with_binding(true)?,
        support::sample_document_with_scene_objects()?,
        support::sample_document_with_parameter_groups()?,
    ] {
        let history = d.history_lengths();
        let preview = d.preview_revision();
        let selection = d.context().selections.clone();
        for info in &d.object_index().objects {
            let cap = d.authoring_capabilities(&info.reference)?;
            assert_eq!(cap.state(A::Inspect), C::Available, "{:?}", info.reference);
            if info.reference.kind == K::Unresolved {
                assert!(matches!(cap.state(A::Patch), C::Unsupported { .. }));
            }
        }
        assert_eq!(d.history_lengths(), history);
        assert_eq!(d.preview_revision(), preview);
        assert_eq!(d.context().selections, selection);
    }
    Ok(())
}
#[test]
fn locked_pending_and_wrong_kind_objects_keep_native_capability_distinctions() -> anyhow::Result<()>
{
    let (mut d, _assets) = support::sample_document_with_binding(true)?;
    let mesh = ObjectRef::new(K::Mesh, &d.layers()[0].id);
    let binding = d.object_index().bindings[0].reference.clone();
    d.set_objects_locked(std::slice::from_ref(&mesh), true)?;
    assert_eq!(
        d.authoring_capabilities(&mesh)?.state(A::Inspect),
        C::Available
    );
    assert_eq!(
        d.authoring_capabilities(&binding)?.state(A::Patch),
        C::Unavailable {
            code: "OBJECT_LOCKED"
        }
    );
    assert!(
        d.authoring_capabilities(&ObjectRef::new(K::Parameter, &mesh.id))
            .is_err()
    );
    d.set_objects_locked(std::slice::from_ref(&mesh), false)?;
    let p = d.plan_pose_authoring(zipzi_sdk::PoseEdit::CreateGroup {
        pose_id: id(),
        id: id(),
        name: "Empty".into(),
        before: None,
    })?;
    let r = d.execute_pose_authoring(&p)?;
    let group = r
        .suggested_selection
        .iter()
        .find(|r| r.kind == K::PoseGroup)
        .unwrap();
    assert_eq!(
        d.authoring_capabilities(group)?.state(A::Inspect),
        C::Available
    );
    assert!(matches!(
        d.authoring_capabilities(group)?.state(A::Preview),
        C::Unsupported { .. }
    ));
    let history = d.history_lengths();
    let caps = d.authoring_collection_capabilities(&Collection::Pose(
        zipzi_sdk::PoseCollection::Entries {
            group_id: group.id.clone(),
        },
    ))?;
    assert_eq!(caps.domain.create, C::Available);
    assert_eq!(d.history_lengths(), history);
    Ok(())
}
#[test]
fn business_collection_results_have_one_identity_and_location_conversion() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let group = id();
    let member = id();
    for edit in [
        R::CreateModelParameterGroup {
            id: group.clone(),
            name: "Behavior".into(),
            before: None,
        },
        R::CreateMember {
            id: member.clone(),
            collection: RC::ModelParameterMembers {
                group_id: group.clone(),
            },
            before: None,
        },
    ] {
        let plan = d.plan_reference_authoring(edit)?;
        let result = d.execute_reference_authoring(&plan)?;
        assert_eq!(result.suggested_selection.len(), 1);
        assert!(result.locations.contains(&result.suggested_selection[0]));
        assert_eq!(
            d.authoring_capabilities(&result.suggested_selection[0])?
                .state(A::Inspect),
            C::Available
        );
    }
    let p = d.plan_scene_object_creation(SceneObjectType::Part, "Part".into(), &[])?;
    let r = d.execute_scene_object_creation(&p)?;
    let part = r.suggested_selection[0].clone();
    let p = d.plan_scene_object_creation_in(
        PaletteProjection::Organization,
        SceneObjectType::Mesh,
        "Mesh".into(),
        std::slice::from_ref(&part),
    )?;
    let r = d.execute_scene_object_creation(&p)?;
    assert!(r.locations.contains(&part));
    assert!(r.locations.contains(&r.suggested_selection[0]));
    Ok(())
}
#[test]
fn inspecting_animation_children_does_not_activate_resources_or_touch_modeling_selection()
-> anyhow::Result<()> {
    let (d, _assets) = support::combined_animation_document()?;
    let modeling = d.context().selections.clone();
    let entry = d
        .object_index()
        .objects
        .iter()
        .find(|i| i.reference.kind == K::ExpressionEntry)
        .unwrap()
        .reference
        .clone();
    let mut editor = Editor::from_document(d);
    let document = editor.active_id().unwrap();
    editor.set_workspace(zipzi_editor_core::WorkspaceKind::Animation)?;
    let before = {
        let a = editor.animation().unwrap();
        (a.motion.clone(), a.expression.clone(), a.playing, a.time)
    };
    let history = editor.document().unwrap().history_lengths();
    editor.inspect_authoring_object(document, entry.clone())?;
    assert_eq!(editor.active_inspection().unwrap().object(), Some(entry));
    let after = editor.animation().unwrap();
    assert_eq!(after.motion, before.0);
    assert_eq!(after.expression, before.1);
    assert_eq!(after.playing, before.2);
    assert_eq!(after.time, before.3);
    assert_eq!(editor.document().unwrap().context().selections, modeling);
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    Ok(())
}
#[test]
fn parameter_capability_and_deletion_match_toolbar_menu_and_native_command_semantics()
-> anyhow::Result<()> {
    let assets = tempfile::tempdir()?;
    let mut session = zipzi_sdk::AuthoringSession::new(&id(), zipzi_core::Canvas::default())?;
    let group = ObjectRef::new(K::ParameterGroup, id());
    let plan = session.plan_organization_edit(zipzi_sdk::OrganizationEdit::CreateGroup {
        group: zipzi_core::document::CdiParameterGroup {
            id: group.id.clone(),
            runtime_id: "Group".into(),
            name: "Group".into(),
            parent_id: None,
            extensions: Default::default(),
        },
        before: None,
    })?;
    session.execute_organization_plan(&plan)?;
    assert!(session.document().get_parameter_group(&group.id).is_some());
    session
        .edit("Lock", None, |e| {
            let mut state = e.candidate_document().editor_state().clone();
            state.objects.entry(group.id.clone()).or_default().locked = true;
            e.replace_editor_state(state)
        })
        .map_err(|e| anyhow::anyhow!("lock stage: {e}"))?;
    let saved = session
        .save_project(&assets.path().join("locked.zipzi"), None)
        .map_err(|e| anyhow::anyhow!("save stage: {e}"))?;
    let mut d = Document::open(&saved.manifest).map_err(|e| anyhow::anyhow!("open stage: {e}"))?;
    d.navigate(group.clone())?;
    let editor = Editor::from_document(d);
    assert_eq!(
        editor.authoring_command_capability(zipzi_editor_core::CommandId::CreateParameter)?,
        C::Unavailable {
            code: "OBJECT_LOCKED"
        }
    );
    assert_eq!(
        editor.authoring_command_capability(zipzi_editor_core::CommandId::EditParameterGroup)?,
        C::Available
    );
    assert!(
        !editor.command_available(editor.command(zipzi_editor_core::CommandId::CreateParameter))
    );
    assert!(
        editor.command_available(editor.command(zipzi_editor_core::CommandId::EditParameterGroup))
    );
    Ok(())
}
#[test]
fn physics_channel_edges_and_node_references_use_native_states_and_one_history()
-> anyhow::Result<()> {
    use zipzi_editor_core::PhysicsChannelAction as P;
    let (mut d, _assets) = support::combined_animation_document()?;
    let group = d.physics_definition().unwrap().data.settings[0].id.clone();
    let input = ObjectRef::new(
        K::PhysicsInput,
        &d.physics_definition().unwrap().data.settings[0].inputs[0].id,
    );
    let invalid = P::Delete(ObjectRef::new(K::Unresolved, "missing"));
    assert!(d.physics_channel_edit(&group, &invalid).is_err());
    assert!(d.physics_channel_capability(&group, &invalid).is_err());
    let before = d.history_lengths();
    assert_eq!(
        d.physics_channel_capability(
            &group,
            &P::Step {
                object: input.clone(),
                down: false
            }
        )?,
        C::Unavailable { code: "NO_CHANGE" }
    );
    assert!(
        d.physics_channel_edit(
            &group,
            &P::Step {
                object: input,
                down: false
            }
        )?
        .is_none()
    );
    assert_eq!(d.history_lengths(), before);
    let intent = d.physics_channel_edit(&group, &P::CreateInput)?.unwrap();
    let p = d.plan_physics_authoring(intent)?;
    let result = d.execute_physics_authoring(&p)?;
    let created = result
        .suggested_selection
        .iter()
        .find(|r| r.kind == K::PhysicsInput)
        .unwrap()
        .clone();
    assert!(result.locations.contains(&created));
    assert_eq!(d.history_lengths(), (before.0 + 1, 0));
    let plan = d.plan_physics_authoring(
        d.physics_channel_edit(
            &group,
            &P::Step {
                object: created.clone(),
                down: false,
            },
        )?
        .unwrap(),
    )?;
    d.execute_physics_authoring(&plan)?;
    assert_eq!(
        d.physics_definition().unwrap().data.settings[0].inputs[0].id,
        created.id
    );
    d.undo()?;
    d.undo()?;
    assert_eq!(d.history_lengths(), (before.0, 2));
    Ok(())
}

#[test]
fn candidate_state_is_separate_from_native_domain_and_queries_preserve_it() -> anyhow::Result<()> {
    let (mut d, _assets) = support::combined_animation_document()?;
    let entry = d
        .object_index()
        .objects
        .iter()
        .find(|i| i.reference.kind == K::ExpressionEntry)
        .unwrap()
        .reference
        .clone();
    let address = d.expression_entry_address(&entry.id)?;
    let _token = d.begin_expression_entry_edit(&address)?;
    let history = d.history_lengths();
    let capabilities = d.authoring_capabilities(&entry)?;
    assert_eq!(
        capabilities.session_reason,
        Some(zipzi_editor_core::AuthoringSessionReason::CandidateActive)
    );
    assert_eq!(capabilities.state(A::Inspect), C::Available);
    assert_eq!(capabilities.state(A::Patch), C::Available);
    assert!(d.has_pending_edit());
    assert_eq!(d.history_lengths(), history);
    d.cancel_edit();
    assert!(d.authoring_capabilities(&entry)?.session_reason.is_none());
    Ok(())
}

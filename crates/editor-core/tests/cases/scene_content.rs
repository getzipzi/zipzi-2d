use editor_core::{Document, ObjectKind, ObjectRef};
use kasane_sdk::{
    SceneContentEdit as E, SceneContentTarget as T, SceneContentValue as V, ShapeTarget,
};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
#[test]
fn mesh_content_receipt_refreshes_canvas_and_keeps_retained_snapshot() -> anyhow::Result<()> {
    let (mut d, _assets) = super::support::sample_document_with_two_layers()?;
    let mesh = d.mesh_tree()[0].children[0].id.clone();
    let vertex = 0;
    let before = d.snapshot()?;
    let selections = d.context().selections.clone();
    let address = d.scene_content_address(
        &mesh,
        T::VertexPosition {
            vertex_id: vertex,
            shape: ShapeTarget::Base,
        },
    )?;
    let plan = d.plan_scene_content_patch(&address, V::Point(kasane_core::Vec2::new(17., 31.)))?;
    let r = d.execute_scene_content(&plan)?;
    assert!(r.receipt.changed);
    assert!(r.suggested_selection.is_empty());
    assert_eq!(d.context().selections, selections);
    let after = d.snapshot()?;
    assert_ne!(
        before.viewport_frame.drawables,
        after.viewport_frame.drawables
    );
    assert_eq!(
        *match d
            .scene_content_address(
                &mesh,
                T::VertexPosition {
                    vertex_id: vertex,
                    shape: ShapeTarget::Base
                }
            )?
            .expected()
        {
            V::Point(point) => point,
            _ => unreachable!(),
        },
        kasane_core::Vec2::new(17., 31.)
    );
    let uv = d.scene_content_address(&mesh, T::VertexUv { vertex_id: vertex })?;
    let plan = d.plan_scene_content_patch(&uv, V::Point(kasane_core::Vec2::new(0.3, 0.7)))?;
    d.execute_scene_content(&plan)?;
    let updated = d.snapshot()?;
    assert_ne!(
        after.viewport_frame.drawables[0].uvs,
        updated.viewport_frame.drawables[0].uvs
    );
    d.undo()?;
    d.undo()?;
    assert_eq!(
        d.snapshot()?.viewport_frame.drawables,
        before.viewport_frame.drawables
    );
    assert!(
        d.plan_scene_content_patch(&address, address.expected().clone())
            .is_err()
    );
    Ok(())
}
#[test]
fn pair_child_projection_and_history_share_the_native_lifecycle() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let glue = id();
    let p = d.plan_scene_authoring(kasane_sdk::SceneEdit::Create {
        id: glue.clone(),
        name: "Glue".into(),
        object: kasane_sdk::SceneCreate::Glue {
            a: Default::default(),
            b: Default::default(),
        },
        part_id: None,
        parent_id: None,
        before: None,
        deformation_before: None,
    })?;
    d.execute_scene_authoring(&p)?;
    let pair = kasane_core::GlueVertexPair::default();
    let pair_id = pair.id.clone();
    let p = d.plan_scene_content(E::InsertGluePair {
        glue_id: glue.clone(),
        pair,
        before: None,
    })?;
    d.execute_scene_content(&p)?;
    let object = ObjectRef::new(ObjectKind::GlueVertexPair, &pair_id);
    let index = d.object_index();
    assert!(index.get(&object).unwrap().diagnostic.is_some());
    let a = d.scene_content_address(
        &glue,
        T::GluePair {
            pair_id: pair_id.clone(),
        },
    )?;
    let p = d.plan_scene_content(E::DeleteGluePairs {
        glue_id: glue.clone(),
        ids: vec![pair_id],
    })?;
    d.execute_scene_content(&p)?;
    assert!(d.object_index().get(&object).is_none());
    d.undo()?;
    assert!(d.object_index().get(&object).is_some());
    assert!(
        d.plan_scene_content_patch(&a, a.expected().clone())
            .is_err()
    );
    let root = tempfile::tempdir()?;
    let saved = d.save(&root.path().join("pairs.kasane"))?;
    let reopened = Document::open(&saved.manifest)?;
    assert!(reopened.object_index().get(&object).is_some());
    Ok(())
}
#[test]
fn warp_resize_and_offscreen_content_use_one_history_step_each() -> anyhow::Result<()> {
    let mut d = Document::new_blank()?;
    let warp = id();
    let part = id();
    let os = id();
    let p = d.plan_scene_authoring(kasane_sdk::SceneEdit::Create {
        id: warp.clone(),
        name: "Warp".into(),
        object: kasane_sdk::SceneCreate::Warp {
            bounds: Some([0., 0., 10., 10.]),
        },
        part_id: None,
        parent_id: None,
        before: None,
        deformation_before: None,
    })?;
    d.execute_scene_authoring(&p)?;
    let p = d.plan_organization_authoring(kasane_sdk::OrganizationEdit::CreatePart {
        part: kasane_core::Part {
            id: part.clone(),
            ..Default::default()
        },
        before: None,
    })?;
    d.execute_organization_authoring(&p)?;
    let p = d.plan_scene_authoring(kasane_sdk::SceneEdit::Create {
        id: os.clone(),
        name: "Offscreen".into(),
        object: kasane_sdk::SceneCreate::Offscreen,
        part_id: Some(part),
        parent_id: None,
        before: None,
        deformation_before: None,
    })?;
    d.execute_scene_authoring(&p)?;
    let before = d.history_lengths();
    let p = d.plan_scene_content(E::ResizeWarp {
        transform_id: warp.clone(),
        rows: 2,
        columns: 3,
        quad: true,
    })?;
    d.execute_scene_content(&p)?;
    assert_eq!(d.history_lengths().0, before.0 + 1);
    assert_eq!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::Deformer, &warp))
            .unwrap()
            .properties["warp"]["points"]
            .as_array()
            .unwrap()
            .len(),
        12
    );
    let p = d.plan_scene_content(E::InsertOffscreenKeyform {
        offscreen_id: os.clone(),
        value: kasane_core::OffscreenKeyform {
            opacity: 0.4,
            ..Default::default()
        },
        before: Some(0),
    })?;
    d.execute_scene_content(&p)?;
    assert_eq!(d.history_lengths().0, before.0 + 2);
    d.undo()?;
    d.undo()?;
    assert_eq!(
        d.object_index()
            .get(&ObjectRef::new(ObjectKind::Deformer, &warp))
            .unwrap()
            .properties["warp"]["points"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    Ok(())
}

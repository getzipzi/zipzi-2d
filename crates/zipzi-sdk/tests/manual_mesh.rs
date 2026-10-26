use zipzi_core::{
    Mesh, MeshConstraint, MeshConstraintKind, MeshMeshing, ModelSetup, Vec2, document::*,
};
use zipzi_sdk::{AuthoringSession, SceneContentEdit, TopologyReplacement};

fn id(n: u32) -> String {
    format!("a0000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    let mut s = AuthoringSession::new(&id(1), ModelSetup::default()).unwrap();
    s.edit("mesh", None, |e| {
        e.create_mesh(Mesh {
            id: id(2),
            vertex_ids: vec![10, 20, 30, 40],
            base_positions: vec![
                Vec2::new(0., 0.),
                Vec2::new(-2., 0.),
                Vec2::new(-2., 2.),
                Vec2::new(0., 2.),
            ],
            uvs: vec![
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(0., 1.),
            ],
            triangles: vec![[10, 20, 30], [10, 30, 40]],
            meshing: Some(MeshMeshing {
                boundary: vec![10, 20, 30, 40],
                constraints: vec![],
            }),
            ..Default::default()
        })
    })
    .unwrap();
    s.drain_events();
    s
}

#[test]
fn constraint_only_commit_notifies_and_restores_exact_geometry_without_evaluation() {
    let mut s = session();
    let before = s.mesh(&id(2)).unwrap();
    let eval = s.evaluation_revision();
    let mut mesh = before.clone();
    mesh.meshing
        .as_mut()
        .unwrap()
        .constraints
        .push(MeshConstraint {
            endpoints: [10, 30],
            kind: MeshConstraintKind::EdgeSupport,
        });
    let (_, receipt) = s
        .edit("fix automatic edge", None, |e| e.replace_mesh(mesh.clone()))
        .unwrap();
    assert!(receipt.changed);
    assert_eq!(s.history_lengths(), (2, 0));
    assert_eq!(s.evaluation_revision(), eval);
    assert!(
        !receipt
            .changes
            .categories
            .contains(&zipzi_sdk::ChangeCategory::EvaluationInputs)
    );
    assert!(
        receipt
            .changes
            .content_changes
            .iter()
            .any(|c| c.entity.id == id(2)
                && c.domain == SceneContentDomain::MeshTopology
                && c.topology)
    );
    assert_eq!(s.geometry(&id(2)).unwrap().meshing, mesh.meshing);
    s.undo().unwrap();
    assert_eq!(s.mesh(&id(2)).unwrap(), before);
    assert_eq!(s.evaluation_revision(), eval);
    s.redo().unwrap();
    assert_eq!(s.mesh(&id(2)).unwrap(), mesh);
    assert_eq!(s.evaluation_revision(), eval);
    let mut user = mesh.clone();
    user.meshing.as_mut().unwrap().constraints[0].kind = MeshConstraintKind::User;
    let (_, receipt) = s
        .edit("promote support", None, |e| e.replace_mesh(user.clone()))
        .unwrap();
    assert!(receipt.changed);
    assert!(receipt.changes.content_changes.iter().any(|c| c.topology));
    assert_eq!(s.evaluation_revision(), eval);
    s.undo().unwrap();
    let history = s.history_state();
    let version = s.version();
    assert!(
        !s.edit("unchanged", None, |e| e.replace_mesh(mesh))
            .unwrap()
            .1
            .changed
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
}

#[test]
fn old_writes_preserve_or_reject_structure_atomically() {
    let mut s = session();
    let structure = s.mesh(&id(2)).unwrap().meshing;
    s.edit("pose and label", None, |e| {
        e.update_positions(&id(2), &[10], &[Vec2::new(100., 100.)])?;
        e.rename_mesh(&id(2), "renamed")
    })
    .unwrap();
    assert_eq!(s.mesh(&id(2)).unwrap().meshing, structure);
    s.undo().unwrap();
    let before = s.mesh(&id(2)).unwrap();
    let version = s.version();
    let history = s.history_state();
    let mut clear = before.clone();
    clear.meshing = None;
    assert!(
        s.edit("accidental clear", None, |e| e.replace_mesh(clear))
            .is_err()
    );
    for operation in [
        SceneContentEdit::MeshVertices {
            mesh_id: id(2),
            target: ShapeTarget::Base,
            updates: vec![VertexContentPatch {
                vertex_id: 10,
                position: None,
                uv: Some(Vec2::new(1., 1.)),
            }],
        },
        SceneContentEdit::DeleteTriangles {
            mesh_id: id(2),
            slots: vec![0],
        },
        SceneContentEdit::DeleteMeshVertices {
            mesh_id: id(2),
            vertices: vec![10],
            policy: VertexDeletePolicy::RemoveDependentContent,
        },
    ] {
        assert!(
            s.edit("invalid raw edit", None, |e| e
                .edit_scene_content(operation))
                .is_err()
        );
        assert_eq!(s.mesh(&id(2)).unwrap(), before);
        assert_eq!(s.version(), version);
        assert_eq!(s.history_state(), history);
    }
    s.edit("reorder", None, |e| {
        e.edit_scene_content(SceneContentEdit::ReorderMeshVertices {
            mesh_id: id(2),
            order: vec![40, 20, 10, 30],
        })
    })
    .unwrap();
    assert_eq!(s.mesh(&id(2)).unwrap().meshing, structure);
    assert!(s.document().validate_structure().is_empty());
}

#[test]
fn complete_replacement_explicitly_discards_structure_and_undo_restores_it() {
    let mut s = session();
    let source = s.geometry(&id(2)).unwrap();
    let before = s.mesh(&id(2)).unwrap();
    let mut mesh = before.clone();
    mesh.meshing = None;
    s.edit("replace all geometry", None, |e| {
        e.replace_topology(
            &source,
            TopologyReplacement {
                mesh,
                binding: None,
                blend_bindings: vec![],
                glues: vec![],
                vertex_mapping: source.vertex_ids.iter().map(|&id| (id, Some(id))).collect(),
            },
        )
    })
    .unwrap();
    assert!(s.mesh(&id(2)).unwrap().meshing.is_none());
    s.undo().unwrap();
    assert_eq!(s.mesh(&id(2)).unwrap(), before);
}

#[test]
fn candidate_snapshots_keep_structure_and_failed_creation_has_no_history() {
    let mut s = session();
    let mut mesh = s.mesh(&id(2)).unwrap();
    mesh.meshing
        .as_mut()
        .unwrap()
        .constraints
        .push(MeshConstraint {
            endpoints: [10, 30],
            kind: MeshConstraintKind::User,
        });
    let mut edit = s.begin_owned_edit("candidate", None).unwrap();
    edit.replace_mesh(mesh.clone()).unwrap();
    let snapshot = s.candidate_snapshot(&edit).unwrap();
    assert_eq!(snapshot.document().get_mesh(&id(2)).unwrap(), &mesh);
    drop(edit);
    assert!(
        s.mesh(&id(2))
            .unwrap()
            .meshing
            .unwrap()
            .constraints
            .is_empty()
    );
    let version = s.version();
    mesh.id = id(3);
    mesh.triangles.clear();
    assert!(
        s.edit("invalid create", None, |e| e.create_mesh(mesh))
            .is_err()
    );
    assert_eq!(s.version(), version);
}

mod support;

use support::{image, prepare};
use zipzi_core::{
    document::{SceneContentEdit, ShapeTarget, VertexContentPatch},
    evaluation::EvaluationModel,
    state::{DocumentBuilder, DocumentState, RecordChange, RecordDomain, RecordWrite, RootDomain},
    Canvas, Mesh, ModelSetup, Status, Vec2,
};

const DOC: &str = "11111111-1111-4111-8111-111111111111";
const ASSET: &str = "22222222-2222-4222-8222-222222222222";
const MESH: &str = "33333333-3333-4333-8333-333333333333";
const SECOND: &str = "44444444-4444-4444-8444-444444444444";

fn setup() -> ModelSetup {
    Canvas::new(100., 100., Vec2::default(), 1.).into()
}

fn textured_model() -> DocumentState {
    let state = DocumentState::new(DOC, setup()).unwrap();
    prepare(&state, |builder| {
        builder.create_asset(image(ASSET.into(), "checker", 32, 32))
    })
    .unwrap()
    .state
}

fn move_vertices(builder: &mut DocumentBuilder, updates: &[(u32, Vec2)]) -> Result<(), Status> {
    builder.edit_scene_content(
        SceneContentEdit::MeshVertices {
            mesh_id: MESH.into(),
            target: ShapeTarget::Base,
            updates: updates
                .iter()
                .map(|&(vertex_id, position)| VertexContentPatch {
                    vertex_id,
                    position: Some(position),
                    uv: None,
                })
                .collect(),
        },
        Default::default(),
        &Default::default(),
    )?;
    Ok(())
}

#[test]
fn attachment_edits_create_a_new_root_and_coalesce_when_reverted_in_one_candidate() {
    let base = DocumentState::new(DOC, setup()).unwrap();
    let added = prepare(&base, |b| {
        b.set_missing_attachments(vec!["attachment.json".into()])
    })
    .unwrap();
    assert!(added.writes.root_changed(RootDomain::MissingAttachments));
    assert_ne!(added.state.content_id(), base.content_id());
    assert!(base.missing_attachments().is_empty());

    let reverted = prepare(&base, |b| {
        b.set_missing_attachments(vec!["attachment.json".into()])?;
        b.set_missing_attachments(Vec::new())
    })
    .unwrap();
    assert!(reverted.writes.is_empty());
    assert_eq!(reverted.state.content_id(), base.content_id());
}

fn sample() -> Mesh {
    Mesh {
        id: MESH.to_string(),
        name: "quad".to_string(),
        runtime_id: "Quad".into(),
        texture_asset_id: (ASSET.to_string()).into(),
        vertex_ids: vec![40, 10, 90, 20],
        base_positions: vec![
            Vec2::new(-10.0, 10.0),
            Vec2::new(-10.0, -10.0),
            Vec2::new(10.0, -10.0),
            Vec2::new(10.0, 10.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(0.0, 1.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(1.0, 0.0),
        ],
        triangles: vec![[40, 10, 90], [40, 90, 20]],
        ..Default::default()
    }
}

#[test]
fn mesh_runtime_ids_remain_unique_in_each_frozen_snapshot() {
    let mut first = sample();
    first.runtime_id = "shared".into();
    let base = prepare(&textured_model(), |b| b.create_mesh(first))
        .unwrap()
        .state;
    let mut second = sample();
    second.id = SECOND.into();
    second.runtime_id = "shared".into();
    assert_eq!(
        prepare(&base, |b| b.create_mesh(second.clone()))
            .unwrap_err()
            .code,
        "DUPLICATE_RUNTIME_ID"
    );

    let mut renamed = base.mesh(MESH).unwrap().clone();
    renamed.runtime_id = "renamed".into();
    let old = prepare(&base, |b| {
        b.replace_mesh(renamed)?;
        b.create_mesh(second.clone())
    })
    .unwrap()
    .state;
    let mut renamed_again = old.mesh(MESH).unwrap().clone();
    renamed_again.runtime_id = "newer".into();
    let current = prepare(&old, |b| b.replace_mesh(renamed_again))
        .unwrap()
        .state;
    assert_eq!(current.mesh(MESH).unwrap().runtime_id, "newer");
    assert_eq!(old.mesh(MESH).unwrap().runtime_id, "renamed");
    let mut conflict = old.mesh(MESH).unwrap().clone();
    conflict.runtime_id = "shared".into();
    assert_eq!(
        prepare(&old, |b| b.replace_mesh(conflict))
            .unwrap_err()
            .code,
        "DUPLICATE_RUNTIME_ID"
    );

    let removed = prepare(&old, |b| b.remove_mesh(SECOND)).unwrap().state;
    assert!(removed.mesh(SECOND).is_none());
    let reinserted = prepare(&removed, |b| b.create_mesh(second.clone()))
        .unwrap()
        .state;
    assert_eq!(reinserted.mesh(SECOND), Some(&second));
}

#[test]
fn mesh_validation_and_topology_are_checked_before_freezing() {
    assert!(DocumentState::new("not-a-uuid", setup()).is_err());
    assert!(DocumentState::new(DOC, Canvas::new(-1., 100., Vec2::default(), 1.).into()).is_err());
    let empty = DocumentState::new(DOC, setup()).unwrap();
    assert_eq!(
        prepare(&empty, |b| b.create_mesh(sample()))
            .unwrap_err()
            .code,
        "MISSING_ASSET"
    );
    assert_eq!(
        prepare(&empty, |b| b.create_asset(image(DOC.into(), "bad", 32, 32)))
            .unwrap_err()
            .code,
        "DUPLICATE_ID"
    );

    let base = textured_model();
    let mut invalid = sample();
    invalid.vertex_ids[1] = 40;
    assert_eq!(
        prepare(&base, |b| b.create_mesh(invalid)).unwrap_err().code,
        "DUPLICATE_VERTEX"
    );
    let mut invalid = sample();
    invalid.triangles[0][0] = 99;
    assert_eq!(
        prepare(&base, |b| b.create_mesh(invalid)).unwrap_err().code,
        "MISSING_VERTEX"
    );
    let mut invalid = sample();
    invalid.triangles[0][0] = 10;
    assert_eq!(
        prepare(&base, |b| b.create_mesh(invalid)).unwrap_err().code,
        "REPEATED_VERTEX"
    );
    let mut invalid = sample();
    invalid.base_positions[0].x = f32::INFINITY;
    assert_eq!(
        prepare(&base, |b| b.create_mesh(invalid)).unwrap_err().code,
        "NON_FINITE"
    );
    assert!(base.mesh_order().is_empty());

    let mut source = sample();
    let created = prepare(&base, |b| b.create_mesh(source.clone())).unwrap();
    assert_eq!(
        created.writes.records(),
        &[RecordWrite {
            domain: RecordDomain::Mesh,
            id: MESH.into(),
            change: RecordChange::Created,
        }]
    );
    let state = created.state;
    source.base_positions[0].x = 999.;
    assert_eq!(state.mesh(MESH).unwrap().base_positions[0].x, -10.);
    assert!(prepare(&state, |b| b.create_mesh(sample())).is_err());

    let mut indices = Vec::new();
    state.render_indices_into(MESH, &mut indices).unwrap();
    assert_eq!(indices, [0, 1, 2, 0, 2, 3]);
    assert!(state.render_indices_into("missing", &mut indices).is_err());

    let mut collapsed = sample();
    collapsed.id = SECOND.into();
    collapsed.runtime_id = "Collapsed".into();
    collapsed.base_positions = vec![Vec2::default(); 4];
    collapsed.uvs[0] = Vec2::new(-1., 2.);
    let state = prepare(&state, |b| b.create_mesh(collapsed.clone()))
        .unwrap()
        .state;
    assert_eq!(state.mesh_order(), [MESH, SECOND]);
    assert_eq!(state.mesh(SECOND), Some(&collapsed));
}

#[test]
fn stable_vertex_updates_and_mixed_record_edits_are_atomic() {
    let state = prepare(&textured_model(), |b| b.create_mesh(sample()))
        .unwrap()
        .state;
    let before = state.mesh(MESH).unwrap();
    for (updates, code) in [
        (
            vec![(40, Vec2::default()), (999, Vec2::default())],
            "MISSING_VERTEX",
        ),
        (
            vec![(40, Vec2::default()), (40, Vec2::default())],
            "DUPLICATE_SELECTION",
        ),
        (vec![(40, Vec2::new(f32::NAN, 0.))], "INVALID_POSITION"),
    ] {
        let mut candidate = state.builder();
        move_vertices(&mut candidate, &[(10, Vec2::new(15., 12.))]).unwrap();
        assert_eq!(
            move_vertices(&mut candidate, &updates).unwrap_err().code,
            code
        );
        assert_eq!(candidate.freeze().unwrap_err().code, code);
        assert_eq!(
            state.mesh(MESH).unwrap().base_positions,
            sample().base_positions
        );
    }
    {
        let mut abandoned = state.builder();
        move_vertices(&mut abandoned, &[(40, Vec2::new(100., 100.))]).unwrap();
    }
    assert_eq!(
        state.mesh(MESH).unwrap().base_positions,
        before.base_positions
    );

    let selected = [(20, Vec2::new(3., 4.)), (10, Vec2::new(5., 6.))];
    let mixed = prepare(&state, |b| {
        move_vertices(b, &selected)?;
        move_vertices(b, &[(40, Vec2::new(-15., 12.))])?;
        let mut mesh = b.mesh(MESH).unwrap().clone();
        mesh.name = "renamed".into();
        b.replace_mesh(mesh)
    })
    .unwrap();
    assert_eq!(
        mixed.writes.records(),
        &[RecordWrite {
            domain: RecordDomain::Mesh,
            id: MESH.into(),
            change: RecordChange::Replaced,
        }]
    );
    let edited = mixed.state;
    let mesh = edited.mesh(MESH).unwrap();
    assert_eq!(mesh.vertex_ids, before.vertex_ids);
    assert_eq!(mesh.name, "renamed");
    assert_eq!(
        mesh.base_positions,
        [
            Vec2::new(-15., 12.),
            selected[1].1,
            before.base_positions[2],
            selected[0].1
        ]
    );
    let unchanged = prepare(&edited, |b| move_vertices(b, &selected)).unwrap();
    assert_eq!(unchanged.state.content_id(), edited.content_id());
    assert!(unchanged.writes.is_empty());

    let mut replacement = mesh.clone();
    replacement.uvs[0] = Vec2::new(0.25, 0.5);
    let with_uv = prepare(&edited, |b| b.replace_mesh(replacement.clone()))
        .unwrap()
        .state;
    assert_eq!(with_uv.mesh(MESH).unwrap().uvs[0], Vec2::new(0.25, 0.5));
    replacement.triangles[0][0] = 999;
    assert!(prepare(&with_uv, |b| b.replace_mesh(replacement)).is_err());
    assert_eq!(with_uv.mesh(MESH).unwrap().triangles, before.triangles);
    assert_eq!(state.mesh(MESH), Some(&sample()));
}

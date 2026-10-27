use zipzi_core::{
    document::*, Mesh, MeshConstraint, MeshConstraintKind, MeshMeshing, ModelSetup, Vec2,
};
use zipzi_sdk::{
    add_manual_interior_sites, edit_manual_mesh, extract_manual_meshing, AffineUvMap,
    AuthoringSession, ManualMeshOperation, ManualVertexIdAllocator, ManualVertexTarget,
    MeshGeometry, SceneContentEdit, TopologyReplacement,
};

fn target(id: u32, uv: Vec2) -> ManualVertexTarget {
    ManualVertexTarget { id, uv }
}

fn square_geometry() -> MeshGeometry {
    MeshGeometry {
        vertex_ids: vec![10, 20, 30, 40],
        positions: vec![
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
        meshing: None,
    }
}

#[test]
fn interior_sampling_keeps_existing_vertices_and_structural_edges() {
    let mut original = square_geometry();
    original.meshing = Some(extract_manual_meshing(&original).unwrap());
    original
        .meshing
        .as_mut()
        .unwrap()
        .constraints
        .push(zipzi_core::MeshConstraint {
            endpoints: [10, 30],
            kind: zipzi_core::MeshConstraintKind::User,
        });
    let mut ids = ManualVertexIdAllocator::new(&original);
    let result = add_manual_interior_sites(
        &original,
        &[
            Vec2::new(0.2, 0.3),
            Vec2::new(0.8, 0.3),
            Vec2::new(0.5, 0.5),
        ],
        &mut ids,
    )
    .unwrap();
    assert_eq!(result.added.len(), 2);
    assert!(result.geometry.vertex_ids.starts_with(&original.vertex_ids));
    assert_eq!(result.geometry.meshing, original.meshing);
    assert!(result.changed);
}

#[test]
fn extract_and_insert_preserve_existing_faces_and_split_structure() {
    let mut g = square_geometry();
    assert_eq!(
        extract_manual_meshing(&g).unwrap().boundary,
        vec![10, 20, 30, 40]
    );
    g.meshing = Some(extract_manual_meshing(&g).unwrap());
    g.meshing
        .as_mut()
        .unwrap()
        .constraints
        .push(MeshConstraint {
            endpoints: [10, 30],
            kind: MeshConstraintKind::EdgeSupport,
        });
    let result = edit_manual_mesh(
        &g,
        ManualMeshOperation::InsertVertex {
            id: 50,
            uv: Vec2::new(0.5, 0.5),
        },
    )
    .unwrap();
    assert_eq!(result.added, vec![50]);
    assert_eq!(result.geometry.positions[4], Vec2::new(-1., 1.));
    assert_eq!(result.geometry.triangles.len(), 4);
    assert_eq!(
        result
            .geometry
            .meshing
            .as_ref()
            .unwrap()
            .constraints
            .iter()
            .map(|c| c.endpoints)
            .collect::<Vec<_>>(),
        vec![[10, 50], [30, 50]]
    );
    assert_eq!(
        result.vertex_mapping,
        vec![
            (10, Some(10)),
            (20, Some(20)),
            (30, Some(30)),
            (40, Some(40))
        ]
    );
    let corner = edit_manual_mesh(
        &square_geometry(),
        ManualMeshOperation::InsertVertex {
            id: 60,
            uv: Vec2::new(0.5, 0.),
        },
    )
    .unwrap();
    assert_eq!(
        corner.geometry.meshing.unwrap().boundary,
        vec![10, 60, 20, 30, 40]
    );
}

#[test]
fn interior_insert_delete_fan_and_batch_delete_are_atomic() {
    let base = square_geometry();
    let inserted = edit_manual_mesh(
        &base,
        ManualMeshOperation::InsertVertex {
            id: 50,
            uv: Vec2::new(0.75, 0.25),
        },
    )
    .unwrap()
    .geometry;
    assert_eq!(inserted.triangles.len(), 4);
    let deleted = edit_manual_mesh(
        &inserted,
        ManualMeshOperation::DeleteVertices { ids: vec![50] },
    )
    .unwrap();
    assert_eq!(deleted.removed, vec![50]);
    assert_eq!(deleted.geometry.triangles.len(), 2);
    assert_eq!(deleted.vertex_mapping.last(), Some(&(50, None)));
    let a = edit_manual_mesh(
        &inserted,
        ManualMeshOperation::DeleteVertices { ids: vec![50, 20] },
    );
    let b = edit_manual_mesh(
        &inserted,
        ManualMeshOperation::DeleteVertices { ids: vec![20, 50] },
    );
    assert_eq!(a.is_ok(), b.is_ok());
    if let (Ok(a), Ok(b)) = (a, b) {
        assert_eq!(a.geometry, b.geometry);
    }
    assert!(edit_manual_mesh(
        &base,
        ManualMeshOperation::DeleteVertices { ids: vec![10, 20] }
    )
    .is_err());
    assert_eq!(base, square_geometry());
}

#[test]
fn constrain_reconnect_release_and_reject_crossing() {
    let base = square_geometry();
    let diagonal =
        edit_manual_mesh(&base, ManualMeshOperation::ConstrainEdge { a: 20, b: 40 }).unwrap();
    assert_eq!(
        diagonal.geometry,
        edit_manual_mesh(&base, ManualMeshOperation::ConstrainEdge { a: 40, b: 20 })
            .unwrap()
            .geometry
    );
    assert_eq!(
        diagonal.geometry.meshing.as_ref().unwrap().constraints[0].endpoints,
        [20, 40]
    );
    assert!(diagonal
        .geometry
        .triangles
        .iter()
        .any(|t| t.contains(&20) && t.contains(&40)));
    assert!(edit_manual_mesh(
        &diagonal.geometry,
        ManualMeshOperation::ConstrainEdge { a: 10, b: 30 }
    )
    .is_err());
    let released = edit_manual_mesh(
        &diagonal.geometry,
        ManualMeshOperation::ReleaseConstraint { a: 20, b: 40 },
    )
    .unwrap();
    assert_eq!(released.geometry.triangles, diagonal.geometry.triangles);
    assert!(released.affected_edges.contains(&[20, 40]));
    assert!(released.geometry.meshing.unwrap().constraints.is_empty());
    let reconnected = edit_manual_mesh(&diagonal.geometry, ManualMeshOperation::Reconnect).unwrap();
    assert_eq!(reconnected.geometry.meshing, diagonal.geometry.meshing);
    assert!(reconnected
        .geometry
        .triangles
        .iter()
        .any(|t| t.contains(&20) && t.contains(&40)));
}

#[test]
fn move_reflected_mapping_releases_generated_support_and_rejects_invalid_target() {
    let mut g = square_geometry();
    g.meshing = Some(extract_manual_meshing(&g).unwrap());
    g.meshing
        .as_mut()
        .unwrap()
        .constraints
        .push(MeshConstraint {
            endpoints: [10, 30],
            kind: MeshConstraintKind::EdgeSupport,
        });
    let map = AffineUvMap::from_geometry(&g).unwrap();
    assert_eq!(map.position(Vec2::new(0.25, 0.75)), Vec2::new(-0.5, 1.5));
    assert_eq!(map.uv(Vec2::new(-0.5, 1.5)).unwrap(), Vec2::new(0.25, 0.75));
    let moved = edit_manual_mesh(
        &g,
        ManualMeshOperation::MoveVertices {
            targets: vec![target(10, Vec2::new(0.1, 0.1))],
            motion_from: None,
        },
    )
    .unwrap();
    assert!(moved.geometry.meshing.unwrap().constraints.is_empty());
    assert_eq!(moved.geometry.positions[0], Vec2::new(-0.2, 0.2));
    assert!(moved.affected_edges.contains(&[10, 30]));
    assert!(edit_manual_mesh(
        &g,
        ManualMeshOperation::MoveVertices {
            targets: vec![target(10, Vec2::new(1., 1.))],
            motion_from: None
        }
    )
    .is_err());
    assert_eq!(
        g.meshing.unwrap().constraints[0].kind,
        MeshConstraintKind::EdgeSupport
    );
}

#[test]
fn boundary_delete_and_chained_constraint_keep_single_domain() {
    let base = square_geometry();
    let shortened =
        edit_manual_mesh(&base, ManualMeshOperation::DeleteVertices { ids: vec![20] }).unwrap();
    assert_eq!(
        shortened.geometry.meshing.unwrap().boundary,
        vec![10, 30, 40]
    );
    assert_eq!(shortened.geometry.triangles.len(), 1);

    let center = edit_manual_mesh(
        &base,
        ManualMeshOperation::InsertVertex {
            id: 50,
            uv: Vec2::new(0.5, 0.5),
        },
    )
    .unwrap()
    .geometry;
    let fixed =
        edit_manual_mesh(&center, ManualMeshOperation::ConstrainEdge { a: 20, b: 40 }).unwrap();
    assert_eq!(
        fixed.geometry.meshing.unwrap().constraints,
        vec![
            MeshConstraint {
                endpoints: [20, 50],
                kind: MeshConstraintKind::User,
            },
            MeshConstraint {
                endpoints: [40, 50],
                kind: MeshConstraintKind::User,
            },
        ]
    );
}

fn with_sites(mut geometry: MeshGeometry, sites: &[(u32, f32, f32)]) -> MeshGeometry {
    for &(id, x, y) in sites {
        geometry = edit_manual_mesh(
            &geometry,
            ManualMeshOperation::InsertVertex {
                id,
                uv: Vec2::new(x, y),
            },
        )
        .unwrap()
        .geometry;
    }
    geometry
}

#[test]
fn free_vertex_crosses_old_triangles_and_generated_support() {
    let mut g = with_sites(square_geometry(), &[(50, 0.2, 0.3), (60, 0.7, 0.6)]);
    g = edit_manual_mesh(&g, ManualMeshOperation::ConstrainEdge { a: 10, b: 30 })
        .unwrap()
        .geometry;
    g.meshing.as_mut().unwrap().constraints[0].kind = MeshConstraintKind::EdgeSupport;
    let moved = edit_manual_mesh(
        &g,
        ManualMeshOperation::MoveVertices {
            targets: vec![target(50, Vec2::new(0.85, 0.25))],
            motion_from: None,
        },
    )
    .unwrap()
    .geometry;
    assert_eq!(
        moved.uvs[moved.vertex_ids.iter().position(|id| *id == 50).unwrap()],
        Vec2::new(0.85, 0.25)
    );
    assert!(moved.meshing.as_ref().unwrap().constraints.is_empty());
    assert_eq!(moved.vertex_ids, g.vertex_ids);

    let mut fixed = g.clone();
    fixed.meshing.as_mut().unwrap().constraints[0].kind = MeshConstraintKind::User;
    let moved = edit_manual_mesh(
        &fixed,
        ManualMeshOperation::MoveVertices {
            targets: vec![target(50, Vec2::new(0.85, 0.25))],
            motion_from: None,
        },
    )
    .unwrap()
    .geometry;
    assert_eq!(
        moved.meshing.as_ref().unwrap().constraints,
        fixed.meshing.as_ref().unwrap().constraints
    );
}

#[test]
fn boundary_delete_encloses_retained_sites_and_splits_new_outline() {
    let g = with_sites(square_geometry(), &[(50, 0.5, 0.5), (60, 0.8, 0.2)]);
    let deleted = edit_manual_mesh(&g, ManualMeshOperation::DeleteVertices { ids: vec![20] })
        .unwrap()
        .geometry;
    assert_eq!(deleted.vertex_ids, vec![10, 30, 40, 50, 60]);
    assert!(deleted.meshing.as_ref().unwrap().boundary.contains(&60));
    let deleted = edit_manual_mesh(
        &deleted,
        ManualMeshOperation::DeleteVertices { ids: vec![60] },
    )
    .unwrap()
    .geometry;
    assert_eq!(
        deleted.meshing.as_ref().unwrap().boundary,
        vec![10, 50, 30, 40]
    );
    let split = edit_manual_mesh(
        &deleted,
        ManualMeshOperation::InsertVertex {
            id: 70,
            uv: Vec2::new(0.25, 0.25),
        },
    )
    .unwrap()
    .geometry;
    assert!(split.meshing.unwrap().boundary.contains(&70));
}

#[test]
fn boundary_moves_past_free_sites_and_batch_delete_recovers_an_outline() {
    let g = with_sites(
        square_geometry(),
        &[(50, 0.2, 0.2), (60, 0.8, 0.2), (70, 0.5, 0.7)],
    );
    let moved = edit_manual_mesh(
        &g,
        ManualMeshOperation::MoveVertices {
            targets: vec![target(10, Vec2::new(0.4, 0.4))],
            motion_from: None,
        },
    )
    .unwrap()
    .geometry;
    assert!(moved.meshing.as_ref().unwrap().boundary.contains(&50));
    assert_eq!(moved.vertex_ids, g.vertex_ids);
    let deleted = edit_manual_mesh(
        &g,
        ManualMeshOperation::DeleteVertices {
            ids: vec![10, 20, 30, 40],
        },
    )
    .unwrap()
    .geometry;
    assert_eq!(deleted.vertex_ids, vec![50, 60, 70]);
    assert_eq!(deleted.triangles.len(), 1);
}

#[test]
fn generated_outline_supports_delete_move_and_projected_split() {
    let alpha: Vec<_> = (0..256)
        .flat_map(|y| {
            (0..256).map(move |x| {
                if ((x as f64 - 128.) / 100.).powi(2) + ((y as f64 - 128.) / 85.).powi(2) < 1. {
                    255
                } else {
                    0
                }
            })
        })
        .collect();
    let g = zipzi_sdk::alpha_mesh_geometry(
        zipzi_sdk::AlphaMask {
            width: 256,
            height: 256,
            alpha: &alpha,
        },
        &zipzi_sdk::AlphaMeshOptions {
            outside_spacing: 28.,
            inside_spacing: 32.,
            ..zipzi_sdk::AlphaMeshOptions::deformation_large()
        },
    )
    .unwrap();
    let boundary = &g.meshing.as_ref().unwrap().boundary;
    let uv = |id| g.uvs[g.vertex_ids.iter().position(|v| *v == id).unwrap()];
    for i in (0..boundary.len()).step_by((boundary.len() / 6).max(1)) {
        let id = boundary[i];
        edit_manual_mesh(&g, ManualMeshOperation::DeleteVertices { ids: vec![id] })
            .unwrap_or_else(|e| panic!("delete generated boundary {id}: {e}"));
        let p = uv(id);
        edit_manual_mesh(
            &g,
            ManualMeshOperation::MoveVertices {
                targets: vec![target(id, Vec2::new(p.x * 0.7 + 0.15, p.y * 0.7 + 0.15))],
                motion_from: None,
            },
        )
        .unwrap_or_else(|e| panic!("move generated boundary {id}: {e}"));
        let q = uv(boundary[(i + 1) % boundary.len()]);
        edit_manual_mesh(
            &g,
            ManualMeshOperation::InsertVertex {
                id: g.vertex_ids.iter().max().unwrap() + 1,
                uv: Vec2::new(p.x * 0.63 + q.x * 0.37, p.y * 0.63 + q.y * 0.37),
            },
        )
        .unwrap_or_else(|e| panic!("split generated boundary {id}: {e}"));
    }
}

#[test]
fn concave_boundary_expands_to_outside_point_and_reconnect_is_repeatable() {
    let g = MeshGeometry {
        vertex_ids: vec![10, 20, 30, 40, 50],
        uvs: vec![
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0.5, 0.5),
            Vec2::new(0., 1.),
        ],
        positions: vec![
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0.5, 0.5),
            Vec2::new(0., 1.),
        ],
        triangles: vec![[10, 20, 40], [20, 30, 40], [10, 40, 50]],
        meshing: None,
    };
    assert_eq!(
        extract_manual_meshing(&g).unwrap().boundary,
        vec![10, 20, 30, 40, 50]
    );
    let expanded = edit_manual_mesh(
        &g,
        ManualMeshOperation::InsertVertex {
            id: 60,
            uv: Vec2::new(0.5, 0.9),
        },
    )
    .unwrap();
    assert!(expanded
        .geometry
        .meshing
        .as_ref()
        .unwrap()
        .boundary
        .contains(&60));
    assert_eq!(expanded.geometry.vertex_ids.len(), 6);
    for face in &g.triangles {
        assert!(expanded.geometry.triangles.contains(face));
    }
    let a = edit_manual_mesh(&g, ManualMeshOperation::Reconnect).unwrap();
    let b = edit_manual_mesh(&g, ManualMeshOperation::Reconnect).unwrap();
    assert_eq!(a.geometry, b.geometry);
    assert_eq!(g.meshing, None);
}

#[test]
fn no_op_and_precision_failure_leave_implicit_structure_unwritten() {
    let g = square_geometry();
    let mut ids = ManualVertexIdAllocator::new(&g);
    let canceled_id = ids.allocate(&g).unwrap();
    assert_eq!(ids.allocate(&g).unwrap(), canceled_id + 1);
    let unchanged = edit_manual_mesh(
        &g,
        ManualMeshOperation::MoveVertices {
            targets: vec![target(10, Vec2::new(0., 0.))],
            motion_from: None,
        },
    )
    .unwrap();
    assert!(!unchanged.changed);
    assert_eq!(unchanged.geometry, g);
    let near = Vec2::new(f32::EPSILON, 0.);
    assert!(edit_manual_mesh(&g, ManualMeshOperation::InsertVertex { id: 50, uv: near }).is_err());
    assert_eq!(g.vertex_ids.len(), 4);
}

#[test]
fn sparse_batch_motion_cannot_pass_through_collapsed_geometry() {
    let g = square_geometry();
    let rotated = ManualMeshOperation::MoveVertices {
        targets: vec![
            target(10, Vec2::new(1., 1.)),
            target(20, Vec2::new(0., 1.)),
            target(30, Vec2::new(0., 0.)),
            target(40, Vec2::new(1., 0.)),
        ],
        motion_from: None,
    };
    assert!(edit_manual_mesh(&g, rotated).is_err());

    let mut inset = g.clone();
    for uv in &mut inset.uvs {
        uv.x = uv.x * 0.5 + 0.2;
        uv.y = uv.y * 0.5 + 0.2;
    }
    for p in &mut inset.positions {
        p.x = p.x * 0.5 - 0.4;
        p.y = p.y * 0.5 + 0.4;
    }
    let translated = edit_manual_mesh(
        &inset,
        ManualMeshOperation::MoveVertices {
            targets: inset
                .vertex_ids
                .iter()
                .zip(&inset.uvs)
                .map(|(&id, uv)| target(id, Vec2::new(uv.x + 0.1, uv.y + 0.1)))
                .collect(),
            motion_from: None,
        },
    )
    .unwrap();
    assert_eq!(translated.geometry.vertex_ids, inset.vertex_ids);
    assert_eq!(translated.geometry.triangles.len(), 2);

    let triangle = MeshGeometry {
        vertex_ids: vec![10, 20, 30],
        uvs: vec![
            Vec2::new(0.2, 0.2),
            Vec2::new(0.8, 0.2),
            Vec2::new(0.5, 0.8),
        ],
        positions: vec![
            Vec2::new(0.2, 0.2),
            Vec2::new(0.8, 0.2),
            Vec2::new(0.5, 0.8),
        ],
        triangles: vec![[10, 20, 30]],
        meshing: None,
    };
    assert!(edit_manual_mesh(
        &triangle,
        ManualMeshOperation::MoveVertices {
            targets: vec![
                target(10, Vec2::new(0.8, 0.8)),
                target(20, Vec2::new(0.2, 0.8)),
                target(30, Vec2::new(0.5, 0.2))
            ],
            motion_from: None,
        }
    )
    .is_err());
}

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
    assert!(!receipt
        .changes
        .categories
        .contains(&zipzi_sdk::ChangeCategory::EvaluationInputs));
    assert!(receipt
        .changes
        .content_changes
        .iter()
        .any(|c| c.entity.id == id(2)
            && c.domain == SceneContentDomain::MeshTopology
            && c.topology));
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
    assert!(s
        .edit("accidental clear", None, |e| e.replace_mesh(clear))
        .is_err());
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
        assert!(s
            .edit("invalid raw edit", None, |e| e
                .edit_scene_content(operation))
            .is_err());
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
    assert!(s
        .mesh(&id(2))
        .unwrap()
        .meshing
        .unwrap()
        .constraints
        .is_empty());
    let version = s.version();
    mesh.id = id(3);
    mesh.triangles.clear();
    assert!(s
        .edit("invalid create", None, |e| e.create_mesh(mesh))
        .is_err());
    assert_eq!(s.version(), version);
}

#[test]
fn projected_insert_splits_a_slanted_edge_after_f32_rounding() {
    let mut source = square_geometry();
    source.uvs = vec![
        Vec2::new(0.1, 0.2),
        Vec2::new(0.8, 0.15),
        Vec2::new(0.85, 0.8),
        Vec2::new(0.15, 0.85),
    ];
    source.positions = source.uvs.clone();
    let a = source.uvs[0];
    let b = source.uvs[1];
    let t = 0.37_f64;
    let uv = Vec2::new(
        (a.x as f64 + t * (b.x as f64 - a.x as f64)) as f32,
        (a.y as f64 + t * (b.y as f64 - a.y as f64)) as f32,
    );
    let result =
        edit_manual_mesh(&source, ManualMeshOperation::InsertVertex { id: 50, uv }).unwrap();
    assert!(result
        .geometry
        .meshing
        .as_ref()
        .unwrap()
        .boundary
        .contains(&50));
    assert_eq!(result.geometry.triangles.len(), 3);
}

#[test]
fn outside_insert_expands_outline_without_replacing_old_faces() {
    let mut source = square_geometry();
    for uv in &mut source.uvs {
        uv.x = 0.25 + uv.x * 0.5;
        uv.y = 0.25 + uv.y * 0.5;
    }
    source.positions = source.uvs.clone();
    for uv in [
        Vec2::new(0.9, 0.5),
        Vec2::new(0.9, 0.9),
        Vec2::new(0.5, 0.1),
    ] {
        let result =
            edit_manual_mesh(&source, ManualMeshOperation::InsertVertex { id: 50, uv }).unwrap();
        assert!(result
            .geometry
            .meshing
            .as_ref()
            .unwrap()
            .boundary
            .contains(&50));
        assert_eq!(result.geometry.vertex_ids.len(), 5);
        for face in &source.triangles {
            assert!(result.geometry.triangles.contains(face));
        }
    }
}

#[test]
fn deleting_outer_edge_exposes_remaining_faces_and_removes_orphans() {
    let source = square_geometry();
    let result =
        edit_manual_mesh(&source, ManualMeshOperation::DeleteEdge { a: 10, b: 20 }).unwrap();
    assert_eq!(result.geometry.triangles, vec![[10, 30, 40]]);
    assert_eq!(result.removed, vec![20]);
    assert_eq!(
        result.geometry.meshing.as_ref().unwrap().boundary,
        vec![10, 30, 40]
    );
    assert!(edit_manual_mesh(
        &result.geometry,
        ManualMeshOperation::DeleteEdge { a: 10, b: 30 }
    )
    .is_err());

    let fan = edit_manual_mesh(
        &source,
        ManualMeshOperation::InsertVertex {
            id: 50,
            uv: Vec2::new(0.5, 0.5),
        },
    )
    .unwrap()
    .geometry;
    let result = edit_manual_mesh(&fan, ManualMeshOperation::DeleteEdge { a: 10, b: 20 }).unwrap();
    assert!(result.removed.is_empty());
    assert_eq!(
        result.geometry.meshing.as_ref().unwrap().boundary,
        vec![10, 50, 20, 30, 40]
    );
    assert_eq!(result.geometry.triangles.len(), 3);
}

#[test]
fn connected_insert_is_atomic_and_can_extend_the_outline() {
    let mut source = square_geometry();
    source = edit_manual_mesh(&source, ManualMeshOperation::ConstrainEdge { a: 10, b: 30 })
        .unwrap()
        .geometry;
    assert!(edit_manual_mesh(
        &source,
        ManualMeshOperation::InsertConnectedVertex {
            from: 20,
            id: 50,
            uv: Vec2::new(0.2, 0.8)
        }
    )
    .is_err());
    assert_eq!(source.vertex_ids.len(), 4);
    let result = edit_manual_mesh(
        &source,
        ManualMeshOperation::InsertConnectedVertex {
            from: 20,
            id: 50,
            uv: Vec2::new(0.8, 0.2),
        },
    )
    .unwrap();
    assert_eq!(result.added, vec![50]);
    assert!(result
        .geometry
        .meshing
        .as_ref()
        .unwrap()
        .constraints
        .iter()
        .any(|c| c.endpoints == [20, 50]));
    let cut = edit_manual_mesh(
        &square_geometry(),
        ManualMeshOperation::DeleteEdge { a: 10, b: 20 },
    )
    .unwrap()
    .geometry;
    let result = edit_manual_mesh(
        &cut,
        ManualMeshOperation::InsertConnectedVertex {
            from: 10,
            id: 50,
            uv: Vec2::new(0.8, 0.2),
        },
    )
    .unwrap();
    assert!(result
        .geometry
        .meshing
        .as_ref()
        .unwrap()
        .boundary
        .contains(&50));
}

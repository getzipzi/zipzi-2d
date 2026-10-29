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

#[path = "../../../fixtures/mesh-edit-dependencies.rs"]
mod dependencies;
fn mixed() -> AuthoringSession {
    let mut s = AuthoringSession::new(&dependencies::id(1), ModelSetup::default()).unwrap();
    s.edit("mixed fixture", None, dependencies::install)
        .unwrap();
    s
}
#[test]
fn transfer_mixed_shapes_parent_glues_and_identity_in_one_history_step() {
    use zipzi_core::DeltaKeyforms;
    let mut s = mixed();
    let id = dependencies::id;
    let before = s.document().clone();
    let history = s.history_lengths();
    let uv = Vec2::new(0.35, 0.25);
    let mut edit = s.begin_owned_edit("sample all channels", None).unwrap();
    let plan = edit
        .plan_manual_mesh(&id(2), ManualMeshOperation::InsertVertex { id: 200, uv })
        .unwrap();
    assert_eq!(plan.report.shape_count, 7);
    assert!(!plan.report.affine_extension);
    assert_eq!(plan.report.added, 1);
    assert!(!plan.report.appearance_limits.is_empty());
    // Point lies in [10,30,120]; independently solve its barycentric source.
    let a = before.get_mesh(&id(2)).unwrap().uvs[0];
    let b = before.get_mesh(&id(2)).unwrap().uvs[1];
    let c = before.get_mesh(&id(2)).unwrap().uvs[4];
    let wc = (uv.y - a.y) / (c.y - a.y);
    let wb = (uv.x - a.x - wc * (c.x - a.x)) / (b.x - a.x);
    let wa = 1. - wb - wc;
    let check = |old: &[Vec2], new: &[Vec2]| {
        assert_eq!(&new[..5], old);
        let p = Vec2::new(
            old[0].x * wa + old[1].x * wb + old[4].x * wc,
            old[0].y * wa + old[1].y * wb + old[4].y * wc,
        );
        assert!((new[5].x - p.x).abs() < 2e-5 && (new[5].y - p.y).abs() < 2e-5);
    };
    edit.apply_manual_mesh_plan(plan).unwrap();
    edit.commit_to(&mut s).unwrap();
    let after = s.document();
    check(
        &before.get_mesh(&id(2)).unwrap().base_positions,
        &after.get_mesh(&id(2)).unwrap().base_positions,
    );
    assert_eq!(after.get_mesh(&id(2)).unwrap().deformer_id, id(3));
    let old = before.get_binding(&id(6)).unwrap();
    let new = after.get_binding(&id(6)).unwrap();
    assert_eq!(old.axes, new.axes);
    for (a, b) in old.keyforms.iter().zip(&new.keyforms) {
        check(&a.positions, &b.positions);
        assert_eq!(a.samples, b.samples);
        assert_eq!(a.appearance, b.appearance);
    }
    for n in [12, 22] {
        let old = before.get_blend_binding(&id(n)).unwrap();
        let new = after.get_blend_binding(&id(n)).unwrap();
        let (DeltaKeyforms::Mesh(a), DeltaKeyforms::Mesh(b)) = (&old.keyforms, &new.keyforms)
        else {
            panic!()
        };
        for (a, b) in a.iter().zip(b) {
            check(&a.positions, &b.positions);
            assert_eq!(a.sample_id, b.sample_id);
            assert_eq!(a.opacity, b.opacity);
        }
        assert_eq!(b[0].positions, vec![Vec2::default(); 6]);
    }
    for n in [30, 32] {
        assert_eq!(before.get_glue(&id(n)), after.get_glue(&id(n)));
    }
    for n in [11, 21] {
        assert_eq!(
            before.get_blend_key_table(&id(n)),
            after.get_blend_key_table(&id(n))
        );
    }
    let after = after.clone();
    assert_eq!(s.history_lengths(), (history.0 + 1, 0));
    s.undo().unwrap();
    assert_eq!(s.document().get_mesh(&id(2)), before.get_mesh(&id(2)));
    s.redo().unwrap();
    assert_eq!(s.document().get_binding(&id(6)), after.get_binding(&id(6)));
}
#[test]
fn transfer_drag_freezes_source_and_recovers_after_invalid_increment() {
    let id = dependencies::id;
    let mut s = mixed();
    let mut direct = s.begin_owned_edit("direct", None).unwrap();
    let mut drag = s.begin_owned_edit("drag", None).unwrap();
    let movement = |x, y| ManualMeshOperation::MoveVertices {
        targets: vec![target(120, Vec2::new(x, y))],
        motion_from: None,
    };
    direct.edit_manual_mesh(&id(2), movement(0.7, 0.6)).unwrap();
    for (x, y) in [(0.3, 0.4), (0.6, 0.3), (0.7, 0.6)] {
        drag.edit_manual_mesh(&id(2), movement(x, y)).unwrap();
    }
    assert_eq!(
        drag.candidate_document().get_mesh(&id(2)),
        direct.candidate_document().get_mesh(&id(2))
    );
    assert_eq!(
        drag.candidate_document().get_binding(&id(6)),
        direct.candidate_document().get_binding(&id(6))
    );
    assert_eq!(
        drag.plan_manual_mesh(&id(2), movement(1., 0.6))
            .unwrap_err()
            .code
            .as_ref(),
        "OUTSIDE_TRANSFER_DOMAIN"
    );
    assert!(!drag.is_aborted());
    drag.edit_manual_mesh(&id(2), movement(0.7, 0.6)).unwrap();
    drag.commit_to(&mut s).unwrap();
}
#[test]
fn transfer_glue_deletion_and_outside_sampling_are_operation_rejections() {
    let id = dependencies::id;
    let s = mixed();
    let mut e = s.begin_owned_edit("check", None).unwrap();
    for vertex in [10, 70] {
        let err = e
            .plan_manual_mesh(
                &id(2),
                ManualMeshOperation::DeleteVertices { ids: vec![vertex] },
            )
            .unwrap_err();
        assert_eq!(err.code.as_ref(), "GLUE_VERTEX_REFERENCED");
        assert!(err.object_ids.len() >= 3);
        assert!(err.field_path.is_some());
    }
    let err = e
        .plan_manual_mesh(
            &id(2),
            ManualMeshOperation::InsertVertex {
                id: 200,
                uv: Vec2::new(0.05, 0.3),
            },
        )
        .unwrap_err();
    assert_eq!(err.code.as_ref(), "OUTSIDE_TRANSFER_DOMAIN");
    e.edit_manual_mesh(
        &id(2),
        ManualMeshOperation::DeleteVertices { ids: vec![120] },
    )
    .unwrap();
    e.edit_manual_mesh(&id(2), ManualMeshOperation::ConstrainEdge { a: 30, b: 90 })
        .unwrap();
    let err = e
        .plan_manual_mesh(&id(2), ManualMeshOperation::DeleteEdge { a: 10, b: 30 })
        .unwrap_err();
    assert_eq!(err.code.as_ref(), "GLUE_VERTEX_REFERENCED");
    let b = e.candidate_document().get_binding(&id(6)).unwrap();
    assert_eq!(b.keyforms[1].positions.len(), 4);
}
#[test]
fn transfer_locked_equal_dependencies_are_reads_but_resampling_requires_write_access() {
    let id = dependencies::id;
    let mut s = mixed();
    s.edit("lock dependencies", None, |e| {
        let mut state = e.candidate_document().editor_state().clone();
        state.objects.entry(id(6)).or_default().locked = true;
        state.objects.entry(id(30)).or_default().locked = true;
        e.replace_editor_state(state)
    })
    .unwrap();
    let mut e = s.begin_owned_edit("fixed existing edge", None).unwrap();
    e.edit_manual_mesh(&id(2), ManualMeshOperation::ConstrainEdge { a: 10, b: 120 })
        .unwrap();
    let err = e
        .plan_manual_mesh(
            &id(2),
            ManualMeshOperation::InsertVertex {
                id: 200,
                uv: Vec2::new(0.35, 0.25),
            },
        )
        .unwrap_err();
    assert_eq!(err.code.as_ref(), "OBJECT_LOCKED");
}
#[test]
fn transfer_budget_and_cancellation_reject_before_channel_allocation() {
    let id = dependencies::id;
    let s = mixed();
    let source = zipzi_sdk::ManualMeshSource::new(s.document(), &id(2)).unwrap();
    let limits = zipzi_core::document::AuthoringLimits {
        max_conversion_bytes: 1,
        ..Default::default()
    };
    assert!(source
        .plan(
            s.document(),
            s.version(),
            ManualMeshOperation::Reconnect,
            limits,
            &Default::default()
        )
        .is_err());
    let control = zipzi_core::document::ConversionControl::default();
    control.cancel();
    assert!(source
        .plan(
            s.document(),
            s.version(),
            ManualMeshOperation::Reconnect,
            Default::default(),
            &control
        )
        .is_err());
}

#[test]
fn transfer_plans_reject_same_revision_branches_and_changed_dependencies() {
    let id = dependencies::id;
    let s = mixed();
    let mut a = s.begin_owned_edit("a", None).unwrap();
    let mut b = s.begin_owned_edit("b", None).unwrap();
    a.rename_mesh(&id(2), "A").unwrap();
    b.rename_mesh(&id(2), "B").unwrap();
    assert_eq!(
        a.candidate_document().revision(),
        b.candidate_document().revision()
    );
    let p = a
        .plan_manual_mesh(&id(2), ManualMeshOperation::Reconnect)
        .unwrap();
    assert_eq!(
        b.apply_manual_mesh_plan(p).unwrap_err().code.as_ref(),
        "STALE_MANUAL_MESH_PLAN"
    );
    let source = zipzi_sdk::ManualMeshSource::new(s.document(), &id(2)).unwrap();
    assert_eq!(
        source
            .plan(
                a.candidate_document(),
                s.version(),
                ManualMeshOperation::Reconnect,
                Default::default(),
                &Default::default()
            )
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_MANUAL_MESH_PLAN"
    );
}

#[test]
fn transfer_affine_parent_subdivision_preserves_sampled_uv_position_field() {
    let id = dependencies::id;
    let mut s = mixed();
    let before = s.read_snapshot();
    s.edit("subdivide", None, |e| {
        e.edit_manual_mesh(
            &id(2),
            ManualMeshOperation::InsertVertex {
                id: 200,
                uv: Vec2::new(0.35, 0.25),
            },
        )
        .map(|_| ())
    })
    .unwrap();
    // Independent evaluation comparison uses the same UV coordinates, never
    // array slots across different topologies. Glue has zero influence here.
    let sample = |frame: &zipzi_core::DrawableFrame, u: f32, v: f32| {
        let d = frame.drawables.iter().find(|d| d.id == id(2)).unwrap();
        for t in d.indices.chunks_exact(3) {
            let [a, b, c] = [
                d.uvs[t[0] as usize],
                d.uvs[t[1] as usize],
                d.uvs[t[2] as usize],
            ];
            let det = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
            let wb = ((u - a.x) * (c.y - a.y) - (v - a.y) * (c.x - a.x)) / det;
            let wc = ((b.x - a.x) * (v - a.y) - (b.y - a.y) * (u - a.x)) / det;
            let wa = 1. - wb - wc;
            if [wa, wb, wc].iter().all(|w| *w >= -1e-6) {
                let mut p = Vec2::default();
                for (&i, w) in t.iter().zip([wa, wb, wc]) {
                    p.x += d.positions[i as usize].x * w;
                    p.y += d.positions[i as usize].y * w;
                }
                return p;
            }
        }
        panic!("missing UV {u},{v}")
    };
    let mut maximum = 0_f32;
    for ordinary in [0., 0.25, 0.5, 0.75, 1.] {
        for blend in [0., 0.3, 1.] {
            let values = [(id(5), ordinary), (id(10), blend), (id(20), 1. - blend)]
                .into_iter()
                .collect();
            let a = before.evaluate(&values).unwrap();
            let b = s.evaluate(&values).unwrap();
            for (u, v) in [(0.2, 0.2), (0.35, 0.25), (0.7, 0.3), (0.6, 0.75)] {
                let a = sample(&a, u, v);
                let b = sample(&b, u, v);
                maximum = maximum.max((a.x - b.x).hypot(a.y - b.y));
            }
        }
    }
    assert!(maximum < 2e-5, "UV sample error {maximum}");
}
#[test]
fn transfer_nested_warp_and_rotation_report_nonlinear_parent_chain() {
    use zipzi_core::{Transform, TransformData, WarpTransform};
    let id = dependencies::id;
    let mut s = mixed();
    s.edit("warp ancestor", None, |e| {
        e.create_transform(Transform {
            id: id(50),
            data: TransformData::Warp(WarpTransform {
                rows: 1,
                columns: 1,
                quad: true,
                points: vec![
                    Vec2::new(0., 0.),
                    Vec2::new(4., 0.),
                    Vec2::new(0., 4.),
                    Vec2::new(6., 7.),
                ],
                bezier: None,
            }),
            ..Default::default()
        })?;
        let mut rotation = e
            .candidate_document()
            .get_transform(&id(3))
            .unwrap()
            .clone();
        rotation.parent_id = zipzi_core::TransformId::optional(id(50));
        e.replace_transform(rotation)
    })
    .unwrap();
    let e = s.begin_owned_edit("sample", None).unwrap();
    let plan = e
        .plan_manual_mesh(
            &id(2),
            ManualMeshOperation::InsertVertex {
                id: 200,
                uv: Vec2::new(0.35, 0.25),
            },
        )
        .unwrap();
    assert!(plan.report.non_affine_parent);
    assert!(plan
        .report
        .appearance_limits
        .iter()
        .any(|r| r.contains("Warp")));
    assert!(matches!(
        zipzi_sdk::ManualMeshSource::new(s.document(), &id(2))
            .unwrap()
            .geometry_space,
        zipzi_sdk::GeometrySpace::RotationLocal(_)
    ));
}

#[test]
fn transfer_drag_net_noop_and_cancel_preserve_redo() {
    let id = dependencies::id;
    let mut s = mixed();
    s.edit("later", None, |e| e.rename_mesh(&id(2), "later"))
        .unwrap();
    s.undo().unwrap();
    let before = s.history_state();
    let mut e = s.begin_owned_edit("out and back", None).unwrap();
    for uv in [Vec2::new(0.6, 0.3), Vec2::new(0.4, 0.45)] {
        e.edit_manual_mesh(
            &id(2),
            ManualMeshOperation::MoveVertices {
                targets: vec![target(120, uv)],
                motion_from: None,
            },
        )
        .unwrap();
    }
    assert!(!e.commit_to(&mut s).unwrap().changed);
    assert_eq!(s.history_state(), before);
    let mut e = s.begin_owned_edit("cancel", None).unwrap();
    e.edit_manual_mesh(
        &id(2),
        ManualMeshOperation::InsertVertex {
            id: 300,
            uv: Vec2::new(0.35, 0.25),
        },
    )
    .unwrap();
    drop(e);
    assert_eq!(s.history_state(), before);
}

#[test]
fn transfer_collapsed_base_still_samples_valid_uv_domain() {
    let id = dependencies::id;
    let mut s = mixed();
    s.edit("collapsed base", None, |e| {
        let mesh = e.candidate_document().get_mesh(&id(2)).unwrap();
        let ids = mesh.vertex_ids.clone();
        e.update_positions(&id(2), &ids, &vec![Vec2::default(); ids.len()])
    })
    .unwrap();
    s.edit("UV insertion", None, |e| {
        e.edit_manual_mesh(
            &id(2),
            ManualMeshOperation::InsertVertex {
                id: 200,
                uv: Vec2::new(0.35, 0.25),
            },
        )
        .map(|_| ())
    })
    .unwrap();
    assert_eq!(
        s.mesh(&id(2)).unwrap().base_positions,
        vec![Vec2::default(); 6]
    );
}

#[test]
fn transfer_edited_shapes_roundtrip_through_external_export() {
    let id = dependencies::id;
    let mut s = mixed();
    let texture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    s.edit("configure export", None, |e| {
        e.create_asset(zipzi_sdk::prepare_png_asset(&id(80), "Texture", &texture)?)?;
        e.erase_object(&id(32))?; // External acceptance uses fully configured pairs.
        for n in [2, 4] {
            let mut m = e.candidate_document().get_mesh(&id(n)).unwrap().clone();
            m.texture_asset_id = id(80).into();
            e.replace_mesh(m)?;
        }
        e.edit_manual_mesh(
            &id(2),
            ManualMeshOperation::InsertVertex {
                id: 200,
                uv: Vec2::new(0.35, 0.25),
            },
        )
        .map(|_| ())
    })
    .unwrap();
    let before = s.read_snapshot();
    let artifact = s.encode_cmo3(&zipzi_sdk::Cmo3Options::default()).unwrap();
    assert_eq!(&artifact.bytes[..4], b"CAFF");
    assert!(!artifact.report.has_loss());
    assert!(s.document().same_content(before.document()));
    let root = std::env::temp_dir().join(format!("zipzi-manual-export-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("model.cmo3");
    std::fs::write(&path, artifact.bytes).unwrap();
    let mut imported = AuthoringSession::new(&id(90), ModelSetup::default()).unwrap();
    imported.import_cubism(&path, None).unwrap();
    assert_eq!(imported.document().mesh_order().len(), 2);
    for parameter in [0., 0.5, 1.] {
        let values: zipzi_core::PreviewValues = [(id(5), parameter), (id(10), 0.3), (id(20), 0.7)]
            .into_iter()
            .collect();
        let remapped = values
            .iter()
            .map(|(id, v)| {
                let p = imported
                    .document()
                    .parameter_order()
                    .iter()
                    .filter_map(|p| imported.document().get_parameter(p))
                    .find(|p| p.runtime_id == *id)
                    .unwrap();
                (p.id.clone(), *v)
            })
            .collect();
        let a = before.evaluate(&values).unwrap();
        let b = imported.evaluate(&remapped).unwrap();
        for mesh in a.drawables {
            let result = b
                .drawables
                .iter()
                .find(|m| m.runtime_id == mesh.runtime_id)
                .unwrap();
            assert_eq!(result.positions.len(), mesh.positions.len());
            for p in mesh.positions {
                assert!(result
                    .positions
                    .iter()
                    .any(|q| (p.x - q.x).hypot(p.y - q.y) < 1e-3));
            }
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn transfer_equal_locked_glue_keeps_runtime_and_pair_id_reservations() {
    let id = dependencies::id;
    let mut s = mixed();
    s.edit("lock carried Glue", None, |e| {
        let mut state = e.candidate_document().editor_state().clone();
        state.objects.entry(id(32)).or_default().locked = true;
        e.replace_editor_state(state)
    })
    .unwrap();
    let before = s.read_snapshot();
    let source = s.geometry(&id(2)).unwrap();
    let doc = s.document();
    let replacement = TopologyReplacement {
        mesh: doc.get_mesh(&id(2)).unwrap().clone(),
        binding: doc.binding_for_mesh(&id(2)).cloned(),
        blend_bindings: doc
            .blend_binding_order()
            .iter()
            .filter_map(|id| doc.get_blend_binding(id))
            .cloned()
            .collect(),
        glues: doc
            .glue_order()
            .iter()
            .map(|key| {
                let mut glue = doc.get_glue(key).unwrap().clone();
                if glue.id == id(30) {
                    glue.runtime_id = id(32);
                }
                glue
            })
            .collect(),
        vertex_mapping: source.vertex_ids.iter().map(|&v| (v, Some(v))).collect(),
    };
    assert_eq!(
        s.edit("bad complete replacement", None, |e| e
            .replace_topology(&source, replacement))
            .unwrap_err()
            .code
            .as_ref(),
        "DUPLICATE_RUNTIME_ID"
    );
    assert!(s.document().same_content(before.document()));
}

#[test]
fn transfer_new_move_selection_retains_previous_completed_positions() {
    let id = dependencies::id;
    let s = mixed();
    let mut e = s.begin_owned_edit("two selections", None).unwrap();
    e.edit_manual_mesh(
        &id(2),
        ManualMeshOperation::MoveVertices {
            targets: vec![target(120, Vec2::new(0.6, 0.3))],
            motion_from: None,
        },
    )
    .unwrap();
    let accepted = e.candidate_document().get_mesh(&id(2)).unwrap().clone();
    e.edit_manual_mesh(
        &id(2),
        ManualMeshOperation::MoveVertices {
            targets: vec![target(10, Vec2::new(0.12, 0.12))],
            motion_from: None,
        },
    )
    .unwrap();
    let current = e.candidate_document().get_mesh(&id(2)).unwrap();
    assert_eq!(current.uvs[4], accepted.uvs[4]);
    assert_eq!(current.base_positions[4], accepted.base_positions[4]);
}

#[test]
fn transfer_generated_topology_cannot_bypass_the_complete_image_rectangle() {
    let id = dependencies::id;
    let s = mixed();
    let source = zipzi_sdk::ManualMeshSource::new(s.document(), &id(2)).unwrap();
    let mut g = source.geometry().clone();
    g.uvs[0].x = -0.1;
    let error = source
        .plan_geometry(
            s.document(),
            s.version(),
            g,
            Default::default(),
            &Default::default(),
        )
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "UV_OUTSIDE_SOURCE_IMAGE");
    assert!(error.field_path.is_some());
}

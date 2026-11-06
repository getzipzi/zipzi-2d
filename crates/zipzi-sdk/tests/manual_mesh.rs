use zipzi_core::{
    document::*, Mesh, MeshConstraint, MeshConstraintKind, MeshMeshing, ModelSetup, Vec2,
};
use zipzi_sdk::{
    add_manual_interior_sites, edit_manual_mesh, extract_manual_meshing, AffineUvMap,
    AuthoringSession, ConversionControl, ConversionLimits, Draft, ManualMeshOperation,
    ManualMeshOutcome, ManualVertexIdAllocator, ManualVertexTarget, MeshGeometry, PreparedPreview,
    Status,
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
    let mut s = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(&id(1), (ModelSetup::default()).into()).unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_mesh({
                    let mut value = Mesh {
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
                    };
                    value.runtime_id = value.id.clone();
                    value
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("mesh")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    s.take_events();
    s
}

#[test]
fn constraint_only_commit_notifies_and_restores_exact_geometry_without_evaluation() {
    let mut s = session();
    let before = s.snapshot().document().mesh(&id(2)).unwrap().clone();
    let frame = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    let mut mesh = before.clone();
    mesh.meshing
        .as_mut()
        .unwrap()
        .constraints
        .push(MeshConstraint {
            endpoints: [10, 30],
            kind: MeshConstraintKind::EdgeSupport,
        });
    let (_, p) = crate::support::prepared(&s, |c| c.replace_mesh(mesh.clone())).unwrap();
    let r = s.publish(p).unwrap();
    assert_eq!(r.writes.records().len(), 1);
    assert_eq!(r.writes.records()[0].id, id(2));
    assert_eq!(
        crate::support::frame(s.snapshot(), &Default::default())
            .unwrap()
            .drawables,
        frame.drawables
    );
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().mesh(&id(2)), Some(&before));
    s.redo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().mesh(&id(2)), Some(&mesh));
    let mut user = mesh.clone();
    user.meshing.as_mut().unwrap().constraints[0].kind = MeshConstraintKind::User;
    let (_, p) = crate::support::prepared(&s, |c| c.replace_mesh(user)).unwrap();
    assert!(!s.publish(p).unwrap().writes.is_empty());
    s.undo(s.version()).unwrap();
    let h = s.history_state();
    let v = s.version();
    let (_, p) = crate::support::prepared(&s, |c| c.replace_mesh(mesh)).unwrap();
    assert!(s.publish(p).unwrap().writes.is_empty());
    assert_eq!(s.history_state(), h);
    assert_eq!(s.version(), v);
}

#[test]
fn mesh_edits_preserve_or_reject_structure_atomically() {
    let mut s = session();
    let structure = s
        .snapshot()
        .document()
        .mesh(&id(2))
        .cloned()
        .unwrap()
        .meshing;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.set_vertex_positions(&id(2), &[(10, Vec2::new(100., 100.))])?;
                e.rename(zipzi_sdk::NameTarget::Mesh(&id(2)), ("renamed").into())
            })
            .and_then(|value| {
                let prepared = draft.prepare("pose and label")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .mesh(&id(2))
            .cloned()
            .unwrap()
            .meshing,
        structure
    );
    s.undo(s.version()).unwrap();
    let before = s.snapshot().document().mesh(&id(2)).cloned().unwrap();
    let version = s.version();
    let history = s.history_state();
    let mut clear = before.clone();
    clear.meshing = None;
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_mesh(clear))
            .and_then(|value| {
                let prepared = draft.prepare("accidental clear")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
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
        assert!({
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    e.edit_scene_content(operation, Default::default(), &Default::default())
                })
                .and_then(|value| {
                    let prepared = draft.prepare("invalid raw edit")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .is_err());
        assert_eq!(
            s.snapshot().document().mesh(&id(2)).cloned().unwrap(),
            before
        );
        assert_eq!(s.version(), version);
        assert_eq!(s.history_state(), history);
    }
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_scene_content(
                    SceneContentEdit::ReorderMeshVertices {
                        mesh_id: id(2),
                        order: vec![40, 20, 10, 30],
                    },
                    Default::default(),
                    &Default::default(),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("reorder")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .mesh(&id(2))
            .cloned()
            .unwrap()
            .meshing,
        structure
    );
    assert!(s.snapshot().document().builder().freeze().is_ok());
}

#[test]
fn complete_replacement_explicitly_discards_structure_and_undo_restores_it() {
    let mut s = session();
    let before = s.snapshot().document().mesh(&id(2)).unwrap().clone();
    let mut g = geometry(s.snapshot().document(), &id(2));
    g.meshing = None;
    let (_, p) = crate::support::prepared(&s, |c| {
        c.remesh(&id(2), g, Default::default(), &Default::default())
    })
    .unwrap();
    s.publish(p).unwrap();
    assert!(s
        .snapshot()
        .document()
        .mesh(&id(2))
        .unwrap()
        .meshing
        .is_none());
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().mesh(&id(2)), Some(&before));
}

#[test]
fn candidate_snapshots_keep_structure_and_failed_creation_has_no_history() {
    let mut s = session();
    let mut mesh = s.snapshot().document().mesh(&id(2)).cloned().unwrap();
    mesh.meshing
        .as_mut()
        .unwrap()
        .constraints
        .push(MeshConstraint {
            endpoints: [10, 30],
            kind: MeshConstraintKind::User,
        });
    let mut edit = s.begin_draft();
    edit.batch(edit.source(), |c| c.replace_mesh(mesh.clone()))
        .unwrap();
    let snapshot = edit.read_snapshot().unwrap().snapshot;
    assert_eq!(snapshot.document().mesh(&id(2)).unwrap(), &mesh);
    drop(edit);
    assert!(s
        .snapshot()
        .document()
        .mesh(&id(2))
        .cloned()
        .unwrap()
        .meshing
        .unwrap()
        .constraints
        .is_empty());
    let version = s.version();
    mesh.id = id(3);
    mesh.triangles.clear();
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.create_mesh(mesh))
            .and_then(|value| {
                let prepared = draft.prepare("invalid create")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
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
    let mut s = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &dependencies::id(1),
                (ModelSetup::default()).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), dependencies::install)
            .and_then(|value| {
                let prepared = draft.prepare("mixed fixture")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    s
}
#[test]
fn transfer_mixed_shapes_parent_glues_and_identity_in_one_history_step() {
    use zipzi_core::DeltaKeyforms;
    let mut s = mixed();
    let id = dependencies::id;
    let before = s.snapshot().document().clone();
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    let uv = Vec2::new(0.35, 0.25);
    let mut edit = s.begin_draft();
    let plan = prepare_manual(
        &edit,
        &id(2),
        ManualMeshOperation::InsertVertex { id: 200, uv },
    )
    .unwrap();
    assert_eq!(plan.1.report.shape_count, 7);
    assert!(!plan.1.report.affine_extension);
    assert_eq!(plan.1.report.added, 1);
    assert!(!plan.1.report.appearance_limits.is_empty());
    // Point lies in [10,30,120]; independently solve its barycentric source.
    let a = before.mesh(&id(2)).unwrap().uvs[0];
    let b = before.mesh(&id(2)).unwrap().uvs[1];
    let c = before.mesh(&id(2)).unwrap().uvs[4];
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
    edit.install_preview(plan.0).unwrap();
    s.publish(edit.prepare("Manual mesh").unwrap()).unwrap();
    let after = s.snapshot().document();
    check(
        &before.mesh(&id(2)).unwrap().base_positions,
        &after.mesh(&id(2)).unwrap().base_positions,
    );
    assert_eq!(after.mesh(&id(2)).unwrap().deformer_id, id(3));
    let old = before.binding(&id(6)).unwrap();
    let new = after.binding(&id(6)).unwrap();
    assert_eq!(old.axes, new.axes);
    for (a, b) in old.keyforms.iter().zip(&new.keyforms) {
        check(&a.positions, &b.positions);
        assert_eq!(a.samples, b.samples);
        assert_eq!(a.appearance, b.appearance);
    }
    for n in [12, 22] {
        let old = before.blend_binding(&id(n)).unwrap();
        let new = after.blend_binding(&id(n)).unwrap();
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
        assert_eq!(before.glue(&id(n)), after.glue(&id(n)));
    }
    for n in [11, 21] {
        assert_eq!(
            before.blend_key_table(&id(n)),
            after.blend_key_table(&id(n))
        );
    }
    let after = after.clone();
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        (history.0 + 1, 0)
    );
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().mesh(&id(2)), before.mesh(&id(2)));
    s.redo(s.version()).unwrap();
    assert_eq!(
        s.snapshot().document().binding(&id(6)),
        after.binding(&id(6))
    );
}
#[test]
fn transfer_outline_extension_extrapolates_every_shape_from_the_same_boundary_triangle() {
    use zipzi_core::DeltaKeyforms;
    let id = dependencies::id;
    for insert in [false, true] {
        let mut s = mixed();
        let before = s.snapshot().document().clone();
        let uv = Vec2::new(0.3, 0.05);
        let operation = |y| {
            if insert {
                ManualMeshOperation::InsertVertex {
                    id: 200,
                    uv: Vec2::new(uv.x, y),
                }
            } else {
                ManualMeshOperation::MoveVertices {
                    targets: vec![target(10, Vec2::new(uv.x, y))],
                    motion_from: None,
                }
            }
        };
        let mut e = s.begin_draft();
        let plan = prepare_manual(&e, &id(2), operation(uv.y)).unwrap();
        assert!(plan.1.provenance.iter().any(|p| matches!(
            p,
            zipzi_sdk::VertexProvenance::Extrapolated {
                triangle: [10, 30, 120],
                ..
            }
        )));
        e.install_preview(plan.0).unwrap();
        if !insert {
            apply_gesture(&mut e, &id(2), operation(0.02)).unwrap();
            apply_gesture(&mut e, &id(2), operation(uv.y)).unwrap();
        }
        s.publish(e.prepare("Manual mesh").unwrap()).unwrap();
        let after = s.snapshot().document().clone();
        let old_mesh = before.mesh(&id(2)).unwrap();
        let a = old_mesh.uvs[0];
        let b = old_mesh.uvs[1];
        let c = old_mesh.uvs[4];
        let wc = (uv.y - a.y) / (c.y - a.y);
        let wb = (uv.x - a.x - wc * (c.x - a.x)) / (b.x - a.x);
        let wa = 1. - wb - wc;
        assert!(wc < 0.);
        let slot = if insert { 5 } else { 0 };
        let check = |old: &[Vec2], new: &[Vec2]| {
            let expected = Vec2::new(
                old[0].x * wa + old[1].x * wb + old[4].x * wc,
                old[0].y * wa + old[1].y * wb + old[4].y * wc,
            );
            assert!((new[slot].x - expected.x).hypot(new[slot].y - expected.y) < 2e-5);
            for i in 0..old.len() {
                if i != slot {
                    assert_eq!(new[i], old[i]);
                }
            }
        };
        check(
            &old_mesh.base_positions,
            &after.mesh(&id(2)).unwrap().base_positions,
        );
        for (a, b) in before
            .binding(&id(6))
            .unwrap()
            .keyforms
            .iter()
            .zip(&after.binding(&id(6)).unwrap().keyforms)
        {
            check(&a.positions, &b.positions);
            assert_eq!(a.samples, b.samples);
        }
        for n in [12, 22] {
            let (DeltaKeyforms::Mesh(a), DeltaKeyforms::Mesh(b)) = (
                &before.blend_binding(&id(n)).unwrap().keyforms,
                &after.blend_binding(&id(n)).unwrap().keyforms,
            ) else {
                panic!()
            };
            for (a, b) in a.iter().zip(b) {
                check(&a.positions, &b.positions);
            }
        }
        for n in [30, 32] {
            assert_eq!(before.glue(&id(n)), after.glue(&id(n)));
        }
        s.undo(s.version()).unwrap();
        assert_eq!(s.snapshot().document().mesh(&id(2)), before.mesh(&id(2)));
        s.redo(s.version()).unwrap();
        assert_eq!(s.snapshot().document().mesh(&id(2)), after.mesh(&id(2)));
        assert_eq!(
            s.snapshot().document().binding(&id(6)),
            after.binding(&id(6))
        );
    }
}

#[test]
fn manual_uv_map_round_trips_non_affine_extension_and_rejects_overlapping_branches() {
    let s = mixed();
    let map =
        zipzi_sdk::ManualMeshUvMap::new(geometry(s.snapshot().document(), &dependencies::id(2)))
            .unwrap();
    for uv in [
        Vec2::new(0.3, 0.05),
        Vec2::new(0.95, 0.4),
        Vec2::new(0.4, 0.95),
        Vec2::new(0.05, 0.3),
        Vec2::new(0.05, 0.05),
    ] {
        let p = map.position(uv).unwrap();
        let actual = map.uv(p).unwrap();
        assert!(
            (actual.x - uv.x).hypot(actual.y - uv.y) < 1e-5,
            "{uv:?} => {actual:?}"
        );
    }
    let mut overlapping = square_geometry();
    overlapping.positions[3] = Vec2::new(-1.5, 0.5);
    let map = zipzi_sdk::ManualMeshUvMap::new(overlapping).unwrap();
    assert_eq!(
        map.uv(Vec2::new(-1.25, 0.75)).unwrap_err().code.as_str(),
        "AMBIGUOUS_MESH_SURFACE"
    );
    assert!(map.uv(Vec2::new(f32::NAN, 0.)).is_err());
}

#[test]
fn transfer_drag_freezes_source_and_recovers_after_invalid_increment() {
    let id = dependencies::id;
    let mut s = mixed();
    let mut direct = s.begin_draft();
    let mut drag = s.begin_draft();
    let movement = |x, y| ManualMeshOperation::MoveVertices {
        targets: vec![target(120, Vec2::new(x, y))],
        motion_from: None,
    };
    apply_gesture(&mut direct, &id(2), movement(0.7, 0.6)).unwrap();
    for (x, y) in [(0.3, 0.4), (0.6, 0.3), (0.7, 0.6)] {
        apply_gesture(&mut drag, &id(2), movement(x, y)).unwrap();
    }
    assert_eq!(
        drag.document().unwrap().mesh(&id(2)),
        direct.document().unwrap().mesh(&id(2))
    );
    assert_eq!(
        drag.document().unwrap().binding(&id(6)),
        direct.document().unwrap().binding(&id(6))
    );
    assert_eq!(
        prepare_manual(&drag, &id(2), movement(1.1, 0.6))
            .unwrap_err()
            .code
            .as_str(),
        "INVALID_MANUAL_MESH"
    );
    assert!(drag.is_active());
    apply_gesture(&mut drag, &id(2), movement(0.7, 0.6)).unwrap();
    s.publish(drag.prepare("Manual mesh").unwrap()).unwrap();
}
#[test]
fn transfer_batch_freezes_each_mesh_and_rejects_without_partial_publication() {
    let id = dependencies::id;
    let mut session = mixed();
    let before = session.snapshot().document().clone();
    let history = {
        let history = session.history_state();
        (history.undo_steps, history.redo_steps)
    };
    let movement = |x, y| ManualMeshOperation::MoveVertices {
        targets: vec![target(120, Vec2::new(x, y))],
        motion_from: None,
    };
    let batch = |x, y| vec![(id(2), movement(x, y)), (id(4), movement(x, y))];
    let mut direct = session.begin_draft();
    apply_batch(&mut direct, batch(0.7, 0.6)).unwrap();
    let mut drag = session.begin_draft();
    for (x, y) in [(0.3, 0.4), (0.6, 0.3), (0.7, 0.6)] {
        apply_batch(&mut drag, batch(x, y)).unwrap();
    }
    for n in [2, 4] {
        assert_eq!(
            drag.document().unwrap().mesh(&id(n)),
            direct.document().unwrap().mesh(&id(n))
        );
    }
    assert_eq!(
        drag.document().unwrap().binding(&id(6)),
        direct.document().unwrap().binding(&id(6))
    );
    for n in [12, 22] {
        assert_eq!(
            drag.document().unwrap().blend_binding(&id(n)),
            direct.document().unwrap().blend_binding(&id(n))
        );
    }
    let accepted = drag.document().unwrap().clone();
    let source = drag.source();
    assert!(apply_batch(
        &mut drag,
        vec![(id(2), movement(0.5, 0.5)), (id(4), movement(2., 2.))]
    )
    .is_err());
    assert_eq!(drag.source(), source);
    assert_eq!(drag.document().unwrap().mesh(&id(2)), accepted.mesh(&id(2)));
    apply_batch(&mut drag, batch(0.7, 0.6)).unwrap();
    session
        .publish(drag.prepare("Manual mesh").unwrap())
        .unwrap();
    assert_eq!(
        {
            let history = session.history_state();
            (history.undo_steps, history.redo_steps)
        },
        (history.0 + 1, 0)
    );
    session.undo(session.version()).unwrap();
    for n in [2, 4] {
        assert_eq!(
            session.snapshot().document().mesh(&id(n)),
            before.mesh(&id(n))
        );
    }
    session.redo(session.version()).unwrap();
    for n in [2, 4] {
        assert_eq!(
            session.snapshot().document().mesh(&id(n)),
            accepted.mesh(&id(n))
        );
    }
}

#[test]
fn transfer_glue_deletion_and_outside_image_are_operation_rejections() {
    let id = dependencies::id;
    let s = mixed();
    let mut e = s.begin_draft();
    for vertex in [10, 70] {
        let err = prepare_manual(
            &e,
            &id(2),
            ManualMeshOperation::DeleteVertices { ids: vec![vertex] },
        )
        .unwrap_err();
        assert_eq!(err.code.as_str(), "GLUE_VERTEX_REFERENCED");
        assert!(err.message.contains(&id(2)));
    }
    let err = prepare_manual(
        &e,
        &id(2),
        ManualMeshOperation::InsertVertex {
            id: 200,
            uv: Vec2::new(-0.05, 0.3),
        },
    )
    .unwrap_err();
    assert_eq!(err.code.as_str(), "INVALID_MANUAL_MESH");
    apply_manual(
        &mut e,
        &id(2),
        ManualMeshOperation::DeleteVertices { ids: vec![120] },
    )
    .unwrap();
    apply_manual(
        &mut e,
        &id(2),
        ManualMeshOperation::ConstrainEdge { a: 30, b: 90 },
    )
    .unwrap();
    let err =
        prepare_manual(&e, &id(2), ManualMeshOperation::DeleteEdge { a: 10, b: 30 }).unwrap_err();
    assert_eq!(err.code.as_str(), "GLUE_VERTEX_REFERENCED");
    let b = e.document().unwrap().binding(&id(6)).unwrap();
    assert_eq!(b.keyforms[1].positions.len(), 4);
}
#[test]
fn transfer_locked_equal_dependencies_are_reads_but_resampling_requires_write_access() {
    let id = dependencies::id;
    let mut s = mixed();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut state = e.read_document().unwrap().editor_state().clone();
                state.objects.entry(id(6)).or_default().locked = true;
                state.objects.entry(id(30)).or_default().locked = true;
                e.replace_editor_state(state)
            })
            .and_then(|value| {
                let prepared = draft.prepare("lock dependencies")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let mut e = s.begin_draft();
    apply_manual(
        &mut e,
        &id(2),
        ManualMeshOperation::ConstrainEdge { a: 10, b: 120 },
    )
    .unwrap();
    let err = prepare_manual(
        &e,
        &id(2),
        ManualMeshOperation::InsertVertex {
            id: 200,
            uv: Vec2::new(0.35, 0.25),
        },
    )
    .unwrap_err();
    assert_eq!(err.code.as_str(), "OBJECT_LOCKED");
}
#[test]
fn transfer_budget_and_cancellation_reject_before_channel_allocation() {
    let s = mixed();
    let mesh = dependencies::id(2);
    let before = s.read_snapshot().source;
    assert_eq!(
        crate::support::prepared(&s, |c| c.edit_manual_mesh(
            &mesh,
            ManualMeshOperation::Reconnect,
            ConversionLimits {
                max_workspace_bytes: 1,
                ..Default::default()
            },
            &Default::default()
        ))
        .unwrap_err()
        .code
        .as_str(),
        "AUTHORING_MEMORY_LIMIT"
    );
    let control = ConversionControl::default();
    control.cancel();
    assert_eq!(
        crate::support::prepared(&s, |c| c.edit_manual_mesh(
            &mesh,
            ManualMeshOperation::Reconnect,
            Default::default(),
            &control
        ))
        .unwrap_err()
        .code
        .as_str(),
        "AUTHORING_CANCELLED"
    );
    assert_eq!(s.read_snapshot().source, before);
}

#[test]
fn transfer_workspace_shares_unrelated_records_and_honors_conversion_budget() {
    let mut s = session();
    let mut unrelated = s.snapshot().document().mesh(&id(2)).cloned().unwrap();
    unrelated.id = id(3);
    unrelated.runtime_id = id(3);
    // An unrelated record larger than the old 64 MiB / 6 admission ceiling.
    // Its data must remain shared through source capture, planning and history.
    unrelated.name = "x".repeat(12 * 1024 * 1024);
    // Publication must also reuse the unrelated mesh's large vertex index.
    // Rebuilding it alone exceeds this edit's 1 MiB temporary budget.
    unrelated.meshing = None;
    for vertex in 100..120_100 {
        unrelated.vertex_ids.push(vertex);
        unrelated.base_positions.push(Vec2::default());
        unrelated.uvs.push(Vec2::default());
    }
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.create_mesh(unrelated))
            .and_then(|value| {
                let prepared = draft.prepare("unrelated content")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let original = s.snapshot().clone();
    let limits = ConversionLimits {
        max_workspace_bytes: 1024 * 1024,
        ..Default::default()
    };
    let insertion = || ManualMeshOperation::InsertVertex {
        id: 50,
        uv: Vec2::new(0.4, 0.3),
    };
    for _ in 0..2 {
        let (outcome, p) = crate::support::prepared(&s, |c| {
            c.edit_manual_mesh(&id(2), insertion(), limits, &Default::default())
        })
        .unwrap();
        assert!(outcome.report.cost.workspace_bytes < limits.max_workspace_bytes);
        assert!(std::ptr::eq(
            original.document().mesh(&id(3)).unwrap(),
            p.snapshot().document().mesh(&id(3)).unwrap()
        ));
        s.publish(p).unwrap();
        assert_eq!(
            s.snapshot()
                .document()
                .mesh(&id(2))
                .unwrap()
                .vertex_ids
                .len(),
            5
        );
        s.undo(s.version()).unwrap();
        assert_eq!(s.snapshot().content_id(), original.content_id());
        s.redo(s.version()).unwrap();
        s.undo(s.version()).unwrap();
    }
    assert_eq!(
        crate::support::prepared(&s, |c| c.edit_manual_mesh(
            &id(2),
            insertion(),
            ConversionLimits {
                max_workspace_bytes: 1,
                ..limits
            },
            &Default::default()
        ))
        .unwrap_err()
        .code
        .as_str(),
        "AUTHORING_MEMORY_LIMIT"
    );
}

#[test]
fn transfer_batch_rejects_an_over_budget_mesh_without_installing_prior_work() {
    let s = mixed();
    let mut draft = s.begin_draft();
    let before = draft.read_snapshot().unwrap();
    let rejected = draft.prepare_preview(draft.source(), |c| {
        c.edit_manual_mesh(
            &dependencies::id(2),
            ManualMeshOperation::Reconnect,
            Default::default(),
            &Default::default(),
        )?;
        c.edit_manual_mesh(
            &dependencies::id(4),
            ManualMeshOperation::Reconnect,
            ConversionLimits {
                max_workspace_bytes: 1,
                ..Default::default()
            },
            &Default::default(),
        )
    });
    assert_eq!(
        rejected.unwrap_err().code.as_str(),
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(
        draft.read_snapshot().unwrap().snapshot.content_id(),
        before.snapshot.content_id()
    );
    assert_eq!(draft.read_snapshot().unwrap().source, before.source);
    apply_manual(
        &mut draft,
        &dependencies::id(2),
        ManualMeshOperation::Reconnect,
    )
    .unwrap();
}

#[test]
fn transfer_plans_reject_same_revision_branches_and_changed_dependencies() {
    let s = mixed();
    let mut a = s.begin_draft();
    let mut b = s.begin_draft();
    a.batch(a.source(), |c| {
        c.rename(
            zipzi_sdk::NameTarget::Mesh(&dependencies::id(2)),
            "A".into(),
        )
    })
    .unwrap();
    b.batch(b.source(), |c| {
        c.rename(
            zipzi_sdk::NameTarget::Mesh(&dependencies::id(2)),
            "B".into(),
        )
    })
    .unwrap();
    assert_eq!(a.source().sequence, b.source().sequence);
    let (p, _) = prepare_manual(&a, &dependencies::id(2), ManualMeshOperation::Reconnect).unwrap();
    assert_eq!(
        b.install_preview(p).unwrap_err().code.as_str(),
        "STALE_SOURCE"
    );
    let (p, _) = prepare_manual(&a, &dependencies::id(2), ManualMeshOperation::Reconnect).unwrap();
    a.batch(a.source(), |c| {
        c.rename(
            zipzi_sdk::NameTarget::Mesh(&dependencies::id(4)),
            "dependency".into(),
        )
    })
    .unwrap();
    assert_eq!(
        a.install_preview(p).unwrap_err().code.as_str(),
        "STALE_SOURCE"
    );
}

#[test]
fn transfer_affine_parent_subdivision_preserves_sampled_uv_position_field() {
    let id = dependencies::id;
    let mut s = mixed();
    let before = s.read_snapshot();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_manual_mesh(
                    &id(2),
                    ManualMeshOperation::InsertVertex {
                        id: 200,
                        uv: Vec2::new(0.35, 0.25),
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())
            })
            .and_then(|value| {
                let prepared = draft.prepare("subdivide")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
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
            let a = crate::support::frame(&before.snapshot, &values).unwrap();
            let b = crate::support::frame(s.snapshot(), &values).unwrap();
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
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
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
                    },
                )?;
                let mut rotation = e
                    .read_document()
                    .unwrap()
                    .transform(&id(3))
                    .unwrap()
                    .clone();
                rotation.parent_id = zipzi_core::TransformId::optional(id(50));
                e.replace_transform(rotation)
            })
            .and_then(|value| {
                let prepared = draft.prepare("warp ancestor")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let e = s.begin_draft();
    let plan = prepare_manual(
        &e,
        &id(2),
        ManualMeshOperation::InsertVertex {
            id: 200,
            uv: Vec2::new(0.35, 0.25),
        },
    )
    .unwrap();
    assert!(plan.1.report.non_affine_parent);
    assert!(plan
        .1
        .report
        .appearance_limits
        .iter()
        .any(|r| r.contains("Warp")));
    assert_eq!(
        s.snapshot().document().mesh(&id(2)).unwrap().deformer_id,
        id(3)
    );
}

#[test]
fn transfer_drag_net_noop_and_cancel_preserve_redo() {
    let id = dependencies::id;
    let mut s = mixed();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.rename(zipzi_sdk::NameTarget::Mesh(&id(2)), ("later").into())
            })
            .and_then(|value| {
                let prepared = draft.prepare("later")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    s.undo(s.version()).unwrap();
    let before = s.history_state();
    let mut e = s.begin_draft();
    for uv in [Vec2::new(0.6, 0.3), Vec2::new(0.4, 0.45)] {
        apply_gesture(
            &mut e,
            &id(2),
            ManualMeshOperation::MoveVertices {
                targets: vec![target(120, uv)],
                motion_from: None,
            },
        )
        .unwrap();
    }
    assert!(s
        .publish(e.prepare("Manual mesh").unwrap())
        .unwrap()
        .writes
        .is_empty());
    assert_eq!(s.history_state(), before);
    let mut e = s.begin_draft();
    apply_gesture(
        &mut e,
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
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let ids = e.read_document()?.mesh(&id(2)).unwrap().vertex_ids.clone();
                e.set_vertex_positions(
                    &id(2),
                    &ids.iter()
                        .map(|id| (*id, Vec2::default()))
                        .collect::<Vec<_>>(),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("collapsed base")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_manual_mesh(
                    &id(2),
                    ManualMeshOperation::InsertVertex {
                        id: 200,
                        uv: Vec2::new(0.35, 0.25),
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())
            })
            .and_then(|value| {
                let prepared = draft.prepare("UV insertion")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .mesh(&id(2))
            .cloned()
            .unwrap()
            .base_positions,
        vec![Vec2::default(); 6]
    );
}

#[test]
fn transfer_edited_shapes_roundtrip_through_external_export() {
    let id = dependencies::id;
    let mut s = mixed();
    let texture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_image(
                    e,
                    crate::support::image_fixture(&id(80), "Texture", &texture)?,
                )?;
                e.edit_scene(
                    SceneEdit::Delete {
                        ids: vec![id(32)],
                        scope: Default::default(),
                    },
                    Default::default(),
                    &Default::default(),
                )?; // External acceptance uses fully configured pairs.
                for n in [2, 4] {
                    let mut m = e.read_document().unwrap().mesh(&id(n)).unwrap().clone();
                    m.texture_asset_id = id(80).into();
                    e.replace_mesh(m)?;
                }
                e.edit_manual_mesh(
                    &id(2),
                    ManualMeshOperation::InsertVertex {
                        id: 200,
                        uv: Vec2::new(0.35, 0.25),
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())
            })
            .and_then(|value| {
                let prepared = draft.prepare("configure export")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.read_snapshot();
    let artifact = zipzi_project::encode_document_cmo3(
        s.snapshot().document(),
        s.snapshot().resources(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(&artifact.bytes[..4], b"CAFF");
    assert!(!artifact.report.has_loss());
    assert_eq!(s.snapshot().content_id(), before.snapshot.content_id());
    let root = std::env::temp_dir().join(format!("zipzi-manual-export-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("model.cmo3");
    std::fs::write(&path, artifact.bytes).unwrap();
    let mut imported = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(&id(90), (ModelSetup::default()).into()).unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    let loaded = zipzi_project::ProjectImporter::default()
        .import_cubism_authoring(&path)
        .unwrap();
    imported
        .replace(
            imported.version(),
            zipzi_sdk::Snapshot::new(loaded.document, loaded.resources).unwrap(),
        )
        .unwrap();
    assert_eq!(imported.snapshot().document().mesh_order().len(), 2);
    for parameter in [0., 0.5, 1.] {
        let values: zipzi_core::PreviewValues = [(id(5), parameter), (id(10), 0.3), (id(20), 0.7)]
            .into_iter()
            .collect();
        let remapped = values
            .iter()
            .map(|(id, v)| {
                let p = imported
                    .snapshot()
                    .document()
                    .parameter_order()
                    .iter()
                    .filter_map(|p| imported.snapshot().document().parameter(p))
                    .find(|p| p.runtime_id == *id)
                    .unwrap();
                (p.id.clone(), *v)
            })
            .collect();
        let a = crate::support::frame(&before.snapshot, &values).unwrap();
        let b = crate::support::frame(imported.snapshot(), &remapped).unwrap();
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
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut state = e.read_document().unwrap().editor_state().clone();
                state.objects.entry(id(32)).or_default().locked = true;
                e.replace_editor_state(state)
            })
            .and_then(|value| {
                let prepared = draft.prepare("lock carried Glue")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.read_snapshot();
    let (outcome, prepared) = crate::support::prepared(&s, |c| {
        c.edit_manual_mesh(
            &id(2),
            ManualMeshOperation::InsertVertex {
                id: 200,
                uv: Vec2::new(0.35, 0.25),
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    assert_eq!(outcome.report.added, 1);
    assert_eq!(
        prepared.snapshot().document().glue(&id(32)),
        before.snapshot.document().glue(&id(32))
    );
    s.publish(prepared).unwrap();
    let committed = s.read_snapshot();
    let mut glue = committed.snapshot.document().glue(&id(30)).unwrap().clone();
    glue.runtime_id = id(32);
    let error = crate::support::prepared(&s, |c| c.replace_glue(glue)).unwrap_err();
    assert_eq!(error.code.as_str(), "DUPLICATE_RUNTIME_ID");
    assert_eq!(s.read_snapshot().source, committed.source);
}

#[test]
fn transfer_new_move_selection_retains_previous_completed_positions() {
    let id = dependencies::id;
    let s = mixed();
    let mut e = s.begin_draft();
    apply_manual(
        &mut e,
        &id(2),
        ManualMeshOperation::MoveVertices {
            targets: vec![target(120, Vec2::new(0.6, 0.3))],
            motion_from: None,
        },
    )
    .unwrap();
    let accepted = e.document().unwrap().mesh(&id(2)).unwrap().clone();
    apply_manual(
        &mut e,
        &id(2),
        ManualMeshOperation::MoveVertices {
            targets: vec![target(10, Vec2::new(0.12, 0.12))],
            motion_from: None,
        },
    )
    .unwrap();
    let current = e.document().unwrap().mesh(&id(2)).unwrap();
    assert_eq!(current.uvs[4], accepted.uvs[4]);
    assert_eq!(current.base_positions[4], accepted.base_positions[4]);
}

#[test]
fn transfer_generated_topology_cannot_bypass_the_complete_image_rectangle() {
    let s = mixed();
    let mut g = geometry(s.snapshot().document(), &dependencies::id(2));
    g.uvs[0].x = -0.1;
    let error = crate::support::prepared(&s, |c| {
        c.remesh(
            &dependencies::id(2),
            g,
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap_err();
    assert_eq!(error.code.as_str(), "UV_OUTSIDE_SOURCE_IMAGE");
    assert!(error.message.contains("uvs[0]"));
}

#[cfg(feature = "bench-instrumentation")]
#[test]
fn transfer_preview_installs_the_checked_snapshot() {
    let id = dependencies::id;
    let s = mixed();
    let mut edit = s.begin_draft();
    let movement = |x, y| ManualMeshOperation::MoveVertices {
        targets: vec![target(120, Vec2::new(x, y))],
        motion_from: None,
    };
    // Source admission is separate from the repeated-frame cost.
    let first = prepare_manual(&edit, &id(2), movement(0.45, 0.4)).unwrap();
    edit.install_preview(first.0).unwrap();
    let capture = zipzi_core::benchmark::Capture::start();
    let plan = prepare_manual(&edit, &id(2), movement(0.47, 0.41)).unwrap();
    let geometry = geometry(plan.0.read_snapshot().unwrap().document(), &id(2));
    let stages = capture.finish();
    assert!(stages["core.validate_mesh_meshing"].calls > 0);
    let capture = zipzi_core::benchmark::Capture::start();
    edit.install_preview(plan.0).unwrap();
    assert!(!capture.finish().contains_key("core.validate_mesh_meshing"));
    let actual = edit.document().unwrap().mesh(&id(2)).unwrap();
    assert_eq!(actual.uvs, geometry.uvs);
    assert_eq!(actual.triangles, geometry.triangles);
    assert_eq!(actual.base_positions, geometry.positions);
    assert!(edit.is_active());
}

fn geometry(document: &zipzi_core::state::DocumentState, id: &str) -> MeshGeometry {
    let m = document.mesh(id).unwrap();
    MeshGeometry {
        vertex_ids: m.vertex_ids.clone(),
        positions: m.base_positions.clone(),
        uvs: m.uvs.clone(),
        triangles: m.triangles.clone(),
        meshing: m.meshing.clone(),
    }
}
fn prepare_manual(
    draft: &Draft,
    id: &str,
    operation: ManualMeshOperation,
) -> Result<(PreparedPreview, ManualMeshOutcome), Status> {
    draft.prepare_preview(draft.source(), |c| {
        c.edit_manual_mesh(id, operation, Default::default(), &Default::default())
    })
}
fn apply_gesture(
    draft: &mut Draft,
    id: &str,
    operation: ManualMeshOperation,
) -> Result<(), Status> {
    let (preview, _) = draft.prepare_baseline_preview(draft.source(), |c| {
        c.edit_manual_mesh(id, operation, Default::default(), &Default::default())
    })?;
    draft.install_preview(preview)?;
    Ok(())
}
fn apply_manual(
    draft: &mut Draft,
    id: &str,
    operation: ManualMeshOperation,
) -> Result<ManualMeshOutcome, Status> {
    let (p, outcome) = prepare_manual(draft, id, operation)?;
    draft.install_preview(p)?;
    Ok(outcome)
}
fn apply_batch(
    draft: &mut Draft,
    operations: Vec<(String, ManualMeshOperation)>,
) -> Result<Vec<ManualMeshOutcome>, Status> {
    let (p, outcomes) = draft.prepare_baseline_preview(draft.source(), |c| {
        operations
            .into_iter()
            .map(|(id, op)| c.edit_manual_mesh(&id, op, Default::default(), &Default::default()))
            .collect::<Result<Vec<_>, _>>()
    })?;
    draft.install_preview(p)?;
    Ok(outcomes)
}

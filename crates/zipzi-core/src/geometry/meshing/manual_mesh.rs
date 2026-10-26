use super::*;
use crate::{MeshConstraint, MeshConstraintKind};

fn square() -> (Vec<u32>, Vec<Vec2>, Vec<[u32; 3]>, MeshMeshing) {
    (
        vec![7, 2, 9, 4],
        vec![
            Vec2::new(0., 0.),
            Vec2::new(1., 0.),
            Vec2::new(1., 1.),
            Vec2::new(0., 1.),
        ],
        vec![[7, 2, 9], [7, 9, 4]],
        MeshMeshing {
            boundary: vec![2, 9, 4, 7],
            constraints: vec![],
        },
    )
}

#[test]
fn canonical_boundary_and_both_triangle_windings() {
    let (ids, uvs, mut faces, mut meshing) = square();
    meshing.constraints.push(MeshConstraint {
        endpoints: [7, 9],
        kind: MeshConstraintKind::EdgeSupport,
    });
    assert!(validate_mesh_meshing(&ids, &uvs, &faces, &meshing).is_ok());
    for f in &mut faces {
        f.swap(0, 2);
    }
    assert!(validate_mesh_meshing(&ids, &uvs, &faces, &meshing).is_ok());
    meshing.constraints[0].kind = MeshConstraintKind::User;
    assert!(validate_mesh_meshing(&ids, &uvs, &faces, &meshing).is_ok());
    meshing.boundary.rotate_left(1);
    assert!(!validate_mesh_meshing(&ids, &uvs, &faces, &meshing).is_ok());
}

#[test]
fn rejects_missing_faces_duplicates_unused_vertices_and_invalid_constraints() {
    let (ids, uvs, faces, meshing) = square();
    for bad in [
        vec![],
        vec![faces[0]],
        vec![faces[0], faces[0]],
        vec![faces[0], [7, 4, 9]],
    ] {
        assert!(!validate_mesh_meshing(&ids, &uvs, &bad, &meshing).is_ok());
    }
    for constraints in [
        vec![[2, 4]],
        vec![[2, 9]],
        vec![[9, 7]],
        vec![[7, 9], [7, 9]],
    ] {
        let mut m = meshing.clone();
        m.constraints = constraints
            .into_iter()
            .map(|endpoints| MeshConstraint {
                endpoints,
                kind: MeshConstraintKind::User,
            })
            .collect();
        assert!(!validate_mesh_meshing(&ids, &uvs, &faces, &m).is_ok());
    }
    let mut extended_ids = ids;
    extended_ids.push(12);
    let mut extended_uvs = uvs;
    extended_uvs.push(Vec2::new(0.5, 0.5));
    assert!(!validate_mesh_meshing(&extended_ids, &extended_uvs, &faces, &meshing).is_ok());
}

#[test]
fn rejects_bow_ties_t_junctions_and_f32_collapse() {
    let (ids, mut uvs, faces, meshing) = square();
    uvs.swap(1, 2);
    assert!(!validate_mesh_meshing(&ids, &uvs, &faces, &meshing).is_ok());
    let ids = vec![0, 1, 2, 3, 4];
    let uvs = vec![
        Vec2::new(0., 0.),
        Vec2::new(1., 0.),
        Vec2::new(1., 1.),
        Vec2::new(0., 1.),
        Vec2::new(0.5, 0.5),
    ];
    let m = MeshMeshing {
        boundary: vec![0, 1, 2, 3],
        constraints: vec![],
    };
    assert!(!validate_mesh_meshing(&ids, &uvs, &[[0, 1, 2], [0, 4, 3], [4, 2, 3]], &m).is_ok());
    let collapsed = uvs
        .iter()
        .map(|p| Vec2::new(p.x + 100_000_000., p.y))
        .collect::<Vec<_>>();
    assert!(!validate_mesh_meshing(
        &ids,
        &collapsed,
        &[[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]],
        &m
    )
    .is_ok());
}

#[test]
fn concavity_and_inner_support_loop_are_not_holes() {
    let ids = vec![0, 1, 2, 3, 4];
    let uvs = vec![
        Vec2::new(0., 0.),
        Vec2::new(2., 0.),
        Vec2::new(2., 2.),
        Vec2::new(1., 1.),
        Vec2::new(0., 2.),
    ];
    let m = MeshMeshing {
        boundary: ids.clone(),
        constraints: vec![],
    };
    assert!(validate_mesh_meshing(&ids, &uvs, &[[0, 1, 3], [1, 2, 3], [0, 3, 4]], &m).is_ok());
    assert!(!validate_mesh_meshing(&ids, &uvs, &[[0, 1, 2], [0, 2, 4]], &m).is_ok());
    let ids = (0..7).collect::<Vec<_>>();
    let uvs = vec![
        Vec2::new(0., 0.),
        Vec2::new(4., 0.),
        Vec2::new(4., 4.),
        Vec2::new(0., 4.),
        Vec2::new(1., 1.),
        Vec2::new(3., 1.),
        Vec2::new(2., 3.),
    ];
    let m = MeshMeshing {
        boundary: vec![0, 1, 2, 3],
        constraints: [[4, 5], [4, 6], [5, 6]]
            .map(|endpoints| MeshConstraint {
                endpoints,
                kind: MeshConstraintKind::EdgeSupport,
            })
            .to_vec(),
    };
    let faces = [
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 5],
        [2, 6, 5],
        [2, 3, 6],
        [3, 4, 6],
        [3, 0, 4],
        [4, 5, 6],
    ];
    assert!(validate_mesh_meshing(&ids, &uvs, &faces, &m).is_ok());
    assert!(!validate_mesh_meshing(&ids, &uvs, &faces[..7], &m).is_ok());
}

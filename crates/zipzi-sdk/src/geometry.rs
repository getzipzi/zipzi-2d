//! Deterministic authoring geometry and dependency-preserving remeshing.
use crate::ConversionControl;
use crate::Status;
use std::collections::HashSet;
pub use zipzi_contract::spatial::ObjectTarget;
use zipzi_core::{Mesh, Part, Vec2};

pub use zipzi_contract::authoring::MeshGeometry;

fn invalid(message: &str) -> Status {
    Status::error("INVALID_RECTANGLE_GRID", message)
}

fn sample(values: &[Vec2], columns: usize, rows: usize) -> Result<Vec<Vec2>, Status> {
    if values.len() != 4 || values.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
        return Err(invalid("rectangle values must contain four finite points"));
    }
    let mut result = Vec::with_capacity((columns + 1) * (rows + 1));
    for row in 0..=rows {
        let v = row as f64 / rows as f64;
        for column in 0..=columns {
            let u = column as f64 / columns as f64;
            let weights = if u >= v {
                [1.0 - u, u - v, v, 0.0]
            } else {
                [1.0 - v, 0.0, u, v - u]
            };
            let x: f64 = values
                .iter()
                .zip(weights)
                .map(|(p, w)| p.x as f64 * w)
                .sum();
            let y: f64 = values
                .iter()
                .zip(weights)
                .map(|(p, w)| p.y as f64 * w)
                .sum();
            result.push(Vec2::new(x as f32, y as f32));
        }
    }
    Ok(result)
}

/// Subdivide a canonical axis-aligned rectangle, retaining corner IDs and
/// the original two-triangle diagonal. Does not mutate the source.
pub fn rectangle_grid_geometry(
    source: &MeshGeometry,
    columns: usize,
    rows: usize,
) -> Result<MeshGeometry, Status> {
    if source.meshing.is_some() {
        return Err(invalid(
            "rectangle grid cannot discard existing modeling topology",
        ));
    }
    let count = columns
        .checked_add(1)
        .and_then(|c| rows.checked_add(1).and_then(|r| c.checked_mul(r)));
    if columns == 0 || rows == 0 || count.is_none_or(|n| n > 65_536) {
        return Err(invalid(
            "columns and rows must be positive; grid cannot exceed 65536 vertices",
        ));
    }
    let count = count.unwrap();
    let ids = &source.vertex_ids;
    if ids.len() != 4
        || ids.iter().collect::<std::collections::HashSet<_>>().len() != 4
        || source.positions.len() != 4
        || source.uvs.len() != 4
    {
        return Err(invalid(
            "rectangle grid requires exactly four unique corner vertices",
        ));
    }
    if source.triangles != [[ids[0], ids[1], ids[2]], [ids[0], ids[2], ids[3]]] {
        return Err(invalid(
            "rectangle grid requires the standard two-triangle diagonal",
        ));
    }
    if source
        .positions
        .iter()
        .chain(&source.uvs)
        .any(|p| !p.x.is_finite() || !p.y.is_finite())
    {
        return Err(invalid("rectangle vertices and UVs must be finite"));
    }
    for (p, (x, y)) in source
        .uvs
        .iter()
        .zip([(0., 0.), (1., 0.), (1., 1.), (0., 1.)])
    {
        if (p.x - x).abs() > 1e-6 || (p.y - y).abs() > 1e-6 {
            return Err(invalid("rectangle grid requires canonical corner UVs"));
        }
    }
    let [a, b, c, d] = source.positions[..] else {
        unreachable!()
    };
    let tolerance = ((c.x as f64 - a.x as f64)
        .max(c.y as f64 - a.y as f64)
        .max(1.0)
        * 1e-6) as f32;
    if a.x >= c.x
        || a.y >= c.y
        || (b.x - c.x).abs() > tolerance
        || (b.y - a.y).abs() > tolerance
        || (d.x - a.x).abs() > tolerance
        || (d.y - c.y).abs() > tolerance
    {
        return Err(invalid(
            "rectangle grid requires axis-aligned corner positions",
        ));
    }
    let max_id = *ids.iter().max().unwrap();
    if max_id.checked_add((count - 4) as u32).is_none() {
        return Err(invalid("rectangle grid vertex IDs would overflow u32"));
    }
    let mut next = max_id as u64 + 1;
    let vertex_ids: Vec<_> = (0..count)
        .map(|i| {
            if i == 0 {
                ids[0]
            } else if i == columns {
                ids[1]
            } else if i == count - 1 {
                ids[2]
            } else if i == rows * (columns + 1) {
                ids[3]
            } else {
                let id = next as u32;
                next += 1;
                id
            }
        })
        .collect();
    let mut triangles = Vec::with_capacity(columns * rows * 2);
    for row in 0..rows {
        for column in 0..columns {
            let a = row * (columns + 1) + column;
            let b = a + 1;
            let d = a + columns + 1;
            let c = d + 1;
            triangles.extend([
                [vertex_ids[a], vertex_ids[b], vertex_ids[c]],
                [vertex_ids[a], vertex_ids[c], vertex_ids[d]],
            ]);
        }
    }
    Ok(MeshGeometry {
        vertex_ids,
        positions: sample(&source.positions, columns, rows)?,
        uvs: sample(&source.uvs, columns, rows)?,
        triangles,
        meshing: None,
    })
}

pub fn rectangle_mesh(
    id: &str,
    name: &str,
    asset_id: &str,
    min: Vec2,
    max: Vec2,
) -> Result<Mesh, Status> {
    if !min.x.is_finite()
        || !min.y.is_finite()
        || !max.x.is_finite()
        || !max.y.is_finite()
        || min.x >= max.x
        || min.y >= max.y
    {
        return Err(Status::error(
            "INVALID_RECTANGLE",
            "Finite min must be below max",
        ));
    }
    Ok(Mesh {
        id: id.into(),
        runtime_id: id.into(),
        name: name.into(),
        texture_asset_id: asset_id.into(),
        vertex_ids: vec![0, 1, 2, 3],
        base_positions: vec![min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        ..Mesh::default()
    })
}

/// Resolve target IDs in authoring mesh order. Missing IDs are errors; an
/// existing Part without descendant meshes resolves to an empty vector.
pub fn resolve_mesh_ids(
    meshes: &[Mesh],
    parts: &[Part],
    targets: &[ObjectTarget],
) -> Result<Vec<String>, Status> {
    resolve_mesh_ids_from(
        meshes.iter(),
        parts.iter(),
        targets,
        &ConversionControl::default(),
    )
}

fn resolve_mesh_ids_from<'a>(
    meshes: impl Iterator<Item = &'a Mesh> + Clone,
    parts: impl Iterator<Item = &'a Part> + Clone,
    targets: &[ObjectTarget],
    control: &ConversionControl,
) -> Result<Vec<String>, Status> {
    let mesh_by_id: HashSet<_> = meshes.clone().map(|mesh| mesh.id.as_str()).collect();
    let part_by_id: HashSet<_> = parts.clone().map(|part| part.id.as_str()).collect();
    let mut selected: HashSet<&str> = HashSet::new();
    let mut descendants: HashSet<&str> = HashSet::new();
    for target in targets {
        control.check()?;
        match target {
            ObjectTarget::Mesh(id) => {
                if !mesh_by_id.contains(id.as_str()) {
                    return Err(Status::error("MISSING_MESH", id));
                }
                selected.insert(id.as_str());
            }
            ObjectTarget::Part(id) => {
                if !part_by_id.contains(id.as_str()) {
                    return Err(Status::error("MISSING_PART", id));
                }
                descendants.insert(id.as_str());
            }
        }
    }
    loop {
        control.check()?;
        let before = descendants.len();
        for part in parts.clone() {
            if descendants.contains(part.parent_id.as_str()) {
                descendants.insert(part.id.as_str());
            }
        }
        if descendants.len() == before {
            break;
        }
    }
    for mesh in meshes.clone() {
        if descendants.contains(mesh.part_id.as_str()) {
            selected.insert(mesh.id.as_str());
        }
    }
    Ok(meshes
        .clone()
        .filter(|mesh| selected.contains(mesh.id.as_str()))
        .map(|mesh| mesh.id.clone())
        .collect())
}

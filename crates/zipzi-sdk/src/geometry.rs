//! Deterministic authoring geometry and dependency-preserving remeshing.
use crate::{AuthoringSession, EditReceipt, SdkError, TopologyReplacement, Version};
use zipzi_core::{BlendShapeTargetKind, DeltaKeyforms, Vec2, VertexId};

zipzi_core::api_record! {
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
pub struct MeshGeometry {
    /// Stable vertex IDs in the same order as positions and UVs; IDs need not be contiguous.
    pub vertex_ids: Vec<VertexId>,
    /// Absolute XY coordinates in the mesh authoring space (model space without a deformation
    /// parent, otherwise the parent's local/parameter space), ordered by vertex_ids.
    pub positions: Vec<Vec2>,
    /// Texture UV coordinates ordered by vertex_ids.
    pub uvs: Vec<Vec2>,
    /// Triangle entries reference vertex_ids values, never offsets into positions.
    pub triangles: Vec<[VertexId; 3]>,
    /// Optional canonical UV boundary and retained internal structure edges.
    #[serde(default)]
    pub meshing: Option<zipzi_core::MeshMeshing>,
}
}

fn invalid(message: &str) -> SdkError {
    SdkError::new("INVALID_RECTANGLE_GRID", message, "rectangle_grid_geometry")
}

fn sample(values: &[Vec2], columns: usize, rows: usize) -> Result<Vec<Vec2>, SdkError> {
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
) -> Result<MeshGeometry, SdkError> {
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

impl AuthoringSession {
    /// Subdivide a rectangle and migrate mesh keyforms, mesh BlendShapes and
    /// corner glue in one undoable edit. Bound grids must be square so the
    /// source diagonal remains an edge and piecewise-affine poses are preserved.
    pub fn remesh_rectangle_grid(
        &mut self,
        mesh_id: &str,
        columns: usize,
        rows: usize,
        expected: Option<Version>,
    ) -> Result<EditReceipt, SdkError> {
        self.begin_remesh_rectangle_grid(mesh_id, columns, rows, expected)?
            .commit_to(self)
    }

    /// Stage the same topology operation for a host-controlled publication.
    pub fn begin_remesh_rectangle_grid(
        &self,
        mesh_id: &str,
        columns: usize,
        rows: usize,
        expected: Option<Version>,
    ) -> Result<crate::EditSession<'static>, SdkError> {
        let mut edit = self.begin_owned_edit("remesh rectangle grid", expected)?;
        edit.remesh_rectangle_grid(mesh_id, columns, rows)?;
        Ok(edit)
    }
}
impl crate::EditSession<'_> {
    /// Generate topology and migrate ordinary/Blend keyforms in the current candidate.
    pub fn remesh_rectangle_grid(
        &mut self,
        mesh_id: &str,
        columns: usize,
        rows: usize,
    ) -> Result<(), SdkError> {
        self.ensure_active("remesh_rectangle_grid")?;
        let replacement = (|| {
            let doc = self.candidate_document();
            let mut mesh = doc.get_mesh(mesh_id).cloned().ok_or_else(|| {
                SdkError::new("NOT_FOUND", "unknown mesh ID", "remesh_rectangle_grid")
            })?;
            let space = if mesh.deformer_id.is_empty() {
                crate::GeometrySpace::Model
            } else if doc
                .get_transform(&mesh.deformer_id)
                .is_some_and(|t| t.warp().is_some())
            {
                crate::GeometrySpace::WarpParameter(mesh.deformer_id.clone())
            } else {
                crate::GeometrySpace::RotationLocal(mesh.deformer_id.clone())
            };
            let source = crate::GeometrySnapshot {
                version: self.before,
                mesh_id: mesh_id.into(),
                vertex_ids: mesh.vertex_ids.clone(),
                positions: mesh.base_positions.clone(),
                uvs: mesh.uvs.clone(),
                triangles: mesh.triangles.clone(),
                meshing: mesh.meshing.clone(),
                space,
            };
            let geometry = rectangle_grid_geometry(
                &MeshGeometry {
                    vertex_ids: mesh.vertex_ids.clone(),
                    positions: mesh.base_positions.clone(),
                    uvs: mesh.uvs.clone(),
                    triangles: mesh.triangles.clone(),
                    meshing: mesh.meshing.clone(),
                },
                columns,
                rows,
            )?;
            let mut binding = doc
                .binding_order()
                .iter()
                .filter_map(|id| doc.get_binding(id))
                .find(|b| b.mesh_id == mesh_id)
                .cloned();
            let mut blends: Vec<_> = doc
                .blend_binding_order()
                .iter()
                .filter_map(|id| doc.get_blend_binding(id))
                .filter(|b| b.target_id == mesh_id)
                .cloned()
                .collect();
            if (binding.is_some() || !blends.is_empty()) && columns != rows {
                return Err(invalid(
                    "bound rectangle grids require equal columns and rows",
                ));
            }
            if let Some(binding) = &mut binding {
                for form in &mut binding.keyforms {
                    form.positions = sample(&form.positions, columns, rows)?;
                }
            }
            for blend in &mut blends {
                if blend.target_kind != BlendShapeTargetKind::Mesh {
                    return Err(invalid("unsupported BlendShape target on rectangle mesh"));
                }
                let DeltaKeyforms::Mesh(forms) = &mut blend.keyforms else {
                    return Err(invalid("unsupported BlendShape keyform on rectangle mesh"));
                };
                for form in forms {
                    form.positions = sample(&form.positions, columns, rows)?;
                }
            }
            let glues = doc
                .glue_order()
                .iter()
                .filter_map(|id| doc.get_glue(id))
                .filter(|g| g.mesh_a_id == mesh_id || g.mesh_b_id == mesh_id)
                .cloned()
                .collect();
            mesh.vertex_ids = geometry.vertex_ids;
            mesh.base_positions = geometry.positions;
            mesh.uvs = geometry.uvs;
            mesh.triangles = geometry.triangles;
            mesh.meshing = geometry.meshing;
            let replacement = TopologyReplacement {
                mesh,
                binding,
                blend_bindings: blends,
                glues,
                vertex_mapping: source.vertex_ids.iter().map(|&id| (id, Some(id))).collect(),
            };
            Ok((source, replacement))
        })()
        .map_err(|e| self.abort_with(e))?;
        self.replace_topology(&replacement.0, replacement.1)
    }
}

// SPDX-License-Identifier: GPL-3.0-only
//! Fill authored domains without tracing alpha or moving their existing vertices.
use super::*;
use zipzi_sdk::AffineUvMap;

pub fn fill_existing_geometry(
    source: &MeshGeometry,
    width: u32,
    height: u32,
    settings: &MeshSettings,
) -> Result<MeshGeometry, MeshError> {
    settings.validate()?;
    if width == 0 || height == 0 {
        return Err(MeshError::Input);
    }
    let map = AffineUvMap::from_geometry(source)
        .map_err(|_| MeshError::Geometry("non-affine image mapping"))?;
    let slots: BTreeMap<_, _> = source
        .vertex_ids
        .iter()
        .copied()
        .enumerate()
        .map(|(i, id)| (id, i))
        .collect();
    if slots.len() != source.vertex_ids.len()
        || source.vertex_ids.len() != source.uvs.len()
        || source.triangles.is_empty()
        || source
            .triangles
            .iter()
            .flatten()
            .any(|id| !slots.contains_key(id))
    {
        return Err(MeshError::Input);
    }
    let original_uv = source.triangles[0].map(|id| {
        let uv = source.uvs[slots[&id]];
        [uv.x as f64, uv.y as f64]
    });
    let reverse = cross(original_uv[0], original_uv[1], original_uv[2]) > 0.;
    let points: Vec<Point> = source
        .uvs
        .iter()
        .map(|p| snap([p.x as f64 * width as f64, (1. - p.y as f64) * height as f64]))
        .collect();
    let mut mesh = Mesh {
        points: points.clone(),
        triangles: vec![],
    };
    for t in &source.triangles {
        let mut t = t.map(|id| slots[&id]);
        if cross(points[t[0]], points[t[1]], points[t[2]]) < 0. {
            t.swap(1, 2);
        }
        mesh.triangles.push(t);
    }
    let boundaries = trace_boundaries(&mesh)?;
    let loops: Loops = boundaries
        .iter()
        .map(|r| r.iter().map(|&i| points[i as usize]).collect())
        .collect();
    let mut result = source.clone();
    result.triangles.clear();
    let mut allocator = zipzi_sdk::ManualVertexIdAllocator::new(source);
    let spacing = settings.interior_spacing.max(6.);
    let mut assigned = BTreeSet::new();
    for (outer, row) in loops.iter().enumerate().filter(|(_, r)| area(r) > 0.) {
        let mut domain = vec![row.clone()];
        let mut original = boundaries[outer]
            .iter()
            .map(|&i| i as usize)
            .collect::<Vec<_>>();
        for (i, hole) in loops.iter().enumerate().filter(|(_, r)| area(r) < 0.) {
            let owner = loops
                .iter()
                .enumerate()
                .filter(|(_, r)| area(r) > 0. && inside(hole[0], r))
                .min_by(|(_, a), (_, b)| area(a).total_cmp(&area(b)))
                .map(|(i, _)| i);
            if owner == Some(outer) {
                domain.push(hole.clone());
                original.extend(boundaries[i].iter().map(|&i| i as usize));
            }
        }
        let proposal = fill::generate(
            &domain,
            settings.fill_algorithm,
            settings.fill_parameters.get(settings.fill_algorithm),
            spacing,
            settings.suppress_boundary_diagonals,
            false,
        )
        .ok_or(MeshError::Geometry("could not fill existing contour"))?;
        let mut layout = proposal;
        let mut ids: Vec<Option<u32>> = original
            .iter()
            .map(|&i| Some(source.vertex_ids[i]))
            .collect();
        ids.resize(layout.points.len(), None);
        let mut local_slots: BTreeMap<u32, usize> = original
            .iter()
            .enumerate()
            .map(|(i, &slot)| (source.vertex_ids[slot], i))
            .collect();
        assigned.extend(original.iter().copied());
        for (slot, &p) in points.iter().enumerate() {
            if local_slots.contains_key(&source.vertex_ids[slot]) || !in_domain(p, &domain) {
                continue;
            }
            let i = if let Some(i) = layout.points.iter().position(|&q| distance(p, q) < 1e-5) {
                i
            } else {
                layout.points.push(p);
                ids.push(None);
                layout.points.len() - 1
            };
            layout.points[i] = p;
            ids[i] = Some(source.vertex_ids[slot]);
            local_slots.insert(source.vertex_ids[slot], i);
            assigned.insert(slot);
        }
        let fixed: Vec<[usize; 2]> = source
            .meshing
            .iter()
            .flat_map(|m| &m.constraints)
            .filter_map(|c| {
                Some([
                    *local_slots.get(&c.endpoints[0])?,
                    *local_slots.get(&c.endpoints[1])?,
                ])
            })
            .collect();
        // User constraints take precedence over a proposed pattern. Free the
        // intersected cells, keeping the remaining patterned core intact.
        layout.triangles.retain(|t| {
            !fixed.iter().any(|e| {
                edges(*t).iter().any(|side| {
                    intersects(
                        layout.points[e[0]],
                        layout.points[e[1]],
                        layout.points[side[0]],
                        layout.points[side[1]],
                    )
                })
            })
        });
        let out = triangulate::with_edges(
            &domain,
            &layout,
            &[],
            spacing,
            settings.suppress_boundary_diagonals,
            &fixed,
        )
        .ok_or(MeshError::Geometry(
            "could not combine fill with authored vertices and constraints",
        ))?;
        ids.resize(out.points.len(), None);
        for (i, &p) in out.points.iter().enumerate() {
            if ids[i].is_none() {
                let id = allocator.allocate(&result).map_err(|_| MeshError::Limit)?;
                let uv = Vec2::new(
                    (p[0] / width as f64) as f32,
                    (1. - p[1] / height as f64) as f32,
                );
                result.vertex_ids.push(id);
                result.uvs.push(uv);
                result.positions.push(map.position(uv));
                ids[i] = Some(id);
            }
        }
        result.triangles.extend(out.triangles.iter().map(|t| {
            let t = if reverse { [t[0], t[2], t[1]] } else { *t };
            t.map(|i| ids[i].unwrap())
        }));
    }
    if assigned.len() != source.vertex_ids.len() || result.triangles.is_empty() {
        return Err(MeshError::Geometry("unassigned source vertices"));
    }
    if result.vertex_ids.len() == source.vertex_ids.len() {
        return Ok(source.clone());
    }
    if result.vertex_ids.len() > settings.max_vertices {
        return Err(MeshError::Limit);
    }
    if result.meshing.is_some() {
        extract_manual_meshing(&result)
            .map_err(|_| MeshError::Geometry("filled topology is not editable"))?;
    }
    Ok(result)
}

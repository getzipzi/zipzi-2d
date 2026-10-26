use super::*;

pub(crate) fn mesh_from_record(
    data: MeshRecordDataTuple,
    runtime_id: String,
) -> PyResult<zipzi_core::Mesh> {
    let (id, name, texture_asset_id, geometry, (part_id, deformer_id), appearance, drawing) = data;
    let (vertex_ids, positions, uvs, triangles, meshing) = geometry;
    let (draw_order, blend_mode, enabled, double_sided, inverted_mask, masks, raw_blend_mode) =
        drawing;
    Ok(zipzi_core::Mesh {
        id,
        name,
        texture_asset_id: texture_asset_id.into(),
        vertex_ids,
        base_positions: positions
            .into_iter()
            .map(|(x, y)| Vec2::new(x, y))
            .collect(),
        uvs: uvs.into_iter().map(|(x, y)| Vec2::new(x, y)).collect(),
        triangles: triangles.into_iter().map(|(a, b, c)| [a, b, c]).collect(),
        runtime_id,
        part_id,
        deformer_id,
        appearance: appearance_from_tuple(appearance),
        draw_order,
        blend_mode: blend_mode_from_name(&blend_mode)?,
        enabled,
        double_sided,
        inverted_mask,
        masks,
        raw_blend_mode,
        meshing: meshing_from_tuple(meshing)?,
    })
}

pub(crate) fn mesh_record_tuple(mesh: zipzi_core::Mesh, version: Version) -> MeshRecordTuple {
    (
        (
            mesh.id,
            mesh.name,
            mesh.texture_asset_id.as_str().to_owned(),
            (
                mesh.vertex_ids,
                mesh.base_positions
                    .into_iter()
                    .map(|p| (p.x, p.y))
                    .collect(),
                mesh.uvs.into_iter().map(|p| (p.x, p.y)).collect(),
                mesh.triangles
                    .into_iter()
                    .map(|v| (v[0], v[1], v[2]))
                    .collect(),
                meshing_tuple(mesh.meshing),
            ),
            (mesh.part_id, mesh.deformer_id),
            appearance_tuple(mesh.appearance),
            (
                mesh.draw_order,
                blend_mode_name(mesh.blend_mode).to_owned(),
                mesh.enabled,
                mesh.double_sided,
                mesh.inverted_mask,
                mesh.masks,
                mesh.raw_blend_mode,
            ),
        ),
        mesh.runtime_id,
        version_tuple(version),
    )
}

pub(crate) fn meshing_from_tuple(
    value: MeshMeshingTuple,
) -> PyResult<Option<zipzi_core::MeshMeshing>> {
    value
        .map(|(boundary, constraints)| {
            Ok(zipzi_core::MeshMeshing {
                boundary,
                constraints: constraints
                    .into_iter()
                    .map(|(endpoints, kind)| {
                        Ok(zipzi_core::MeshConstraint {
                            endpoints,
                            kind: match kind.as_str() {
                                "user" => zipzi_core::MeshConstraintKind::User,
                                "edge_support" => zipzi_core::MeshConstraintKind::EdgeSupport,
                                _ => {
                                    return Err(PyValueError::new_err(
                                        "Invalid mesh constraint kind",
                                    ));
                                }
                            },
                        })
                    })
                    .collect::<PyResult<_>>()?,
            })
        })
        .transpose()
}

pub(crate) fn meshing_tuple(value: Option<zipzi_core::MeshMeshing>) -> MeshMeshingTuple {
    value.map(|m| {
        (
            m.boundary,
            m.constraints
                .into_iter()
                .map(|c| {
                    (
                        c.endpoints,
                        match c.kind {
                            zipzi_core::MeshConstraintKind::User => "user",
                            zipzi_core::MeshConstraintKind::EdgeSupport => "edge_support",
                        }
                        .to_owned(),
                    )
                })
                .collect(),
        )
    })
}

pub(crate) fn version_tuple(version: Version) -> (u64, u64, u64) {
    (version.session_id, version.generation, version.revision)
}

pub(crate) fn tuple_version(value: (u64, u64, u64)) -> Version {
    Version {
        session_id: value.0,
        generation: value.1,
        revision: value.2,
    }
}

pub(crate) fn mesh_tuple(mesh: zipzi_core::Mesh, version: Version) -> MeshTuple {
    (
        mesh.id,
        mesh.name,
        mesh.vertex_ids,
        mesh.base_positions
            .into_iter()
            .map(|p| (p.x, p.y))
            .collect(),
        version_tuple(version),
    )
}

pub(crate) fn binding_tuple(binding: MeshBinding, version: Version) -> MeshBindingTuple {
    (
        binding.id,
        binding.mesh_id,
        binding
            .axes
            .into_iter()
            .map(|axis| (axis.parameter_id, axis.keys))
            .collect(),
        binding
            .keyforms
            .into_iter()
            .map(|form| {
                (
                    form.keys,
                    form.positions.into_iter().map(|p| (p.x, p.y)).collect(),
                    appearance_tuple(form.appearance),
                    form.draw_order,
                )
            })
            .collect(),
        version_tuple(version),
    )
}

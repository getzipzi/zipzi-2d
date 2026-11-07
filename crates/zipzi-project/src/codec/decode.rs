use zipzi_core::{
    BlendMode, Mesh, RotationPose, RotationTransform, Status, Transform, TransformData,
    TransformKind, Vec2, WarpTransform,
};

use super::types::*;
use super::validation::validate_json_syntax;

fn decode_transform_kind(kind: i32) -> Result<TransformKind, Status> {
    match kind {
        0 => Ok(TransformKind::Warp),
        1 => Ok(TransformKind::Rotation),
        _ => Err(Status::error("INVALID_PROJECT", "Unknown Transform kind")),
    }
}

fn decode_blend_mode(mode: i32) -> Result<BlendMode, Status> {
    match mode {
        0 => Ok(BlendMode::Normal),
        1 => Ok(BlendMode::Additive),
        2 => Ok(BlendMode::Multiplicative),
        _ => Err(Status::error("INVALID_PROJECT", "Unknown blend mode")),
    }
}

pub(super) fn transform_from_wire(
    transform: &TransformWire,
    parent_id: Option<&str>,
) -> Result<Transform, Status> {
    let kind = decode_transform_kind(transform.kind)?;
    let data = match kind {
        TransformKind::Warp => TransformData::Warp(WarpTransform {
            bezier: transform.bezier.clone(),
            rows: transform.rows,
            columns: transform.columns,
            quad: transform.quad,
            points: transform
                .points
                .iter()
                .map(|p| Vec2::new(p[0], p[1]))
                .collect(),
        }),
        TransformKind::Rotation => TransformData::Rotation(RotationTransform {
            base_angle: transform.base_angle,
            pose: RotationPose::from(transform.rotation.clone()),
        }),
    };

    Ok(Transform {
        rotation_scale_unit: transform.rotation_scale_unit,
        original_shape: transform.original_shape.clone(),
        warp_authoring: transform.warp_authoring.clone(),
        id: transform.id.clone(),
        runtime_id: transform.runtime_id.clone(),
        name: transform.name.clone(),
        part_id: zipzi_core::PartId::optional(transform.part_id.clone()),
        parent_id: parent_id.and_then(|id| zipzi_core::TransformId::optional(id.to_owned())),
        enabled: transform.enabled,
        appearance: transform.appearance.clone().into(),
        data,
    })
}

pub(super) fn mesh_from_wire(mesh: &MeshWire, masks: Vec<String>) -> Result<Mesh, Status> {
    if !mesh.triangles.len().is_multiple_of(3) {
        return Err(Status::error(
            "INVALID_PROJECT",
            "triangles: expected triples of vertex IDs",
        ));
    }

    let blend_mode = decode_blend_mode(mesh.properties.blend_mode)?;
    Ok(Mesh {
        id: mesh.id.clone(),
        runtime_id: mesh.runtime_id.clone(),
        name: mesh.name.clone(),
        texture_asset_id: mesh.texture_asset_id.clone(),
        part_id: mesh.properties.part_id.clone(),
        deformer_id: mesh.properties.deformer_id.clone(),
        vertex_ids: mesh.vertex_ids.clone(),
        base_positions: mesh
            .base_positions
            .iter()
            .map(|p| Vec2::new(p[0], p[1]))
            .collect(),
        uvs: mesh.uvs.iter().map(|p| Vec2::new(p[0], p[1])).collect(),
        triangles: mesh.triangles.as_chunks::<3>().0.to_vec(),
        meshing: mesh.meshing.clone(),
        draw_order: mesh.properties.draw_order,
        raw_blend_mode: mesh.properties.raw_blend_mode,
        appearance: mesh.properties.appearance.clone().into(),
        blend_mode,
        enabled: mesh.properties.enabled,
        double_sided: mesh.properties.double_sided,
        inverted_mask: mesh.properties.inverted_mask,
        masks,
    })
}

pub(super) fn parse_wire(text: &str) -> Result<ProjectWire, Status> {
    validate_json_syntax(text)?;

    #[derive(serde::Deserialize)]
    struct Header {
        format: Option<String>,
        format_version: Option<u64>,
    }
    let header: Header =
        serde_json::from_str(text).map_err(|e| Status::error("INVALID_PROJECT", e.to_string()))?;
    let format = header.format.as_deref();
    let version = header.format_version;
    if format != Some("zipzi-project") {
        return Err(Status::error(
            "INVALID_PROJECT",
            "Unknown project document identifier",
        ));
    }
    if version != Some(22) {
        return Err(Status::error(
            "UNSUPPORTED_VERSION",
            "Unsupported project document version",
        ));
    }
    let root: ProjectWire =
        serde_json::from_str(text).map_err(|e| Status::error("INVALID_PROJECT", e.to_string()))?;

    Ok(root)
}

pub(super) fn admit_wire(
    mut root: ProjectWire,
) -> Result<
    (
        DocumentWire,
        Option<zipzi_core::document::DisplayInfo>,
        AnimationAssetsWire,
    ),
    Status,
> {
    if !root.extra.is_empty() || !root.document.extra.is_empty() {
        return Err(Status::error(
            "UNSUPPORTED_VERSION",
            "Unrecognized project fields",
        ));
    }
    let display_info = match std::mem::take(&mut root.document.display_info) {
        Present::Present(Some(info)) => Some(info),
        _ => {
            return Err(Status::error(
                "INVALID_PROJECT",
                "Project requires non-null display_info",
            ));
        }
    };
    let animation_assets = match std::mem::take(&mut root.document.animation_assets) {
        Present::Present(Some(assets)) => assets,
        _ => {
            return Err(Status::error(
                "INVALID_PROJECT",
                "Project requires non-null animation_assets",
            ));
        }
    };

    for (id, samples) in root
        .document
        .bindings
        .iter()
        .flat_map(|b| &b.axes)
        .chain(root.document.scene_bindings.iter().flat_map(|b| &b.axes))
        .map(|a| (&a.id, &a.samples))
        .chain(
            root.document
                .glues
                .iter()
                .filter_map(|g| g.binding.as_ref())
                .flat_map(|b| &b.axes)
                .map(|a| (&a.id, &a.samples)),
        )
    {
        if !zipzi_core::document::valid_uuid(id)
            || samples
                .iter()
                .any(|s| !zipzi_core::document::valid_uuid(&s.id))
        {
            return Err(Status::error(
                "INVALID_ID",
                "Native axis/sample identity is required",
            ));
        }
    }
    // Native sources already own their identities. Only external import/creation
    // APIs may synthesize IDs from implicit numeric coordinates.
    let valid = zipzi_core::document::valid_uuid;
    for table in &root.document.blend_key_tables {
        if table.samples.iter().any(|s| !valid(&s.id))
            || (!table.samples.is_empty() && !valid(&table.base_sample_id))
        {
            return Err(Status::error(
                "INVALID_ID",
                "Native table/sample identity is required",
            ));
        }
    }
    for constraint in &root.document.blend_constraints {
        if constraint.samples.iter().any(|s| !valid(&s.id)) {
            return Err(Status::error(
                "INVALID_ID",
                "Native constraint sample identity is required",
            ));
        }
    }
    for glue in &root.document.glues {
        if glue.binding.as_ref().is_some_and(|b| {
            b.keyforms
                .iter()
                .any(|f| f.samples.is_empty() || f.samples.iter().any(|id| !valid(id)))
        }) {
            return Err(Status::error(
                "INVALID_SAMPLE_REFERENCE",
                "Native Glue coordinates are required",
            ));
        }
    }
    for binding in &root.document.blend_bindings {
        let invalid = match &binding.keyforms {
            DeltaKeyformsWire::Mesh(f) => f.iter().any(|f| !valid(&f.sample_id)),
            DeltaKeyformsWire::Warp(f) => f.iter().any(|f| !valid(&f.sample_id)),
            DeltaKeyformsWire::Rotation(f) => f.iter().any(|f| !valid(&f.sample_id)),
            DeltaKeyformsWire::Part(f) => f.iter().any(|f| !valid(&f.sample_id)),
            DeltaKeyformsWire::Glue(f) => f.iter().any(|f| !valid(&f.sample_id)),
            DeltaKeyformsWire::Offscreen(f) => f.iter().any(|f| !valid(&f.sample_id)),
        };
        if invalid {
            return Err(Status::error(
                "INVALID_SAMPLE_REFERENCE",
                "Native delta sample identity is required",
            ));
        }
    }
    let doc = root.document;

    // Check prototype relationships
    for val in [
        &doc.deformers,
        &doc.deformation_links,
        &doc.organization_links,
    ]
    .into_iter()
    .flatten()
    {
        if let Some(arr) = val.as_array() {
            if !arr.is_empty() {
                return Err(Status::error(
                    "LEGACY_PROJECT",
                    "Prototype relationships are unsupported",
                ));
            }
        } else {
            return Err(Status::error(
                "LEGACY_PROJECT",
                "Prototype relationships are unsupported",
            ));
        }
    }

    Ok((doc, display_info, animation_assets))
}

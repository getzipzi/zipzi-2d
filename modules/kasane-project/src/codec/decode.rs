use kasane_core::types::{
    Appearance, BindingAxis, BlendMode, BlendShapeBinding, BlendShapeConstraint,
    BlendShapeKeyTable, DeltaGlueKeyform, DeltaKeyforms, DeltaMeshKeyform, DeltaOffscreenKeyform,
    DeltaPartKeyform, DeltaRotationKeyform, DeltaWarpKeyform, Glue, GlueVertexPair, Mesh,
    MeshBinding, MeshKeyform, Offscreen, OffscreenKeyform, RotationPose, SceneBinding, Status,
    Transform, TransformKind, Vec2,
};
use kasane_core::Document;
use kasane_core::{PartKeyform, RotationKeyform, SceneTrack, WarpKeyform};
use kasane_core::{RotationTransform, TransformData, WarpTransform};

use crate::resources::is_valid_asset_path;

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

fn transform_from_wire(
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
        id: transform.id.clone(),
        runtime_id: transform.runtime_id.clone(),
        name: transform.name.clone(),
        part_id: kasane_core::PartId::optional(transform.part_id.clone()),
        parent_id: parent_id.and_then(|id| kasane_core::TransformId::optional(id.to_owned())),
        enabled: transform.enabled,
        appearance: transform.appearance.clone().into(),
        data,
    })
}

fn mesh_from_wire(mesh: &MeshWire, masks: Vec<String>) -> Result<Mesh, Status> {
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

pub fn decode_project(text: &str) -> Result<Document, Status> {
    validate_json_syntax(text)?;

    let header: serde_json::Value =
        serde_json::from_str(text).map_err(|e| Status::error("INVALID_PROJECT", e.to_string()))?;
    if header.get("format").and_then(|v| v.as_str()) == Some("kasane-directory-project")
        && header.get("format_version").and_then(|v| v.as_u64()) != Some(19)
    {
        return Err(Status::error(
            "UNSUPPORTED_VERSION",
            "Only directory-project version 16 is supported; reimport the source asset",
        ));
    }
    let root: ProjectWire = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => {
            // Check legacy format
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
                if let Some(fmt) = v.get("format").and_then(|f| f.as_str()) {
                    if fmt == "kasane-project" {
                        let ver = v
                            .get("format_version")
                            .map(|x| x.to_string())
                            .unwrap_or_else(|| "\"missing\"".to_string());
                        return Err(Status::error(
                            "LEGACY_PROJECT",
                            format!("Experimental kasane-project version {ver} is unsupported; no automatic migration"),
                        ));
                    }
                }
            }
            return Err(Status::error("INVALID_PROJECT", e.to_string()));
        }
    };

    decode_wire(root)
}

pub(super) fn decode_wire(root: ProjectWire) -> Result<Document, Status> {
    if root.format == "kasane-project" {
        return Err(Status::error(
            "LEGACY_PROJECT",
            "Experimental kasane-project version is unsupported; no automatic migration",
        ));
    }

    if root.format != "kasane-directory-project" {
        return Err(Status::error("INVALID_PROJECT", "Unknown project format"));
    }

    if root.format_version != 19 {
        return Err(Status::error(
            "UNSUPPORTED_VERSION",
            format!(
                "Unsupported directory-project version: {}",
                root.format_version
            ),
        ));
    }

    if !root.extra.is_empty() || !root.document.extra.is_empty() {
        return Err(Status::error(
            "UNSUPPORTED_VERSION",
            "Unrecognized project fields",
        ));
    }
    let display_info = match &root.document.display_info {
        Present::Present(Some(info)) => Some(info.clone()),
        _ => {
            return Err(Status::error(
                "INVALID_PROJECT",
                "Project requires non-null display_info",
            ))
        }
    };
    let animation_assets = match &root.document.animation_assets {
        Present::Present(Some(assets)) => assets.clone(),
        _ => {
            return Err(Status::error(
                "INVALID_PROJECT",
                "Project requires non-null animation_assets",
            ))
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
        if !kasane_core::document::valid_uuid(id)
            || samples
                .iter()
                .any(|s| !kasane_core::document::valid_uuid(&s.id))
        {
            return Err(Status::error(
                "INVALID_ID",
                "Native axis/sample identity is required",
            ));
        }
    }
    // Native sources already own their identities. Only external import/creation
    // APIs may synthesize IDs from implicit numeric coordinates.
    let valid = kasane_core::document::valid_uuid;
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

    let mut candidate = Document::new();
    let s = candidate.begin_batch_build();
    if !s.is_ok() {
        return Err(s);
    }
    let s = candidate.initialize(
        doc.id,
        kasane_core::ModelSetup {
            export: doc.export_settings,
            source_canvas: doc.source_canvas,
        },
    );
    if !s.is_ok() {
        return Err(s);
    }

    for asset in doc.assets {
        if !is_valid_asset_path(&asset.source)
            || asset.sha256.len() != 64
            || !asset
                .sha256
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(Status::error(
                "INVALID_PROJECT",
                format!("{}: invalid relative asset path or SHA-256", asset.id),
            ));
        }
        let res = candidate.add_asset(asset);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    // Parts: Two-pass creation to avoid topological cycle order issues
    for p in &doc.parts {
        let mut tmp = p.clone();
        tmp.parent_id.clear();
        let res = candidate.create_part(tmp);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }
    for p in doc.parts {
        let res = candidate.replace_part(p);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    // Transforms: Two-pass creation
    for t in &doc.transforms {
        let tmp = transform_from_wire(t, None)?;
        let res = candidate.create_transform(tmp);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }
    for t in doc.transforms {
        let actual = transform_from_wire(&t, Some(&t.parent_id))?;
        let res = candidate.replace_transform(actual);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    // Meshes: Two-pass creation
    for m in &doc.meshes {
        let mesh = mesh_from_wire(m, Vec::new())?;
        let res = candidate.create_mesh(mesh);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }
    for m in doc.meshes {
        let mesh = mesh_from_wire(&m, m.properties.masks.clone())?;
        let res = candidate.replace_mesh(mesh);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    for p in doc.parameters {
        let res = candidate.create_parameter(p);
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    for b in doc.bindings {
        let axes = b.axes.into_iter().map(BindingAxis::from).collect();
        let keyforms = b
            .keyforms
            .into_iter()
            .map(|f| MeshKeyform {
                samples: f.samples,
                keys: vec![],
                positions: f.positions.iter().map(|p| Vec2::new(p[0], p[1])).collect(),
                appearance: f.appearance.map(Appearance::from).unwrap_or_default(),
                draw_order: f.draw_order,
            })
            .collect();
        let res = candidate.create_binding(MeshBinding {
            id: b.id,
            mesh_id: b.mesh_id,
            axes,
            keyforms,
        });
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    for sb in doc.scene_bindings {
        let axes = sb.axes.into_iter().map(BindingAxis::from).collect();
        let track = match candidate.get_transform(&sb.target_id).map(|t| t.kind()) {
            Some(TransformKind::Warp) => SceneTrack::Warp {
                target_id: sb.target_id.into(),
                keyforms: sb
                    .keyforms
                    .into_iter()
                    .map(|f| WarpKeyform {
                        samples: f.samples,
                        bezier: f.bezier.clone(),
                        keys: vec![],
                        positions: f
                            .positions
                            .into_iter()
                            .map(|p| Vec2::new(p[0], p[1]))
                            .collect(),
                        appearance: f.appearance.map(Appearance::from).unwrap_or_default(),
                    })
                    .collect(),
            },
            Some(TransformKind::Rotation) => SceneTrack::Rotation {
                target_id: sb.target_id.into(),
                keyforms: sb
                    .keyforms
                    .into_iter()
                    .map(|f| RotationKeyform {
                        samples: f.samples,
                        keys: vec![],
                        rotation: f.rotation.into(),
                        appearance: f.appearance.map(Appearance::from).unwrap_or_default(),
                    })
                    .collect(),
            },
            None => SceneTrack::Part {
                target_id: sb.target_id.into(),
                keyforms: sb
                    .keyforms
                    .into_iter()
                    .map(|f| PartKeyform {
                        samples: f.samples,
                        keys: vec![],
                        draw_order: f.draw_order,
                    })
                    .collect(),
            },
        };
        let res = candidate.create_scene_binding(SceneBinding {
            id: sb.id,
            axes,
            track,
        });
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    if let Some(groups) = doc.draw_order_groups {
        let result = candidate.replace_draw_order_groups(groups);
        if !result.status.is_ok() {
            return Err(result.status);
        }
    }

    for t in doc.blend_key_tables {
        let res = candidate.create_blend_key_table(BlendShapeKeyTable {
            samples: t.samples,
            base_sample_id: t.base_sample_id,
            id: t.id,
            parameter_id: t.parameter_id,
            keys: vec![],
            base_key_idx: 0,
        });
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    for c in doc.blend_constraints {
        let res = candidate.create_blend_constraint(BlendShapeConstraint {
            samples: c.samples,
            id: c.id,
            parameter_id: c.parameter_id,
            keys: vec![],
            weights: vec![],
        });
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    for g in doc.glues {
        if g.binding_id.is_some() {
            return Err(Status::error(
                "INVALID_GLUE_BINDING",
                "Legacy MeshBinding references cannot represent Glue intensity",
            ));
        }
        let pairs = g
            .pairs
            .into_iter()
            .map(|p| GlueVertexPair {
                id: p.id,
                vertex_a: p.vertex_a,
                vertex_b: p.vertex_b,
                weight_a: p.weight_a,
                weight_b: p.weight_b,
            })
            .collect();
        let res = candidate.create_glue(Glue {
            id: g.id,
            runtime_id: g.runtime_id,
            name: g.name,
            mesh_a_id: g.mesh_a_id,
            mesh_b_id: g.mesh_b_id,
            pairs,
            intensity: g.intensity,
            binding: g.binding,
        });
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    for o in doc.offscreens {
        let keyforms = o.keyforms.into_iter().map(OffscreenKeyform::from).collect();
        let res = candidate.create_offscreen(Offscreen {
            id: o.id,
            runtime_id: o.runtime_id,
            name: o.name,
            part_id: o.part_id,
            enabled: o.enabled,
            blend_mode: o.blend_mode,
            flags: o.flags,
            masks: o.masks,
            part_keyform_indices: o.part_keyform_indices,
            keyforms,
        });
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    for b in doc.blend_bindings {
        let keyforms = match b.keyforms {
            DeltaKeyformsWire::Mesh(forms) => DeltaKeyforms::Mesh(
                forms
                    .into_iter()
                    .map(|f| DeltaMeshKeyform {
                        sample_id: f.sample_id,
                        positions: f
                            .positions
                            .into_iter()
                            .map(|p| Vec2::new(p[0], p[1]))
                            .collect(),
                        opacity: f.opacity,
                        draw_order: f.draw_order,
                        multiply: f.multiply,
                        screen: f.screen,
                    })
                    .collect(),
            ),
            DeltaKeyformsWire::Warp(forms) => DeltaKeyforms::Warp(
                forms
                    .into_iter()
                    .map(|f| DeltaWarpKeyform {
                        sample_id: f.sample_id,
                        points: f
                            .points
                            .into_iter()
                            .map(|p| Vec2::new(p[0], p[1]))
                            .collect(),
                        opacity: f.opacity,
                        multiply: f.multiply,
                        screen: f.screen,
                    })
                    .collect(),
            ),
            DeltaKeyformsWire::Rotation(forms) => DeltaKeyforms::Rotation(
                forms
                    .into_iter()
                    .map(|f| DeltaRotationKeyform {
                        sample_id: f.sample_id,
                        origin: f.origin.map(|p| Vec2::new(p[0], p[1])),
                        angle: f.angle,
                        scale: f.scale,
                        opacity: f.opacity,
                        multiply: f.multiply,
                        screen: f.screen,
                    })
                    .collect(),
            ),
            DeltaKeyformsWire::Part(forms) => DeltaKeyforms::Part(
                forms
                    .into_iter()
                    .map(|f| DeltaPartKeyform {
                        sample_id: f.sample_id,
                        draw_order: f.draw_order,
                    })
                    .collect(),
            ),
            DeltaKeyformsWire::Glue(forms) => DeltaKeyforms::Glue(
                forms
                    .into_iter()
                    .map(|f| DeltaGlueKeyform {
                        sample_id: f.sample_id,
                        intensity: f.intensity,
                    })
                    .collect(),
            ),
            DeltaKeyformsWire::Offscreen(forms) => DeltaKeyforms::Offscreen(
                forms
                    .into_iter()
                    .map(|f| DeltaOffscreenKeyform {
                        sample_id: f.sample_id,
                        opacity: f.opacity,
                        multiply: f.multiply,
                        screen: f.screen,
                    })
                    .collect(),
            ),
        };
        let res = candidate.create_blend_binding(BlendShapeBinding {
            id: b.id,
            target_id: b.target_id,
            target_kind: b.target_kind,
            key_table_id: b.key_table_id,
            constraint_ids: b.constraint_ids,
            keyforms,
        });
        if !res.status.is_ok() {
            return Err(res.status);
        }
    }

    if let Present::Present(state) = &doc.editor_state {
        let result = candidate.replace_editor_state(state.clone());
        if !result.status.is_ok() {
            return Err(result.status);
        }
    }

    let result = candidate.replace_hierarchy_order(doc.hierarchy_order.clone());
    if !result.status.is_ok() {
        return Err(result.status);
    }
    if let Some(info) = display_info {
        let result = candidate.replace_display_info(info);
        if !result.status.is_ok() {
            return Err(result.status);
        }
    }
    for expression in animation_assets.expressions {
        let result = candidate.create_expression(expression);
        if !result.status.is_ok() {
            return Err(result.status);
        }
    }
    for motion in animation_assets.motions {
        let result = candidate.create_motion(motion);
        if !result.status.is_ok() {
            return Err(result.status);
        }
    }
    let result = candidate.set_package_attachments(animation_assets.package_attachments);
    if !result.status.is_ok() {
        return Err(result.status);
    }
    let result = candidate.set_motion_library(
        animation_assets.motion_groups,
        animation_assets.motion_roots,
    );
    if !result.status.is_ok() {
        return Err(result.status);
    }
    if let Some(pose) = animation_assets.pose {
        let result = candidate.set_pose(pose);
        if !result.status.is_ok() {
            return Err(result.status);
        }
    }
    if let Some(physics) = animation_assets.physics {
        let result = candidate.set_physics(physics);
        if !result.status.is_ok() {
            return Err(result.status);
        }
    }
    let result = candidate.set_missing_attachments(animation_assets.missing_attachments);
    if !result.status.is_ok() {
        return Err(result.status);
    }
    let settings = serde_json::from_value(animation_assets.model3_settings)
        .map_err(|e| Status::error("INVALID_PROJECT", e.to_string()))?;
    let result = candidate.set_model3_settings(settings);
    if !result.status.is_ok() {
        return Err(result.status);
    }

    let status = candidate.finish_batch_build();
    if !status.is_ok() {
        return Err(status);
    }
    candidate.mark_saved();
    Ok(candidate)
}

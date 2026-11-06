use std::sync::Arc;
use zipzi_core::state::{AssetContent, AssetRecord, OwnedDocument};
use zipzi_core::types::{
    Appearance, BindingAxis, BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable,
    DeltaGlueKeyform, DeltaKeyforms, DeltaMeshKeyform, DeltaOffscreenKeyform, DeltaPartKeyform,
    DeltaRotationKeyform, DeltaWarpKeyform, Glue, GlueVertexPair, MeshBinding, MeshKeyform,
    Offscreen, OffscreenKeyform, SceneBinding, Status, TransformKind, Vec2,
};
use zipzi_core::{PartKeyform, RotationKeyform, SceneTrack, WarpKeyform};

use crate::resources::is_valid_asset_path;

use super::types::*;

/// Decoded owned content plus stored image descriptors, before resource acquisition.
pub(crate) struct DecodedState {
    pub model: OwnedDocument,
    pub images: Vec<zipzi_core::ImageAsset>,
}
pub(crate) fn decode_state(text: &str) -> Result<DecodedState, Status> {
    let (doc, display_info, animation_assets) = super::decode::admit_wire(
        super::decode::parse_wire(text, Some(("zipzi-project", 22)))?,
    )?;
    let mut input = OwnedDocument {
        id: doc.id,
        setup: zipzi_core::ModelSetup {
            export: doc.export_settings,
            source_canvas: doc.source_canvas,
        },
        parts: doc.parts,
        parameters: doc.parameters,
        draw_order_groups: doc.draw_order_groups,
        hierarchy_order: doc.hierarchy_order,
        display_info: display_info.expect("admitted display info"),
        editor_state: match doc.editor_state {
            Present::Present(value) => value,
            Present::Absent => Default::default(),
        },
        expressions: animation_assets.expressions,
        motions: animation_assets.motions,
        motion_groups: animation_assets.motion_groups,
        motion_roots: animation_assets.motion_roots,
        pose: animation_assets.pose.map(Arc::new),
        physics: animation_assets.physics.map(Arc::new),
        missing_attachments: animation_assets.missing_attachments,
        package_attachments: animation_assets.package_attachments,
        model3_settings: serde_json::from_value(animation_assets.model3_settings)
            .map_err(|e| Status::error("INVALID_PROJECT", e.to_string()))?,
        ..Default::default()
    };
    for t in doc.transforms {
        input
            .transforms
            .push(super::decode::transform_from_wire(&t, Some(&t.parent_id))?);
    }
    for m in doc.meshes {
        input.meshes.push(super::decode::mesh_from_wire(
            &m,
            m.properties.masks.clone(),
        )?);
    }
    for asset in &doc.assets {
        if let Some(content) = &asset.content {
            if !is_valid_asset_path(&content.source)
                || content.sha256.len() != 64
                || !content
                    .sha256
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                || !content.is_valid()
            {
                return Err(Status::error("INVALID_PROJECT", "Invalid image descriptor"));
            }
        }
        input.assets.push(AssetRecord {
            id: asset.id.clone(),
            name: asset.name.clone(),
            content: match asset
                .content
                .as_ref()
                .map(|c| &c.source)
                .or(asset.unresolved_source.as_ref())
            {
                Some(source) => AssetContent::Unresolved {
                    source: source.clone(),
                },
                None => AssetContent::Empty,
            },
        });
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
        input.bindings.push(MeshBinding {
            id: b.id,
            mesh_id: b.mesh_id,
            axes,
            keyforms,
        });
    }

    for sb in doc.scene_bindings {
        let axes = sb.axes.into_iter().map(BindingAxis::from).collect();
        let track = match input
            .transforms
            .iter()
            .find(|t| t.id == sb.target_id)
            .map(|t| t.kind())
        {
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
        input.scene_bindings.push(SceneBinding {
            id: sb.id,
            axes,
            track,
        });
    }

    for t in doc.blend_key_tables {
        input.blend_key_tables.push(BlendShapeKeyTable {
            samples: t.samples,
            base_sample_id: t.base_sample_id,
            id: t.id,
            parameter_id: t.parameter_id,
            keys: vec![],
            base_key_idx: 0,
        });
    }

    for c in doc.blend_constraints {
        input.blend_constraints.push(BlendShapeConstraint {
            samples: c.samples,
            id: c.id,
            parameter_id: c.parameter_id,
            keys: vec![],
            weights: vec![],
        });
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
        input.glues.push(Glue {
            id: g.id,
            runtime_id: g.runtime_id,
            name: g.name,
            mesh_a_id: g.mesh_a_id,
            mesh_b_id: g.mesh_b_id,
            pairs,
            intensity: g.intensity,
            binding: g.binding,
        });
    }

    for o in doc.offscreens {
        let keyforms = o.keyforms.into_iter().map(OffscreenKeyform::from).collect();
        input.offscreens.push(Offscreen {
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
        input.blend_bindings.push(BlendShapeBinding {
            id: b.id,
            target_id: b.target_id,
            target_kind: b.target_kind,
            key_table_id: b.key_table_id,
            constraint_ids: b.constraint_ids,
            keyforms,
        });
    }

    Ok(DecodedState {
        model: input,
        images: doc.assets,
    })
}

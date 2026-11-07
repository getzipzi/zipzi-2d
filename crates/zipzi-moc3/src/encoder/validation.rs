use std::collections::HashMap;

use zipzi_core::evaluation::{evaluate_frame, DrawableFrame};
use zipzi_core::state::DocumentState;
use zipzi_core::types::{BlendShapeTargetKind, Status};

use super::helpers::is_representable;
use super::Moc3ExportVersion;

pub(super) fn validate_preflight(
    doc: &DocumentState,
    target_version: Moc3ExportVersion,
) -> Result<(u8, DrawableFrame), Status> {
    if let Some(omission) = doc.scene_export_omissions().first() {
        return Err(Status::error(
            "SCENE_NEEDS_CONFIGURATION",
            format!(
                "{}: {:?}; {:?}",
                omission.id, omission.reason, omission.chain
            ),
        ));
    }
    if let Some(omission) = doc.binding_export_omissions().first() {
        return Err(Status::error(
            if omission.code == "ZERO_AXIS_BINDING" {
                "ZERO_AXIS_BINDING_NOT_REPRESENTABLE"
            } else {
                "NEEDS_CONFIGURATION"
            },
            format!("{}: {}", omission.object_id, omission.code),
        ));
    }
    let has_v53_features = doc.offscreen_order().len() > 0
        || doc
            .mesh_order()
            .iter()
            .any(|id| doc.mesh(id).and_then(|m| m.raw_blend_mode).is_some())
        || doc.blend_binding_order().iter().any(|id| {
            doc.blend_binding(id)
                .map(|b| b.target_kind == BlendShapeTargetKind::Offscreen)
                .unwrap_or(false)
        });

    let export_version = match target_version {
        Moc3ExportVersion::V50 => {
            if has_v53_features {
                return Err(Status::error(
                    "INCOMPATIBLE_EXPORT_VERSION",
                    "Cannot export to MOC3 5.0: document contains Cubism 5.3 features (offscreens or extended blend modes)",
                ));
            }
            5
        }
        Moc3ExportVersion::V53 => 6,
        Moc3ExportVersion::Auto => {
            if has_v53_features {
                6
            } else {
                5
            }
        }
    };

    let mut frame = DrawableFrame::default();
    let s = evaluate_frame(doc, &HashMap::new(), &mut frame);
    if !s.is_ok() {
        return Err(s);
    }

    let drawables = &frame.drawables;
    if drawables.is_empty() {
        return Err(Status::error(
            "EMPTY_MODEL",
            "At least one mesh is required",
        ));
    }

    for d in drawables {
        if !is_representable(&d.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", d.id),
            ));
        }
        if d.positions.len() > 65536 {
            return Err(Status::error(
                "CAPACITY",
                format!("{}.vertex_ids: at most 65536 vertices", d.id),
            ));
        }
    }

    for id in doc.parameter_order() {
        let p = doc.parameter(id).unwrap();
        if !is_representable(&p.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", id),
            ));
        }
    }

    let parts = doc.sorted_parts();
    let transforms = doc.sorted_transforms();

    for id in &parts {
        let p = doc.part(id).unwrap();
        if !is_representable(&p.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", id),
            ));
        }
    }

    for id in &transforms {
        let t = doc.transform(id).unwrap();
        if !is_representable(&t.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", id),
            ));
        }
    }

    for id in doc.glue_order() {
        let g = doc.glue(id).unwrap();
        if !is_representable(&g.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", id),
            ));
        }
    }

    for id in doc.offscreen_order() {
        let os = doc.offscreen(id).unwrap();
        if !is_representable(&os.runtime_id) {
            return Err(Status::error(
                "UNREPRESENTABLE_ID",
                format!("{}.runtime_id: requires 1..63 printable ASCII bytes", id),
            ));
        }
    }

    Ok((export_version, frame))
}

#[cfg(test)]
mod authoring_tests {
    use super::*;
    #[test]
    fn zero_axis_binding_is_rejected_before_runtime_encoding() {
        use zipzi_core::{Canvas, Mesh, MeshBinding};
        let id = |n: u32| format!("00000000-0000-4000-8000-{n:012x}");
        let mut d = DocumentState::new(id(1), Canvas::default().into())
            .unwrap()
            .builder();
        d.create_asset(zipzi_core::state::AssetRecord {
            id: id(2),
            name: String::new(),
            content: zipzi_core::state::AssetContent::Image {
                resource: zipzi_core::state::ResourceKey::new(),
                width: 1,
                height: 1,
            },
        })
        .unwrap();
        assert!(d
            .create_mesh(Mesh {
                id: id(3),
                runtime_id: id(3),
                texture_asset_id: (id(2)).into(),
                ..Default::default()
            })
            .is_ok());
        assert!(d
            .create_binding(MeshBinding {
                id: id(4),
                mesh_id: id(3),
                ..Default::default()
            })
            .is_ok());
        let d = d.freeze().unwrap().state;
        let error = validate_preflight(&d, Moc3ExportVersion::Auto).unwrap_err();
        assert_eq!(error.code, "ZERO_AXIS_BINDING_NOT_REPRESENTABLE");
    }
}

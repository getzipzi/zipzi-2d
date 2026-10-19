//! Scene-object field layouts and display names.
use super::*;
use kasane_core::document::PosePartRef;

pub(super) const PROVIDER: PropertyProvider = PropertyProvider {
    fields,
    type_name: scene_type_name,
    ..PropertyProvider::DEFAULT
};

fn fields(kind: ObjectKind) -> FormDefinition {
    use FieldEditor::*;
    use editor_core::property::{deformer, glue, mesh, offscreen, part, texture};
    let fields: &[_] = match kind {
        ObjectKind::Part => &[
            (part::NAME, "properties.name", Text),
            (part::RUNTIME_ID, "properties.runtime_id", Text),
            (part::ENABLED, "properties.enabled", Toggle),
            (part::DRAW_ORDER, "properties.draw_order", Number),
        ],
        ObjectKind::Mesh => &[
            (mesh::NAME, "properties.name", Text),
            (mesh::RUNTIME_ID, "properties.runtime_id", Text),
            (mesh::ENABLED, "properties.enabled", Toggle),
            (mesh::DRAW_ORDER, "properties.draw_order", OptionalNumber),
            (mesh::OPACITY, "properties.opacity", Number),
        ],
        ObjectKind::Deformer => &[
            (deformer::NAME, "properties.name", Text),
            (deformer::RUNTIME_ID, "properties.runtime_id", Text),
            (deformer::ENABLED, "properties.enabled", Toggle),
        ],
        ObjectKind::Offscreen => &[
            (offscreen::NAME, "properties.name", Text),
            (offscreen::RUNTIME_ID, "properties.runtime_id", Text),
            (offscreen::ENABLED, "properties.enabled", Toggle),
        ],
        ObjectKind::Texture => &[(texture::NAME, "properties.name", Text)],
        ObjectKind::Glue => &[
            (glue::NAME, "properties.name", Text),
            (glue::RUNTIME_ID, "properties.runtime_id", Text),
            (glue::INTENSITY, "properties.intensity", Number),
        ],
        _ => &[],
    };
    FormDefinition::new(fields)
}

fn scene_type_name(document: &Document, object: &ObjectRef) -> SharedString {
    match object.kind {
        ObjectKind::Mesh => "ArtMesh".into(),
        ObjectKind::Deformer => {
            let rotation = document
                .object_index()
                .get(object)
                .is_some_and(|info| info.properties["kind"] == "rotation");
            t!(if rotation {
                "editor.tree_type_rotation"
            } else {
                "editor.tree_type_warp"
            })
            .into()
        }
        _ => super::type_name(document, object),
    }
}

pub(crate) fn part_name(document: &Document, part: &PosePartRef) -> String {
    match part {
        PosePartRef::Resolved { part_id } => document
            .object_index()
            .get(&ObjectRef::new(ObjectKind::Part, part_id))
            .map_or_else(
                || part_id.clone(),
                |p| {
                    format!(
                        "{} · {}",
                        p.name,
                        p.properties["runtime_id"].as_str().unwrap_or(part_id)
                    )
                },
            ),
        PosePartRef::Unresolved { runtime_id } => {
            t!("workspace.unresolved_binding", target = runtime_id).into_owned()
        }
    }
}

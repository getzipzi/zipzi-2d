//! Scene-object field layouts and display names.
use super::*;
use zipzi_core::document::PosePartRef;

pub(super) const PROVIDER: PropertyProvider = PropertyProvider {
    fields,
    type_name: scene_type_name,
    ..PropertyProvider::DEFAULT
};

fn fields(kind: ObjectKind) -> FormDefinition {
    if kind == ObjectKind::Mesh {
        return super::mesh::fields();
    }
    if kind == ObjectKind::Part {
        return part_fields();
    }
    use FieldEditor::*;
    use zipzi_editor_core::property::{glue, offscreen, texture};
    if kind == ObjectKind::Glue {
        return FormDefinition {
            fields: vec![
                FieldSpec::new(glue::NAME, "properties.name", Text),
                FieldSpec::new(glue::RUNTIME_ID, "properties.runtime_id", Text),
                FieldSpec::new(glue::INTENSITY, "properties.glue_attachment", Number).percent(),
            ],
            ..Default::default()
        };
    }
    let fields: &[_] = match kind {
        ObjectKind::Offscreen => &[
            (offscreen::NAME, "properties.name", Text),
            (offscreen::RUNTIME_ID, "properties.runtime_id", Text),
            (offscreen::ENABLED, "properties.enabled", Toggle),
        ],
        ObjectKind::Texture => &[(texture::NAME, "properties.name", Text)],
        _ => &[],
    };
    FormDefinition::new(fields)
}

pub(super) fn glue_details(
    editor: &Editor,
    target: Option<&PropertyTarget>,
) -> Vec<super::details::DetailRow> {
    use super::details::DetailRow;
    use zipzi_editor_core::RelationKind;
    let Some((document, target)) = editor.document().zip(target) else {
        return Vec::new();
    };
    let index = document.object_index();
    let Some(glue) = index.get(&target.object) else {
        return Vec::new();
    };
    let side = |kind| {
        index
            .outgoing(&target.object)
            .find(|edge| edge.kind == kind)
            .map_or("—", |edge| index.name(&edge.target))
    };
    let count = glue.properties["pairs"]
        .as_array()
        .map_or(0, |pairs| pairs.len());
    let scope = match &target.scope {
        zipzi_editor_core::PropertyScope::GlueKeyform { .. } => "properties.glue_strength_keyform",
        zipzi_editor_core::PropertyScope::GlueBlendShape { .. } => "properties.glue_strength_blend",
        zipzi_editor_core::PropertyScope::InterpolatedGlue
        | zipzi_editor_core::PropertyScope::UnavailableGlueBlendShape { .. } => {
            "properties.glue_strength_preview"
        }
        _ => "properties.glue_strength_base",
    };
    vec![
        DetailRow::Hint(t!(scope).into()),
        DetailRow::property(
            "glue-side-a",
            t!("properties.glue_mesh_a"),
            side(RelationKind::GlueA).to_owned(),
        ),
        DetailRow::property(
            "glue-side-b",
            t!("properties.glue_mesh_b"),
            side(RelationKind::GlueB).to_owned(),
        ),
        DetailRow::property(
            "glue-pair-count",
            t!("properties.glue_pair_count"),
            if count == 0 {
                t!("properties.glue_empty").into_owned()
            } else {
                count.to_string()
            },
        ),
    ]
}

fn part_fields() -> FormDefinition {
    use FieldEditor::*;
    use zipzi_editor_core::property::part::*;
    let mut fields = vec![
        FieldSpec::new(NAME, "properties.name", Text),
        FieldSpec::new(RUNTIME_ID, "properties.runtime_id", Text),
        FieldSpec::new(ENABLED, "properties.enabled", Toggle),
        FieldSpec::new(DRAW_ORDER, "properties.draw_order", Number),
        FieldSpec::new(OFFSCREEN, "properties.offscreen_drawing", Toggle),
        FieldSpec::new(OPACITY, "properties.opacity", Number).percent(),
        FieldSpec::new(MULTIPLY, "properties.multiply", Color).reset_color(0xffffff),
        FieldSpec::new(SCREEN, "properties.screen", Color).reset_color(0x000000),
        FieldSpec::new(COLOR_BLEND, "properties.color_blend", Select)
            .choices(false, super::mesh::color_modes),
        FieldSpec::new(ALPHA_BLEND, "properties.alpha_blend", Select)
            .choices(false, super::mesh::alpha_modes),
        FieldSpec::new(MASKS, "properties.masks", References).choices(true, super::mesh::meshes),
        FieldSpec::new(INVERTED_MASK, "properties.inverted_mask", Toggle),
    ];
    for field in &mut fields[4..] {
        field.section = Some("properties.offscreen_drawing");
    }
    FormDefinition {
        fields,
        ..Default::default()
    }
}

pub(super) fn part_definition(target: &PropertyTarget, editor: &Editor) -> FormDefinition {
    use zipzi_editor_core::property::part;
    let mut definition = part_fields();
    if editor
        .document()
        .and_then(|doc| doc.property_values(&target.object).ok())
        .is_none_or(|values| values.get(&part::OFFSCREEN.key) != Some(&PropertyValue::Bool(true)))
    {
        definition.fields.retain(|field| {
            !field.binding.path.starts_with("offscreen.")
                || field.binding.key == part::OFFSCREEN.key
        });
    }
    definition
}

pub(super) fn scene_type_name(document: &Document, object: &ObjectRef) -> SharedString {
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
        PosePartRef::Unspecified => "—".into(),
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

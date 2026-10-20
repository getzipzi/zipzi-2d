//! Mesh form presentation. Storage and editing policy belong to editor-core.
use super::*;
use editor_core::property::mesh::*;

pub(super) fn fields() -> FormDefinition {
    use FieldEditor::*;
    let field = |binding, label, editor| FieldSpec::new(binding, label, editor);
    FormDefinition {
        fields: vec![
            field(NAME, "properties.name", Text),
            field(RUNTIME_ID, "properties.runtime_id", Text),
            field(PART, "properties.part", Select).choices(true, parts),
            field(DEFORMER, "properties.deformer", Select).choices(true, deformers),
            field(MASKS, "properties.masks", References).choices(true, meshes),
            field(INVERTED_MASK, "properties.inverted_mask", Toggle),
            field(DRAW_ORDER, "properties.draw_order", OptionalNumber),
            field(OPACITY, "properties.opacity", Number).percent(),
            field(MULTIPLY, "properties.multiply", Color),
            field(SCREEN, "properties.screen", Color),
            field(COLOR_BLEND, "properties.color_blend", Select).choices(false, color_modes),
            field(ALPHA_BLEND, "properties.alpha_blend", Select).choices(false, alpha_modes),
            field(CULLING, "properties.culling", Toggle),
            field(ENABLED, "properties.enabled", Toggle),
        ],
        ..Default::default()
    }
}

fn tree_choices(
    document: &Document,
    deformer: bool,
    kind: ObjectKind,
    none: Option<&str>,
) -> Vec<Choice> {
    fn collect(
        nodes: &[editor_core::TreeNode],
        path: &str,
        kind: ObjectKind,
        index: &editor_core::ObjectIndex,
        choices: &mut Vec<Choice>,
    ) {
        for node in nodes {
            let label = if path.is_empty() {
                node.name.clone()
            } else {
                format!("{path} / {}", node.name)
            };
            if let Some(reference) = node.reference().filter(|r| r.kind == kind) {
                let runtime = index
                    .get(&reference)
                    .and_then(|info| info.properties["runtime_id"].as_str())
                    .unwrap_or_default();
                let title = if runtime.is_empty() || runtime == node.id || runtime == node.name {
                    label.clone()
                } else {
                    format!("{label} · {runtime}")
                };
                choices.push(Choice {
                    label: title.into(),
                    value: node.id.clone(),
                });
            }
            collect(
                &node.children,
                if node.reference().is_some() {
                    &label
                } else {
                    path
                },
                kind,
                index,
                choices,
            );
        }
    }
    let mut choices = none
        .map(|key| Choice {
            label: t!(key).into(),
            value: String::new(),
        })
        .into_iter()
        .collect();
    collect(
        &if deformer {
            document.deformer_tree()
        } else {
            document.mesh_tree()
        },
        "",
        kind,
        &document.object_index(),
        &mut choices,
    );
    choices
}
fn parts(document: &Document) -> Vec<Choice> {
    tree_choices(
        document,
        false,
        ObjectKind::Part,
        Some("properties.no_part"),
    )
}
fn deformers(document: &Document) -> Vec<Choice> {
    tree_choices(
        document,
        true,
        ObjectKind::Deformer,
        Some("properties.no_deformer"),
    )
}
fn meshes(document: &Document) -> Vec<Choice> {
    tree_choices(document, false, ObjectKind::Mesh, None)
}
fn modes(names: &[&str]) -> Vec<Choice> {
    names
        .iter()
        .map(|name| {
            let key = format!("properties.blend_{name}");
            Choice {
                label: t!(&key).into(),
                value: (*name).into(),
            }
        })
        .collect()
}
fn color_modes(_: &Document) -> Vec<Choice> {
    modes(COLOR_MODES)
}
fn alpha_modes(_: &Document) -> Vec<Choice> {
    modes(ALPHA_MODES)
}

//! Pose references retain configured, unresolved and unconfigured identities.
use super::*;
use zipzi_editor_core::property::business as fields;
pub(super) const PROVIDER: PropertyProvider = PropertyProvider {
    fields,
    choices,
    editable_in: |c| c == EditingContext::Animation,
    show_readonly: |field| {
        !field.starts_with("groups[")
            && !field.starts_with("entries[")
            && !field.starts_with("links[")
    },
    ..PropertyProvider::DEFAULT
};
fn fields(kind: ObjectKind) -> FormDefinition {
    use FieldEditor::*;
    let fields: &[_] = match kind {
        ObjectKind::Physics => &[(fields::PHYSICS_FPS, "timeline.fps", OptionalNumber)],
        ObjectKind::Pose => &[(fields::POSE_FADE, "workspace.pose_fade", OptionalNumber)],
        ObjectKind::PoseGroup => &[(fields::POSE_NAME, "properties.name", Text)],
        ObjectKind::PoseEntry => &[(fields::POSE_PART, "properties.type_Part", Select)],
        ObjectKind::PoseLink => &[(fields::POSE_LINK, "properties.type_Part", Select)],
        _ => &[],
    };
    let mut definition = FormDefinition::new(fields);
    for spec in &mut definition.fields {
        spec.searchable = spec.editor == Select;
    }
    definition
}
fn choices(document: &Document, object: &ObjectRef, key: PropertyKey) -> Option<Vec<Choice>> {
    if key.name() != "target" {
        return None;
    }
    let mut choices = vec![Choice {
        value: String::new(),
        label: t!("authoring.unconfigured").into(),
    }];
    choices.extend(
        document
            .object_index()
            .objects
            .iter()
            .filter(|o| o.reference.kind == ObjectKind::Part)
            .map(|o| Choice {
                value: o.reference.id.clone(),
                label: format!("{} · {}", o.name, o.reference.id).into(),
            }),
    );
    if let Ok(values) = document.property_values(object) {
        if let Some(PropertyValue::Text(value)) = values.get(&key) {
            if let Some(runtime) = value.strip_prefix("runtime:") {
                choices.push(Choice {
                    value: value.clone(),
                    label: format!("{} · {runtime}", t!("authoring.unresolved")).into(),
                });
            }
        }
    }
    Some(choices)
}

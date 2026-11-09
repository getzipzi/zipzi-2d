use super::*;
use zipzi_editor_core::{CursorPreviewAxis, CursorPreviewBindings, PropertyPatch};

const AXIS: PropertySpec = PropertySpec::new(
    ObjectKind::Document,
    "cursor_axis",
    "axis",
    PropertyType::Text,
);
const INFLUENCE: PropertySpec = PropertySpec::new(
    ObjectKind::Document,
    "cursor_influence",
    "influence_percent",
    PropertyType::Double,
)
.range(0., 100., 1.);
const REVERSED: PropertySpec = PropertySpec::new(
    ObjectKind::Document,
    "cursor_reversed",
    "reversed",
    PropertyType::Bool,
);

pub(crate) fn field_keys(parameter: &str) -> [PropertyKey; 3] {
    [AXIS, INFLUENCE, REVERSED].map(|spec| spec.key.for_entity(parameter))
}

pub(super) fn definition(doc: &Document) -> FormDefinition {
    let bindings = doc.cursor_preview_bindings();
    let mut influence = FieldSpec::new(INFLUENCE, "physics.cursor_influence", FieldEditor::Number);
    influence.display_decimals = Some(3);
    let specs = [
        FieldSpec::new(AXIS, "physics.cursor_axis", FieldEditor::Select).choices(false, |_| {
            [
                ("x", t!("physics.cursor_left_x")),
                ("y", t!("physics.cursor_left_y")),
                ("right_x", t!("physics.cursor_right_x")),
                ("right_y", t!("physics.cursor_right_y")),
            ]
            .into_iter()
            .map(|(value, label)| Choice {
                label: label.into(),
                value: value.into(),
            })
            .collect()
        }),
        influence,
        FieldSpec::new(REVERSED, "physics.reflect", FieldEditor::Toggle),
    ];
    let mut form = FormDefinition::default();
    for parameter in doc.parameters() {
        if bindings
            .get(&parameter.id)
            .is_some_and(|binding| binding.axis != CursorPreviewAxis::Disabled)
        {
            form.fields
                .extend(
                    specs
                        .into_iter()
                        .zip(field_keys(&parameter.id))
                        .map(|(mut spec, key)| {
                            spec.binding.key = key;
                            spec
                        }),
                );
        }
    }
    form
}

pub(super) fn values(doc: &Document) -> PropertyValues {
    doc.cursor_preview_bindings()
        .into_iter()
        .filter_map(|(id, binding)| {
            let axis = match binding.axis {
                CursorPreviewAxis::X => "x",
                CursorPreviewAxis::Y => "y",
                CursorPreviewAxis::RightX => "right_x",
                CursorPreviewAxis::RightY => "right_y",
                CursorPreviewAxis::Disabled => return None,
            };
            let [axis_key, influence, reversed] = field_keys(&id);
            Some([
                (axis_key, PropertyValue::Text(axis.into())),
                (influence, PropertyValue::Double(binding.influence_percent)),
                (reversed, PropertyValue::Bool(binding.reversed)),
            ])
        })
        .flatten()
        .collect()
}

pub(super) fn commit(
    editor: &mut Editor,
    document: DocumentId,
    patch: PropertyPatch,
) -> anyhow::Result<()> {
    let committed = values(
        editor
            .document()
            .ok_or_else(|| anyhow::anyhow!("Document closed"))?,
    );
    anyhow::ensure!(
        patch
            .expected
            .iter()
            .all(|(key, value)| committed.get(key) == Some(value)),
        "Mouse mapping changed during editing"
    );
    edit_bindings(editor, document, |mut bindings| {
        for (key, value) in patch.changes {
            let binding = key
                .entity_id()
                .and_then(|id| bindings.get_mut(&id))
                .ok_or_else(|| anyhow::anyhow!("Mouse mapping changed during editing"))?;
            match (key.name(), value) {
                ("cursor_axis", PropertyValue::Text(axis)) => {
                    binding.axis = match axis.as_str() {
                        "x" => CursorPreviewAxis::X,
                        "y" => CursorPreviewAxis::Y,
                        "right_x" => CursorPreviewAxis::RightX,
                        "right_y" => CursorPreviewAxis::RightY,
                        _ => anyhow::bail!("Invalid mouse mapping axis"),
                    };
                }
                ("cursor_influence", PropertyValue::Double(value))
                    if (0. ..=100.).contains(&value) =>
                {
                    binding.influence_percent = value;
                }
                ("cursor_reversed", PropertyValue::Bool(value)) => binding.reversed = value,
                _ => anyhow::bail!("Invalid mouse mapping property"),
            }
        }
        Ok(Some(bindings))
    })
}

pub(crate) fn edit_bindings(
    editor: &mut Editor,
    document: DocumentId,
    change: impl FnOnce(CursorPreviewBindings) -> anyhow::Result<Option<CursorPreviewBindings>>,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        editor.active_id() == Some(document)
            && editor.editing_context() == Some(EditingContext::Animation),
        "Editing context changed"
    );
    let doc = editor
        .document()
        .ok_or_else(|| anyhow::anyhow!("Document closed"))?;
    let revision = doc.model_generation();
    let before = doc.cursor_preview_bindings();
    let after = change(before.clone())?;
    if after.as_ref() != Some(&before)
        && (after.is_some() || doc.has_custom_cursor_preview_bindings())
    {
        editor.replace_cursor_preview_bindings(document, revision, after)?;
    }
    Ok(())
}

//! Animation field definitions and context detail provider.
use super::details::DetailSource;
use super::*;
use std::collections::HashMap;
use zipzi_core::document::{ExpressionBlend, ExpressionTarget};
use zipzi_editor_core::AnimationCatalog;

pub(super) const PROVIDER: PropertyProvider = PropertyProvider {
    fields,
    editable_in: |_| true,
    choices,
    show_readonly: |field| !field.starts_with("events[") && !field.starts_with("entries["),
    ..PropertyProvider::DEFAULT
};
fn fields(kind: ObjectKind) -> FormDefinition {
    use FieldEditor::*;
    use zipzi_editor_core::property::{expression, motion};
    let fields: &[_] = match kind {
        ObjectKind::Motion => &[
            (motion::NAME, "properties.name", Text),
            (motion::DURATION, "timeline.duration", Number),
            (motion::FPS, "timeline.fps", Number),
            (motion::LOOPING, "properties.looping", Toggle),
            (motion::FADE_IN, "properties.fade_in", OptionalNumber),
            (motion::FADE_OUT, "properties.fade_out", OptionalNumber),
        ],
        ObjectKind::Expression => &[
            (expression::NAME, "properties.name", Text),
            (expression::FADE_IN, "properties.fade_in", OptionalNumber),
            (expression::FADE_OUT, "properties.fade_out", OptionalNumber),
        ],
        ObjectKind::MotionGroup => &[(
            zipzi_editor_core::property::animation_entry::GROUP_NAME,
            "properties.name",
            Text,
        )],
        ObjectKind::MotionRegistration => &[
            (
                zipzi_editor_core::property::animation_entry::REG_FADE_IN,
                "authoring.registration_fade_in",
                OptionalNumber,
            ),
            (
                zipzi_editor_core::property::animation_entry::REG_FADE_OUT,
                "authoring.registration_fade_out",
                OptionalNumber,
            ),
            (
                zipzi_editor_core::property::animation_entry::SOUND,
                "workspace.sound",
                Select,
            ),
        ],
        ObjectKind::ExpressionEntry => &[
            (
                zipzi_editor_core::property::animation_entry::TARGET,
                "authoring.expression_parameter",
                Select,
            ),
            (
                zipzi_editor_core::property::animation_entry::VALUE,
                "authoring.expression_value",
                Number,
            ),
            (
                zipzi_editor_core::property::animation_entry::BLEND,
                "authoring.expression_blend",
                Select,
            ),
        ],
        _ => unreachable!("animation object provider"),
    };
    let mut definition = FormDefinition::new(fields);
    for field in &mut definition.fields {
        field.searchable = matches!(field.binding.key.name(), "target" | "registration_sound");
    }
    definition
}
pub(super) fn details_source(
    editor: &Editor,
    target: Option<&PropertyTarget>,
) -> Option<DetailSource> {
    editor.animation()?;
    let target = target?;
    let inspected = &target.object;
    if !matches!(inspected.kind, ObjectKind::Motion | ObjectKind::Expression) {
        return None;
    }
    Some(DetailSource {
        dependencies: vec![
            Some(inspected.id.clone()),
            target.motion.as_ref().map(|motion| motion.clip_id.clone()),
            target
                .motion
                .as_ref()
                .and_then(|motion| motion.entry.clone()),
            target
                .motion
                .as_ref()
                .and_then(|motion| motion.entry.clone()),
        ],
        build: |editor, target| {
            let document = editor.document().unwrap();
            rows(document, &document.animation_catalog(), target)
        },
    })
}

fn rows(
    document: &Document,
    catalog: &AnimationCatalog,
    target: Option<&PropertyTarget>,
) -> Vec<DetailRow> {
    let inspected = target.map(|target| &target.object);
    let mut rows = Vec::new();
    let motion_selected = inspected.is_some_and(|object| object.kind == ObjectKind::Motion);
    let resource = catalog
        .motions
        .iter()
        .find(|r| target.and_then(|target| target.motion.as_ref()) == Some(&r.selection));
    if motion_selected {
        if let Some(resource) = resource {
            rows.push(DetailRow::property(
                "animation-property-registration",
                t!("workspace.registration"),
                resource.group.as_ref().map_or_else(
                    || t!("workspace.ungrouped").into_owned(),
                    |group| format!("{group} [{}]", resource.entry_index + 1),
                ),
            ));
            rows.push(DetailRow::text(t!(
                "authoring.clip_shared",
                count = catalog
                    .motions
                    .iter()
                    .filter(|r| r.selection.clip_id == resource.selection.clip_id)
                    .count()
            )));
            if let Some((_, entry)) = resource
                .selection
                .entry
                .as_deref()
                .and_then(|id| document.motion_registration(id))
            {
                for (label, value, effective) in [
                    ("properties.fade_in", entry.fade_in, resource.fade_in),
                    ("properties.fade_out", entry.fade_out, resource.fade_out),
                ] {
                    rows.push(DetailRow::property(
                        if label == "properties.fade_in" {
                            "animation-registration-fade-in"
                        } else {
                            "animation-registration-fade-out"
                        },
                        t!(label),
                        t!(
                            if value.is_some() {
                                "authoring.overridden_fade"
                            } else {
                                "authoring.inherited_fade"
                            },
                            value = format!("{effective:.3}")
                        ),
                    ));
                }
            }
            if !resource.playable() {
                rows.push(DetailRow::text(t!("workspace.invalid_duration")));
            }
            if let Some(sound) = &resource.sound {
                rows.push(DetailRow::field(
                    "animation-property-sound",
                    t!("workspace.sound"),
                    sound.clone(),
                ));
                rows.push(DetailRow::text(t!("workspace.sound_unsupported")));
            }
        }
        return rows;
    }
    // Resolve names once, rather than scanning the parameter catalog per track.
    let names: HashMap<_, _> = catalog
        .parameters
        .iter()
        .map(|p| {
            (
                p.id.as_str(),
                SharedString::from(format!("{} · {}", p.name, p.runtime_id)),
            )
        })
        .collect();
    let parameter_name = |id: &str| {
        names
            .get(id)
            .cloned()
            .unwrap_or_else(|| id.to_owned().into())
    };
    rows.push(DetailRow::Heading(
        t!("workspace.selected_expression").into(),
    ));
    if let Some(resource) = catalog.expressions.iter().find(|r| {
        inspected.is_some_and(|object| object.kind == ObjectKind::Expression && object.id == r.id)
    }) {
        rows.push(DetailRow::property(
            "animation-property-expression",
            t!("workspace.resource_name"),
            resource.name.clone(),
        ));
        rows.push(DetailRow::Text {
            id: Some("animation-property-expression-summary".into()),
            value: expression_info(resource).into(),
        });
        if let Some(expression) = document.expression_definition(&resource.id) {
            for (index, entry) in expression.entries.iter().enumerate() {
                let target = match &entry.target {
                    ExpressionTarget::Unspecified => t!("workspace.none").into(),
                    ExpressionTarget::Resolved { parameter_id } => parameter_name(parameter_id),
                    ExpressionTarget::Unresolved { runtime_id } => {
                        t!("workspace.unresolved_binding", target = runtime_id).into()
                    }
                };
                let blend = match entry.blend.unwrap_or(ExpressionBlend::Add) {
                    ExpressionBlend::Add => t!("workspace.blend_add"),
                    ExpressionBlend::Multiply => t!("workspace.blend_multiply"),
                    ExpressionBlend::Overwrite => t!("workspace.blend_overwrite"),
                };
                rows.push(DetailRow::field(
                    ("animation-property-expression-entry", index),
                    target,
                    format!("{blend} · {:.4}", entry.value),
                ));
            }
        }
    } else {
        rows.push(DetailRow::Text {
            id: Some("animation-property-no-expression".into()),
            value: t!("workspace.none").into(),
        });
    }
    rows
}

pub(crate) fn expression_info(resource: &zipzi_editor_core::ExpressionResource) -> String {
    let blends = resource
        .blends
        .iter()
        .map(|blend| {
            match blend {
                zipzi_core::document::ExpressionBlend::Add => t!("workspace.blend_add"),
                zipzi_core::document::ExpressionBlend::Multiply => t!("workspace.blend_multiply"),
                zipzi_core::document::ExpressionBlend::Overwrite => {
                    t!("workspace.blend_overwrite")
                }
            }
            .into_owned()
        })
        .collect::<Vec<_>>()
        .join(" / ");
    format!(
        "{} · {} · {}",
        t!("workspace.expression_count", count = resource.parameters),
        blends,
        t!(
            "workspace.fades",
            fade_in = format!("{:.3}", resource.fade_in),
            fade_out = format!("{:.3}", resource.fade_out)
        )
    )
}

pub(super) fn definition(target: &PropertyTarget, editor: &Editor) -> FormDefinition {
    let mut definition = fields(target.object.kind);
    if target.object.kind == ObjectKind::Motion {
        for field in &mut definition.fields {
            field.section = Some("authoring.clip_content");
        }
        if let Some(entry) = target.motion.as_ref().and_then(|m| m.entry.as_deref()) {
            for mut field in fields(ObjectKind::MotionRegistration).fields {
                field.binding.key = field.binding.key.for_entity(entry);
                field.section = Some("authoring.current_registration");
                definition.fields.push(field);
            }
        }
    } else if target.object.kind == ObjectKind::Expression {
        if let Some(expression) = editor
            .document()
            .and_then(|d| d.expression_definition(&target.object.id))
        {
            for entry in &expression.entries {
                for mut field in fields(ObjectKind::ExpressionEntry).fields {
                    field.binding.key = field.binding.key.for_entity(&entry.id);
                    field.section = Some("authoring.expression_member");
                    definition.fields.push(field);
                }
            }
        }
    }
    definition
}
pub(super) fn values(target: &PropertyTarget, editor: &Editor) -> anyhow::Result<PropertyValues> {
    let document = editor.document().unwrap();
    let mut values = document.property_values(&target.object)?;
    let children: Vec<_> = if target.object.kind == ObjectKind::Motion {
        target
            .motion
            .as_ref()
            .and_then(|m| m.entry.as_ref())
            .map(|id| ObjectRef::new(ObjectKind::MotionRegistration, id))
            .into_iter()
            .collect()
    } else if target.object.kind == ObjectKind::Expression {
        document
            .expression_definition(&target.object.id)
            .into_iter()
            .flat_map(|e| e.entries.iter())
            .map(|e| ObjectRef::new(ObjectKind::ExpressionEntry, &e.id))
            .collect()
    } else {
        vec![]
    };
    for child in children {
        values.extend(
            document
                .property_values(&child)?
                .into_iter()
                .map(|(key, value)| (key.for_entity(&child.id), value)),
        );
    }
    Ok(values)
}
/// Repeated fields carry child identity in the key. Remap only the captured
/// child's descriptors, preserving the captured revision and baseline.
pub(super) fn field_target(
    target: &PropertyTarget,
    patch: &zipzi_editor_core::PropertyPatch,
) -> (ObjectRef, zipzi_editor_core::PropertyPatch) {
    let key = patch
        .changes
        .keys()
        .next()
        .or_else(|| patch.expected.keys().next());
    let Some((key, id)) = key
        .filter(|key| {
            matches!(
                key.object_kind(),
                ObjectKind::MotionRegistration | ObjectKind::ExpressionEntry
            )
        })
        .and_then(|key| key.entity_id().map(|id| (*key, id)))
    else {
        let mut patch = patch.clone();
        if matches!(
            target.scope,
            zipzi_editor_core::PropertyScope::Resource { .. }
        ) {
            patch.scope = target.scope.clone();
        }
        return (target.object.clone(), patch);
    };
    let remap = |values: &PropertyValues| {
        values
            .iter()
            .filter(|(k, _)| {
                k.object_kind() == key.object_kind() && k.entity_id().as_deref() == Some(&id)
            })
            .map(|(k, v)| (PropertyKey::new(k.object_kind(), k.name()), v.clone()))
            .collect()
    };
    (
        ObjectRef::new(key.object_kind(), &id),
        zipzi_editor_core::PropertyPatch {
            scope: target.scope.clone(),
            expected: remap(&patch.expected),
            changes: remap(&patch.changes),
        },
    )
}
fn choices(document: &Document, _: &ObjectRef, key: PropertyKey) -> Option<Vec<Choice>> {
    use zipzi_editor_core::property::animation_entry as schema;
    let choice = |value: &str, label: SharedString| Choice {
        value: value.into(),
        label,
    };
    match key.name() {
        name if name == schema::TARGET.key.name() => {
            let mut choices = vec![choice("", t!("workspace.none").into())];
            choices.extend(
                document
                    .animation_catalog()
                    .parameters
                    .iter()
                    .map(|p| choice(&p.id, format!("{} · {}", p.name, p.runtime_id).into())),
            );
            Some(choices)
        }
        name if name == schema::BLEND.key.name() => Some(vec![
            choice("", t!("authoring.inherit_add").into()),
            choice("add", t!("workspace.blend_add").into()),
            choice("multiply", t!("workspace.blend_multiply").into()),
            choice("overwrite", t!("workspace.blend_overwrite").into()),
        ]),
        name if name == schema::SOUND.key.name() => {
            let mut choices = vec![choice("", t!("workspace.none").into())];
            choices.extend(
                document
                    .object_index()
                    .objects
                    .iter()
                    .filter(|info| info.reference.kind == ObjectKind::Attachment)
                    .map(|info| choice(&info.reference.id, info.name.clone().into())),
            );
            Some(choices)
        }
        _ => None,
    }
}

/// Effective inheritance is visible beside the override, even with Details folded.
pub(crate) fn registration_status(
    document: &Document,
    target: &PropertyTarget,
    key: PropertyKey,
) -> Option<DetailRow> {
    use zipzi_editor_core::property::animation_entry as schema;
    if key.object_kind() != ObjectKind::MotionRegistration
        || ![
            schema::REG_FADE_IN.key.name(),
            schema::REG_FADE_OUT.key.name(),
        ]
        .contains(&key.name())
    {
        return None;
    }
    let id = key.entity_id().unwrap_or_else(|| target.object.id.clone());
    let (_, entry) = document.motion_registration(&id)?;
    let clip = document.motion_definition(&entry.clip_id)?;
    let (value, inherited) = if key.name() == schema::REG_FADE_IN.key.name() {
        (entry.fade_in, clip.fade_in)
    } else {
        (entry.fade_out, clip.fade_out)
    };
    Some(DetailRow::Text {
        id: Some(gpui_kit::ElementId::Name(
            format!("registration-status-{}-{id}", key.name()).into(),
        )),
        value: t!(
            if value.is_some() {
                "authoring.overridden_fade"
            } else {
                "authoring.inherited_fade"
            },
            value = format!("{:.3}", value.or(inherited).unwrap_or(1.))
        )
        .into(),
    })
}

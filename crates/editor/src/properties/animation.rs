//! Animation field definitions and context detail provider.
use super::details::DetailSource;
use super::*;
use editor_core::AnimationCatalog;
use kasane_core::document::{ExpressionBlend, ExpressionTarget};
use std::collections::HashMap;

pub(super) const PROVIDER: PropertyProvider = PropertyProvider {
    fields,
    editable_in: |_| true,
    show_readonly: |field| !field.starts_with("events[") && !field.starts_with("entries["),
    ..PropertyProvider::DEFAULT
};
fn fields(kind: ObjectKind) -> FormDefinition {
    use FieldEditor::*;
    use editor_core::property::{expression, motion};
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
        _ => unreachable!("animation object provider"),
    };
    FormDefinition::new(fields)
}
pub(super) fn details_source(
    editor: &Editor,
    target: Option<&PropertyTarget>,
) -> Option<DetailSource> {
    editor.animation()?;
    let target = target?;
    let inspected = &target.object;
    if matches!(
        inspected.kind,
        ObjectKind::PhysicsGroup | ObjectKind::Physics
    ) {
        return None;
    }
    Some(DetailSource {
        dependencies: vec![
            Some(inspected.id.clone()),
            target.motion.as_ref().map(|motion| motion.clip_id.clone()),
            target
                .motion
                .as_ref()
                .and_then(|motion| motion.entry.as_ref().map(|(group, _)| group.clone())),
            target
                .motion
                .as_ref()
                .and_then(|motion| motion.entry.as_ref().map(|(_, index)| index.to_string())),
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
                resource.selection.entry.as_ref().map_or_else(
                    || t!("workspace.ungrouped").into_owned(),
                    |(group, index)| format!("{group} [{}]", index + 1),
                ),
            ));
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

pub(crate) fn expression_info(resource: &editor_core::ExpressionResource) -> String {
    let blends = resource
        .blends
        .iter()
        .map(|blend| {
            match blend {
                kasane_core::document::ExpressionBlend::Add => t!("workspace.blend_add"),
                kasane_core::document::ExpressionBlend::Multiply => t!("workspace.blend_multiply"),
                kasane_core::document::ExpressionBlend::Overwrite => {
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

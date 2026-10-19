//! Animation field definitions and context detail provider.
use super::details::DetailSource;
use super::scene::part_name;
use super::*;
use editor_core::{AnimationCatalog, AnimationSession};
use kasane_core::document::{ExpressionBlend, ExpressionTarget, MotionTrackTarget, PosePartRef};
use std::collections::HashMap;

pub(super) const PROVIDER: PropertyProvider = PropertyProvider {
    fields,
    editable_in: |_| true,
    ..PropertyProvider::DEFAULT
};
fn fields(kind: ObjectKind) -> FormDefinition {
    use FieldEditor::*;
    use editor_core::property::{expression, motion};
    let fields: &[_] = match kind {
        ObjectKind::Motion => &[
            (motion::NAME, "properties.name", Text),
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
pub(super) fn details_source(editor: &Editor) -> Option<DetailSource> {
    let animation = editor.animation()?;
    Some(DetailSource {
        dependencies: vec![
            animation
                .motion
                .as_ref()
                .map(|motion| motion.clip_id.clone()),
            animation
                .motion
                .as_ref()
                .and_then(|motion| motion.entry.as_ref().map(|(group, _)| group.clone())),
            animation
                .motion
                .as_ref()
                .and_then(|motion| motion.entry.as_ref().map(|(_, index)| index.to_string())),
            animation.expression.clone(),
        ],
        build: |editor| {
            let document = editor.document().unwrap();
            rows(
                document,
                editor.animation().unwrap(),
                &document.animation_catalog(),
            )
        },
    })
}

fn rows(
    document: &Document,
    animation: &AnimationSession,
    catalog: &AnimationCatalog,
) -> Vec<DetailRow> {
    let mut rows = vec![
        DetailRow::Hint(t!("workspace.properties_hint").into()),
        DetailRow::Heading(t!("workspace.selected_motion").into()),
    ];
    let resource = catalog
        .motions
        .iter()
        .find(|r| animation.motion.as_ref() == Some(&r.selection));
    if let Some(resource) = resource {
        rows.extend([
            DetailRow::property(
                "animation-property-motion",
                t!("workspace.resource_name"),
                resource.name.clone(),
            ),
            DetailRow::property(
                "animation-property-registration",
                t!("workspace.registration"),
                resource.selection.entry.as_ref().map_or_else(
                    || t!("workspace.ungrouped").into_owned(),
                    |(group, index)| format!("{group} [{}]", index + 1),
                ),
            ),
            DetailRow::property(
                "animation-property-duration",
                t!("workspace.duration"),
                format!("{:.3} s", resource.duration),
            ),
            DetailRow::property(
                "animation-property-fps",
                t!("workspace.frame_rate"),
                format!("{:.1} fps", resource.fps),
            ),
            DetailRow::property(
                "animation-property-loop",
                t!("workspace.playback_mode"),
                if resource.looping {
                    t!("workspace.looping")
                } else {
                    t!("workspace.once")
                },
            ),
            DetailRow::property(
                "animation-property-fades",
                t!("workspace.effective_fades"),
                t!(
                    "workspace.fades",
                    fade_in = format!("{:.3}", resource.fade_in),
                    fade_out = format!("{:.3}", resource.fade_out)
                ),
            ),
        ]);
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
    } else {
        rows.push(DetailRow::Text {
            id: Some("animation-property-no-motion".into()),
            value: t!("workspace.select_motion").into(),
        });
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
    if let Some(resource) = catalog
        .expressions
        .iter()
        .find(|r| animation.expression.as_ref() == Some(&r.id))
    {
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
    if let Some(clip) = resource.and_then(|r| document.motion_definition(&r.selection.clip_id)) {
        rows.push(DetailRow::Heading(t!("workspace.track_targets").into()));
        for (index, track) in clip.tracks.iter().enumerate() {
            let target = match &track.target {
                MotionTrackTarget::Parameter { parameter_id } => parameter_name(parameter_id),
                MotionTrackTarget::PartOpacity { part_id } => part_name(
                    document,
                    &PosePartRef::Resolved {
                        part_id: part_id.clone(),
                    },
                )
                .into(),
                MotionTrackTarget::Model { runtime_id } => {
                    t!("workspace.model_channel", target = runtime_id).into()
                }
                MotionTrackTarget::Unresolved {
                    category,
                    runtime_id,
                } => t!(
                    "workspace.unresolved_binding",
                    target = format!("{category}: {runtime_id}")
                )
                .into(),
            };
            rows.push(DetailRow::field(
                ("animation-property-track", index),
                target,
                t!("workspace.track_segments", count = track.segments.len()),
            ));
        }
        if !clip.events.is_empty() {
            rows.push(DetailRow::Heading(t!("workspace.motion_events").into()));
            for (index, event) in clip.events.iter().enumerate() {
                rows.push(DetailRow::field(
                    ("animation-property-event", index),
                    format!("{:.3} s", event.time),
                    event.value.clone(),
                ));
            }
        }
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

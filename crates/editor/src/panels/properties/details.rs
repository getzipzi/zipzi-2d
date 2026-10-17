//! Read-only resource diagnostics, folded into the common inspector's Details.
use super::*;
use editor_core::{AnimationCatalog, AnimationSession, Document, MotionSelection};
use kasane_core::document::{ExpressionBlend, ExpressionTarget, MotionTrackTarget, PosePartRef};
use std::collections::HashMap;
#[derive(PartialEq, Eq)]
struct Key {
    document: DocumentId,
    generation: u64,
    motion: Option<MotionSelection>,
    expression: Option<String>,
    locale: String,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Row {
    Hint(SharedString),
    Heading(SharedString),
    Text {
        id: Option<ElementId>,
        value: SharedString,
    },
    Value {
        id: ElementId,
        label: SharedString,
        value: SharedString,
        stacked: bool,
    },
}
impl Row {
    #[allow(dead_code)]
    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn id(&self) -> Option<ElementId> {
        match self {
            Self::Text { id, .. } => id.clone(),
            Self::Value { id, .. } => Some(id.clone()),
            _ => None,
        }
    }

    fn property(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> Self {
        Self::Value {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            stacked: false,
        }
    }
    fn field(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        value: impl Into<SharedString>,
    ) -> Self {
        Self::Value {
            id: id.into(),
            label: label.into(),
            value: value.into(),
            stacked: true,
        }
    }
    fn text(value: impl Into<SharedString>) -> Self {
        Self::Text {
            id: None,
            value: value.into(),
        }
    }
    pub(super) fn render(&self, cx: &App) -> AnyElement {
        match self {
            Self::Hint(value) => div()
                .text_color(cx.theme().muted_foreground)
                .child(value.clone())
                .into_any_element(),
            Self::Heading(value) => {
                crate::panels::animation_panel::inspectors::heading(value.clone(), cx)
                    .into_any_element()
            }
            Self::Text { id, value } => {
                let element = div().child(value.clone());
                if let Some(id) = id {
                    element.id(id.clone()).test_support().into_any_element()
                } else {
                    element.into_any_element()
                }
            }
            Self::Value {
                id,
                label,
                value,
                stacked,
            } => if *stacked {
                crate::panels::animation_panel::inspectors::field(
                    id.clone(),
                    label.clone(),
                    value.clone(),
                    cx,
                )
            } else {
                crate::panels::animation_panel::inspectors::property(
                    id.clone(),
                    label.clone(),
                    value.clone(),
                    cx,
                )
            }
            .into_any_element(),
        }
    }
}

#[derive(Default)]
pub(super) struct Details {
    key: Option<Key>,
    rows: Arc<[Row]>,
    pub(super) builds: u64,
}
impl Details {
    pub(super) fn rows(
        &mut self,
        document: &Document,
        id: DocumentId,
        animation: &AnimationSession,
    ) -> Arc<[Row]> {
        let key = Key {
            document: id,
            generation: document.properties_revision(),
            motion: animation.motion.clone(),
            expression: animation.expression.clone(),
            locale: rust_i18n::locale().to_string(),
        };
        if self.key.as_ref() != Some(&key) {
            self.rows = rows(document, animation, &document.animation_catalog()).into();
            self.key = Some(key);
            self.builds += 1;
        }
        self.rows.clone()
    }
}
fn rows(document: &Document, animation: &AnimationSession, catalog: &AnimationCatalog) -> Vec<Row> {
    let mut rows = vec![
        Row::Hint(t!("workspace.properties_hint").into()),
        Row::Heading(t!("workspace.selected_motion").into()),
    ];
    let resource = catalog
        .motions
        .iter()
        .find(|r| animation.motion.as_ref() == Some(&r.selection));
    if let Some(resource) = resource {
        rows.extend([
            Row::property(
                "animation-property-motion",
                t!("workspace.resource_name"),
                resource.name.clone(),
            ),
            Row::property(
                "animation-property-registration",
                t!("workspace.registration"),
                resource.selection.entry.as_ref().map_or_else(
                    || t!("workspace.ungrouped").into_owned(),
                    |(group, index)| format!("{group} [{}]", index + 1),
                ),
            ),
            Row::property(
                "animation-property-duration",
                t!("workspace.duration"),
                format!("{:.3} s", resource.duration),
            ),
            Row::property(
                "animation-property-fps",
                t!("workspace.frame_rate"),
                format!("{:.1} fps", resource.fps),
            ),
            Row::property(
                "animation-property-loop",
                t!("workspace.playback_mode"),
                if resource.looping {
                    t!("workspace.looping")
                } else {
                    t!("workspace.once")
                },
            ),
            Row::property(
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
            rows.push(Row::text(t!("workspace.invalid_duration")));
        }
        if let Some(sound) = &resource.sound {
            rows.push(Row::field(
                "animation-property-sound",
                t!("workspace.sound"),
                sound.clone(),
            ));
            rows.push(Row::text(t!("workspace.sound_unsupported")));
        }
    } else {
        rows.push(Row::Text {
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
    rows.push(Row::Heading(t!("workspace.selected_expression").into()));
    if let Some(resource) = catalog
        .expressions
        .iter()
        .find(|r| animation.expression.as_ref() == Some(&r.id))
    {
        rows.push(Row::property(
            "animation-property-expression",
            t!("workspace.resource_name"),
            resource.name.clone(),
        ));
        rows.push(Row::Text {
            id: Some("animation-property-expression-summary".into()),
            value: crate::panels::animation_panel::expression_info(resource).into(),
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
                rows.push(Row::field(
                    ("animation-property-expression-entry", index),
                    target,
                    format!("{blend} · {:.4}", entry.value),
                ));
            }
        }
    } else {
        rows.push(Row::Text {
            id: Some("animation-property-no-expression".into()),
            value: t!("workspace.none").into(),
        });
    }
    if let Some(clip) = resource.and_then(|r| document.motion_definition(&r.selection.clip_id)) {
        rows.push(Row::Heading(t!("workspace.track_targets").into()));
        for (index, track) in clip.tracks.iter().enumerate() {
            let target = match &track.target {
                MotionTrackTarget::Parameter { parameter_id } => parameter_name(parameter_id),
                MotionTrackTarget::PartOpacity { part_id } => {
                    crate::panels::animation_panel::inspectors::part_name(
                        document,
                        &PosePartRef::Resolved {
                            part_id: part_id.clone(),
                        },
                    )
                    .into()
                }
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
            rows.push(Row::field(
                ("animation-property-track", index),
                target,
                t!("workspace.track_segments", count = track.segments.len()),
            ));
        }
        if !clip.events.is_empty() {
            rows.push(Row::Heading(t!("workspace.motion_events").into()));
            for (index, event) in clip.events.iter().enumerate() {
                rows.push(Row::field(
                    ("animation-property-event", index),
                    format!("{:.3} s", event.time),
                    event.value.clone(),
                ));
            }
        }
    }
    rows
}

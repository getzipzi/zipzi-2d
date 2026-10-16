//! Resource rows are immutable between authoring/selection changes. Only visible
//! rows become elements; narrowing the dock must not lay out every motion track.
use super::*;
use editor_core::{AnimationCatalog, AnimationSession, Document, DocumentId, MotionSelection};
use gpui_kit::component::scroll::ScrollableElement;
use kasane_core::document::{ExpressionBlend, ExpressionTarget, MotionTrackTarget, PosePartRef};
use std::{collections::HashMap, sync::Arc};

#[derive(PartialEq, Eq)]
struct Key {
    document: DocumentId,
    generation: u64,
    motion: Option<MotionSelection>,
    expression: Option<String>,
    locale: String,
}

enum Row {
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
    fn render(&self, cx: &App) -> AnyElement {
        let element = match self {
            Self::Hint(value) => div()
                .text_color(cx.theme().muted_foreground)
                .child(value.clone())
                .into_any_element(),
            Self::Heading(value) => inspectors::heading(value.clone(), cx).into_any_element(),
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
                inspectors::field(id.clone(), label.clone(), value.clone(), cx)
            } else {
                inspectors::property(id.clone(), label.clone(), value.clone(), cx)
            }
            .into_any_element(),
        };
        div()
            .w_full()
            .px_3()
            .pb_1()
            .child(element)
            .into_any_element()
    }
}

pub(super) struct Properties {
    key: Option<Key>,
    rows: Arc<[Row]>,
    scroll: ListState,
    #[cfg(any(test, feature = "test-support"))]
    builds: u64,
}
impl Default for Properties {
    fn default() -> Self {
        Self {
            key: None,
            rows: Arc::from([]),
            scroll: ListState::new(0, ListAlignment::Top, px(16.)),
            #[cfg(any(test, feature = "test-support"))]
            builds: 0,
        }
    }
}

impl AnimationPanel {
    pub(super) fn render_properties(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let content = div()
            .id("AnimationProperties")
            .test_support()
            .key_context("KasaneDocument")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .overflow_hidden()
            .min_h_0()
            .min_w_0()
            .text_xs();
        let Some(owner) = self.owner.upgrade() else {
            return content.into_any_element();
        };
        let state = owner.read(cx);
        let (Some(document), Some(id), Some(animation)) = (
            state.editor.document(),
            state.editor.active_id(),
            state.editor.animation(),
        ) else {
            self.properties.key = None;
            self.properties.rows = Arc::from([]);
            self.properties.scroll.reset(0);
            return content
                .child(div().p_3().child(t!("editor.welcome")))
                .into_any_element();
        };
        let key = Key {
            document: id,
            generation: document.model_generation(),
            motion: animation.motion.clone(),
            expression: animation.expression.clone(),
            locale: rust_i18n::locale().to_string(),
        };
        if self.properties.key.as_ref() != Some(&key) {
            self.properties.rows = rows(document, animation, &document.animation_catalog()).into();
            self.properties
                .scroll
                .reset_with_uniform_height(self.properties.rows.len(), px(36.));
            self.properties.key = Some(key);
            #[cfg(any(test, feature = "test-support"))]
            {
                self.properties.builds += 1;
            }
        }
        let rows = self.properties.rows.clone();
        content
            .child(
                list(self.properties.scroll.clone(), move |index, _, cx| {
                    rows[index].render(cx)
                })
                .size_full()
                .py_3(),
            )
            .vertical_scrollbar(&self.properties.scroll)
            .into_any_element()
    }

    #[cfg(any(test, feature = "test-support"))]
    #[allow(dead_code)]
    pub(crate) fn properties_cache_stats(&self) -> (u64, usize) {
        (self.properties.builds, self.properties.rows.len())
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
            value: super::expression_info(resource).into(),
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
                MotionTrackTarget::PartOpacity { part_id } => inspectors::part_name(
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

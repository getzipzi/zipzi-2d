//! Timeline selections describe ordinary inspector fields; only the value and
//! command adapters know about curves, keys and events.
use super::*;
use crate::curve_authoring as curves;
use zipzi_contract::kernel_commands::MotionInterpolation;
use zipzi_contract::motion_curve::{MotionEasingPreset, MotionFrameStep};
use zipzi_editor_core::{
    Inspection, InspectionSource, TimelineCommand, TimelineKeyRef, TimelineMixed, TimelineRowId,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TimelineTarget {
    Channel {
        row: TimelineRowId,
        time: Option<u32>,
    },
    Keys(Vec<TimelineKeyRef>),
    Events(Vec<String>),
    Rows(Vec<TimelineRowId>),
    Mixed {
        keys: usize,
        events: usize,
    },
}

const fn field(name: &'static str, kind: PropertyType) -> PropertySpec {
    PropertySpec::new(ObjectKind::Motion, name, name, kind)
}
pub(crate) const TIME: PropertySpec = field("selection_time", PropertyType::Float)
    .range(0., 1e6, 0.01)
    .preview();
pub(crate) const VALUE: PropertySpec = field("selection_value", PropertyType::Float).preview();
pub(crate) const INTERPOLATION: PropertySpec = field("interpolation", PropertyType::Text);
pub(crate) const PRESET: PropertySpec = field("easing_preset", PropertyType::Text);
pub(crate) const FRAME_STEP: PropertySpec = field("frame_step", PropertyType::Double)
    .range(1., u32::MAX as f64, 1.)
    .preview();
const C1_TIME: PropertySpec = field("control1_time", PropertyType::Double).preview();
const C1_VALUE: PropertySpec = field("control1_value", PropertyType::Float).preview();
const C2_TIME: PropertySpec = field("control2_time", PropertyType::Double).preview();
const C2_VALUE: PropertySpec = field("control2_value", PropertyType::Float).preview();
pub(crate) const EVENT: PropertySpec = field("event_value", PropertyType::Text).allow_empty();

pub(super) fn target(editor: &Editor, inspection: &Inspection) -> Option<TimelineTarget> {
    let InspectionSource::Animation {
        object: None,
        channels,
        keys,
        events,
        ..
    } = &inspection.source
    else {
        return None;
    };
    if !keys.is_empty() && !events.is_empty() {
        return Some(TimelineTarget::Mixed {
            keys: keys.len(),
            events: events.len(),
        });
    }
    if !keys.is_empty() {
        return Some(TimelineTarget::Keys(keys.clone()));
    }
    if !events.is_empty() {
        return Some(TimelineTarget::Events(events.clone()));
    }
    if channels.len() == 1 && channels[0] != TimelineRowId::Events {
        return Some(TimelineTarget::Channel {
            row: channels[0].clone(),
            // Playing samples update the same controls; paused edits retain
            // their exact time so seeking cannot redirect an old draft.
            time: (!editor.animation()?.playing).then_some(editor.animation()?.edit_time.to_bits()),
        });
    }
    (!channels.is_empty()).then(|| TimelineTarget::Rows(channels.clone()))
}

fn interpolations(_: &Document) -> Vec<Choice> {
    curves::INTERPOLATIONS
        .into_iter()
        .map(|(kind, value)| Choice {
            value: value.into(),
            label: curves::kind_label(kind).into(),
        })
        .collect()
}
fn presets(_: &Document) -> Vec<Choice> {
    MotionEasingPreset::ALL
        .into_iter()
        .map(|preset| Choice {
            value: curves::preset_id(preset),
            label: curves::preset_label(preset).into(),
        })
        .collect()
}

impl TimelineTarget {
    fn projection(
        &self,
        editor: &Editor,
    ) -> Option<std::sync::Arc<zipzi_editor_core::TimelineProjection>> {
        // Draft baselines must remain committed while a candidate is rendered.
        editor.timeline_baseline_projection()
    }
    pub(super) fn definition(&self, editor: &Editor) -> FormDefinition {
        let mut fields = Vec::new();
        let Some(projection) = self.projection(editor) else {
            return FormDefinition::default();
        };
        match self {
            Self::Keys(keys) => {
                if keys.len() == 1
                    && let Some(key) = projection.key(&keys[0])
                {
                    let mut time =
                        TIME.range(0., projection.duration, 1. / f64::from(projection.fps));
                    time.readonly = !key.time_editable;
                    fields.push(FieldSpec::new(time, "timeline.time", FieldEditor::Number));
                    let mut value = VALUE;
                    value.readonly = !projection.tracks[&key.reference.track_id].editable;
                    if let Some((min, max)) = projection
                        .channel(&projection.tracks[&key.reference.track_id].channel)
                        .and_then(|channel| channel.range)
                    {
                        value = value.range(min.into(), max.into(), 0.001);
                    }
                    fields.push(FieldSpec::new(value, "timeline.value", FieldEditor::Number));
                }
                let settings = projection.curve_settings(keys, false);
                let mut specs = vec![
                    FieldSpec::new(INTERPOLATION, "timeline.interpolation", FieldEditor::Select)
                        .choices(false, interpolations),
                ];
                if settings.bezier() {
                    let mut preset = PRESET;
                    preset.readonly = settings.restricted_conversion_locked;
                    specs.push(
                        FieldSpec::new(preset, "timeline.easing", FieldEditor::Select)
                            .choices(false, presets),
                    );
                }
                specs.push(FieldSpec::new(
                    FRAME_STEP,
                    "timeline.frame_step_n",
                    FieldEditor::Number,
                ));
                if settings.keys.len() == 1 {
                    let track = &projection.tracks[&settings.keys[0].track_id];
                    if let Some(index) = track
                        .keys
                        .iter()
                        .position(|k| k.reference == settings.keys[0])
                    {
                        let end = &track.keys[index + 1].reference;
                        if let Some(handles) = projection.display_handles(end) {
                            for (mut spec, label, horizontal) in [
                                (C1_TIME, "timeline.control1_time", true),
                                (C1_VALUE, "timeline.control1_value", false),
                                (C2_TIME, "timeline.control2_time", true),
                                (C2_VALUE, "timeline.control2_value", false),
                            ] {
                                spec.readonly = if horizontal {
                                    !handles.horizontal_editable
                                } else {
                                    !handles.value_editable
                                };
                                let mut field = FieldSpec::new(spec, label, FieldEditor::Number);
                                field.display_decimals = Some(6);
                                specs.push(field);
                            }
                        }
                    }
                }
                for mut spec in specs {
                    spec.binding.readonly |= !settings.enabled();
                    spec.section = Some("timeline.interval_curve");
                    fields.push(spec);
                }
            }
            Self::Events(events) if events.len() == 1 => {
                fields.push(FieldSpec::new(
                    TIME.range(0., projection.duration, 1. / f64::from(projection.fps)),
                    "timeline.time",
                    FieldEditor::Number,
                ));
                fields.push(FieldSpec::new(
                    EVENT,
                    "timeline.event_value",
                    FieldEditor::Text,
                ));
            }
            Self::Channel { row, .. } => {
                if let Some(row) = projection.rows.iter().find(|r| &r.id == row)
                    && let Some(channel) =
                        row.channel.as_ref().and_then(|id| projection.channel(id))
                    && (row.track_id.is_some() || channel.tracks.len() <= 1)
                {
                    let (min, max) = channel.range.unwrap_or((-1e9, 1e9));
                    let mut spec = VALUE.range(min.into(), max.into(), 0.001);
                    spec.readonly = !channel.editable;
                    fields.push(FieldSpec::new(
                        spec,
                        "timeline.raw_value",
                        FieldEditor::Number,
                    ));
                }
            }
            _ => {}
        }
        for field in &mut fields {
            field.live_text = true;
            if field.binding.key == VALUE.key && field.binding.max - field.binding.min < 1e6 {
                field.drag_distance = Some(400.);
            }
        }
        let navigation = (!projection.events.is_empty()).then(|| NavigationSpec {
            label: "timeline.event_list",
            choices: projection
                .events
                .iter()
                .enumerate()
                .map(|(index, event)| Choice {
                    value: event.id.clone(),
                    label: format!("{} · {:.6}s · {}", index + 1, event.time, event.value).into(),
                })
                .collect(),
            select: select_event,
        });
        FormDefinition { fields, navigation }
    }
    pub(super) fn values(&self, editor: &Editor) -> anyhow::Result<PropertyValues> {
        let projection = self
            .projection(editor)
            .ok_or_else(|| anyhow::anyhow!("Motion no longer exists"))?;
        let mut values = PropertyValues::new();
        match self {
            Self::Keys(keys) => {
                anyhow::ensure!(
                    keys.iter().all(|key| projection.key(key).is_some()),
                    "Keys no longer exist"
                );
                if keys.len() == 1 {
                    let key = projection.key(&keys[0]).unwrap();
                    values.insert(TIME.key, PropertyValue::Float(key.time as f32));
                    values.insert(VALUE.key, PropertyValue::Float(key.value));
                }
                let settings = projection.curve_settings(keys, false);
                values.insert(
                    INTERPOLATION.key,
                    PropertyValue::Text(match settings.interpolation {
                        TimelineMixed::Uniform(kind) => curves::kind_id(kind).into(),
                        TimelineMixed::Mixed => t!("properties.mixed").into_owned(),
                        TimelineMixed::Empty => "—".into(),
                    }),
                );
                values.insert(
                    PRESET.key,
                    PropertyValue::Text(match settings.preset {
                        TimelineMixed::Uniform(Some(preset)) => curves::preset_id(preset),
                        TimelineMixed::Uniform(None) => t!("timeline.custom_curve").into_owned(),
                        _ => t!("properties.mixed").into_owned(),
                    }),
                );
                values.insert(
                    FRAME_STEP.key,
                    match settings.frame_step {
                        TimelineMixed::Uniform(MotionFrameStep::EveryFrames { frames }) => {
                            PropertyValue::Double(frames as f64)
                        }
                        TimelineMixed::Uniform(MotionFrameStep::Off {}) => {
                            PropertyValue::Double(1.)
                        }
                        _ => PropertyValue::Text(t!("properties.mixed").into_owned()),
                    },
                );
                if settings.keys.len() == 1 {
                    let track = &projection.tracks[&settings.keys[0].track_id];
                    if let Some(index) = track
                        .keys
                        .iter()
                        .position(|k| k.reference == settings.keys[0])
                        && let Some(handles) =
                            projection.display_handles(&track.keys[index + 1].reference)
                    {
                        values.insert(
                            C1_TIME.key,
                            PropertyValue::Double(handles.control1.time.into()),
                        );
                        values.insert(C1_VALUE.key, PropertyValue::Float(handles.control1.value));
                        values.insert(
                            C2_TIME.key,
                            PropertyValue::Double(handles.control2.time.into()),
                        );
                        values.insert(C2_VALUE.key, PropertyValue::Float(handles.control2.value));
                    }
                }
            }
            Self::Events(events) if events.len() == 1 => {
                let event = projection
                    .events
                    .iter()
                    .find(|event| event.id == events[0])
                    .ok_or_else(|| anyhow::anyhow!("Event no longer exists"))?;
                values.insert(TIME.key, PropertyValue::Float(event.time));
                values.insert(EVENT.key, PropertyValue::Text(event.value.clone()));
            }
            Self::Channel { row, time } => {
                let row = projection
                    .rows
                    .iter()
                    .find(|r| &r.id == row)
                    .ok_or_else(|| anyhow::anyhow!("Channel no longer exists"))?;
                if let Some(channel) = row.channel.as_ref().and_then(|id| projection.channel(id)) {
                    let value =
                        projection
                            .raw_value(
                                &channel.id,
                                row.track_id.as_deref(),
                                f64::from(time.map(f32::from_bits).unwrap_or_else(|| {
                                    editor.animation().map_or(0., |a| a.edit_time)
                                })),
                            )
                            .unwrap_or(channel.default_value);
                    values.insert(VALUE.key, PropertyValue::Float(value));
                }
            }
            _ => {}
        }
        Ok(values)
    }
    pub(super) fn command(
        &self,
        editor: &Editor,
        changes: &PropertyValues,
    ) -> anyhow::Result<TimelineCommand> {
        let projection = self
            .projection(editor)
            .ok_or_else(|| anyhow::anyhow!("Motion no longer exists"))?;
        let (&id, value) = changes
            .first_key_value()
            .ok_or_else(|| anyhow::anyhow!("No property change"))?;
        anyhow::ensure!(changes.len() == 1, "Edit one property at a time");
        let definition = self.definition(editor);
        let spec = definition
            .field(id)
            .ok_or_else(|| anyhow::anyhow!("Property no longer exists"))?;
        anyhow::ensure!(!spec.binding.readonly, "This property is read only");
        spec.binding.validate(value)?;
        Ok(match self {
            Self::Keys(keys) if id == INTERPOLATION.key => {
                let PropertyValue::Text(kind) = value else {
                    anyhow::bail!("Invalid interpolation")
                };
                let kind = match kind.as_str() {
                    "linear" => MotionInterpolation::Linear,
                    "auto_smooth" => MotionInterpolation::AutoSmooth,
                    "bezier" => MotionInterpolation::Bezier,
                    "stepped" => MotionInterpolation::Stepped,
                    "inverse" => MotionInterpolation::InverseStepped,
                    _ => anyhow::bail!("Choose an interpolation"),
                };
                TimelineCommand::Interpolation {
                    keys: keys.clone(),
                    kind,
                }
            }
            Self::Keys(keys) if id == PRESET.key => {
                let PropertyValue::Text(value) = value else {
                    anyhow::bail!("Choose a preset")
                };
                let preset = serde_json::from_value(serde_json::Value::String(value.clone()))?;
                TimelineCommand::EasingPreset {
                    keys: keys.clone(),
                    preset,
                }
            }
            Self::Keys(keys) if id == FRAME_STEP.key => {
                let n = value
                    .number()
                    .ok_or_else(|| anyhow::anyhow!("Enter a positive integer"))?;
                anyhow::ensure!(
                    n.fract() == 0. && (1. ..=u32::MAX as f64).contains(&n),
                    "Enter a positive integer"
                );
                TimelineCommand::FrameStep {
                    keys: keys.clone(),
                    frame_step: MotionFrameStep::from_frames(n as u32).unwrap(),
                }
            }
            Self::Keys(keys)
                if [C1_TIME.key, C1_VALUE.key, C2_TIME.key, C2_VALUE.key].contains(&id) =>
            {
                let settings = projection.curve_settings(keys, false);
                anyhow::ensure!(
                    settings.enabled() && settings.keys.len() == 1,
                    "Select one editable interval"
                );
                let track = &projection.tracks[&settings.keys[0].track_id];
                let index = track
                    .keys
                    .iter()
                    .position(|k| k.reference == settings.keys[0])
                    .unwrap();
                let end = track.keys[index + 1].reference.clone();
                let handles = projection
                    .display_handles(&end)
                    .ok_or_else(|| anyhow::anyhow!("No handles"))?;
                let first = id == C1_TIME.key || id == C1_VALUE.key;
                let point = if first {
                    handles.control1
                } else {
                    handles.control2
                };
                let handle = if first {
                    zipzi_sdk::MotionBezierHandle::Control1
                } else {
                    zipzi_sdk::MotionBezierHandle::Control2
                };
                let time = if id == C1_TIME.key || id == C2_TIME.key {
                    value.number().unwrap()
                } else {
                    point.time.into()
                };
                let value = if id == C1_VALUE.key || id == C2_VALUE.key {
                    value.number().unwrap() as f32
                } else {
                    point.value
                };
                if editor
                    .timeline_view()
                    .is_some_and(|view| view.aligned_handles)
                {
                    TimelineCommand::AlignedHandle {
                        end,
                        handle,
                        time,
                        value,
                    }
                } else {
                    TimelineCommand::Handle {
                        end,
                        handle,
                        time,
                        value,
                    }
                }
            }
            Self::Keys(keys) if keys.len() == 1 => {
                let key = projection
                    .key(&keys[0])
                    .ok_or_else(|| anyhow::anyhow!("Key no longer exists"))?;
                TimelineCommand::SetKey {
                    key: keys[0].clone(),
                    time: if id == TIME.key {
                        value.number().unwrap()
                    } else {
                        key.time
                    },
                    value: if id == VALUE.key {
                        value.number().unwrap() as f32
                    } else {
                        key.value
                    },
                }
            }
            Self::Events(events) if events.len() == 1 => {
                let event = projection
                    .events
                    .iter()
                    .find(|event| event.id == events[0])
                    .ok_or_else(|| anyhow::anyhow!("Event no longer exists"))?;
                TimelineCommand::Event {
                    id: Some(event.id.clone()),
                    time: if id == TIME.key {
                        value.number().unwrap()
                    } else {
                        event.time.into()
                    },
                    value: if let PropertyValue::Text(text) = value {
                        text.clone()
                    } else {
                        event.value.clone()
                    },
                }
            }
            Self::Channel { row, time } => {
                anyhow::ensure!(
                    editor
                        .animation()
                        .is_some_and(|a| Some(a.edit_time.to_bits()) == *time),
                    "Editing time changed"
                );
                let row = projection
                    .rows
                    .iter()
                    .find(|r| &r.id == row)
                    .ok_or_else(|| anyhow::anyhow!("Channel no longer exists"))?;
                TimelineCommand::WriteValue {
                    channel: row
                        .channel
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("Select a channel"))?,
                    track_id: row.track_id.clone(),
                    selected_key: editor
                        .timeline_view()
                        .and_then(|state| {
                            state.keys.iter().find(|key| {
                                row.keys.contains(key)
                                    && projection.key(key).is_some_and(|key| {
                                        key.time
                                            == f64::from(time.map(f32::from_bits).unwrap_or_else(
                                                || editor.animation().map_or(0., |a| a.edit_time),
                                            ))
                                    })
                            })
                        })
                        .cloned(),
                    value: value.number().unwrap() as f32,
                }
            }
            _ => anyhow::bail!("Select one editable item"),
        })
    }
    pub(crate) fn title(&self, editor: &Editor) -> SharedString {
        match self {
            Self::Keys(keys) => t!("timeline.selected_keys", count = keys.len()).into(),
            Self::Events(events) => t!("properties.selected_events", count = events.len()).into(),
            Self::Channel { row, .. } => self
                .projection(editor)
                .and_then(|p| {
                    p.rows
                        .iter()
                        .find(|r| &r.id == row)
                        .map(|row| row.name.clone())
                })
                .unwrap_or_else(|| t!("timeline.raw_value").into_owned())
                .into(),
            Self::Rows(rows) => t!("properties.selected_channels", count = rows.len()).into(),
            Self::Mixed { keys, events } => {
                t!("properties.selected_items", count = keys + events).into()
            }
        }
    }
}

fn select_event(editor: &mut Editor, target: &PropertyTarget, id: &str) -> anyhow::Result<()> {
    anyhow::ensure!(
        super::target(editor).as_ref() == Some(target)
            && editor.timeline_projection().is_some_and(|p| {
                p.clip_id == target.object.id && p.events.iter().any(|event| event.id == id)
            }),
        "Selection changed"
    );
    editor.update_timeline_view(target.document, |view| {
        view.keys.clear();
        view.events.clear();
        view.events.insert(id.to_owned());
        view.channels.clear();
        view.channels.insert(TimelineRowId::Events);
        view.curve_focus = None;
        view.reveal_row = Some(TimelineRowId::Events);
    })
}

pub(crate) fn details(target: &PropertyTarget, editor: &Editor) -> Vec<DetailRow> {
    let Some(selection) = &target.timeline else {
        return vec![];
    };
    let mut rows = Vec::new();
    if let TimelineTarget::Keys(keys) = selection
        && let Some(projection) = selection.projection(editor)
    {
        let settings = projection.curve_settings(keys, false);
        rows.push(DetailRow::Hint(curves::settings_hint(&settings).into()));
        if settings.restricted_conversion_locked {
            rows.push(DetailRow::Hint(
                t!("timeline.preset_conversion_locked").into(),
            ));
        }
        if let TimelineMixed::Uniform(Some(preset)) = settings.preset
            && preset.definition().approximate
        {
            rows.push(DetailRow::Hint(t!("timeline.approximate_preset").into()));
        }
        rows.push(DetailRow::Hint(t!("timeline.step_semantics").into()));
    }
    if matches!(selection, TimelineTarget::Mixed { .. }) {
        rows.push(DetailRow::Hint(t!("timeline.curve_events_selected").into()));
    }
    if let TimelineTarget::Channel { row, .. } = selection
        && let Some(projection) = selection.projection(editor)
        && let Some(row) = projection.rows.iter().find(|r| &r.id == row)
        && let Some(channel) = row.channel.as_ref().and_then(|id| projection.channel(id))
    {
        rows.push(DetailRow::property(
            "property-channel-target",
            t!("properties.channel_target"),
            channel.runtime_id.clone(),
        ));
        if let Some((min, max)) = channel.range {
            rows.push(DetailRow::property(
                "property-channel-range",
                t!("properties.channel_range"),
                format!("{min} … {max}"),
            ));
        }
        for diagnostic in &channel.diagnostics {
            rows.push(DetailRow::text(t!(match diagnostic {
                zipzi_editor_core::TimelineDiagnostic::OpaqueClip => "timeline.opaque",
                zipzi_editor_core::TimelineDiagnostic::UnresolvedTarget =>
                    "timeline.unresolved_hint",
                zipzi_editor_core::TimelineDiagnostic::UnsupportedModelChannel =>
                    "timeline.unsupported",
                zipzi_editor_core::TimelineDiagnostic::PartOpacityWithoutPose => "timeline.no_pose",
                zipzi_editor_core::TimelineDiagnostic::NonMonotonicBezier =>
                    "timeline.non_monotonic",
            })));
        }
    }
    if editor.animation().is_some_and(|a| a.playing) {
        rows.push(DetailRow::Hint(t!("timeline.pause_to_edit").into()));
    }
    rows
}

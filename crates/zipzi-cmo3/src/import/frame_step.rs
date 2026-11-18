//! Adapt native model/group frame-step timelines to segment-local authoring.
use super::{
    graph::{Graph, Node},
    invalid, Result,
};
use zipzi_animation::{sample_motion_curve_base, CompiledCurve};
use zipzi_core::{
    document::{
        CdiParameterEntry, MotionCurve, MotionFrameStep, MotionKey, MotionSegment, MotionTrack,
        MotionTrackTarget,
    },
    state::DocumentState,
};

#[derive(Clone, Default)]
pub(super) struct Steps(Vec<(i32, u32)>);
impl Steps {
    pub fn read(g: &Graph<'_>, attr: Node<'_>) -> Result<Self> {
        if attr.has_tag_name("null") {
            return Ok(Self::default());
        }
        let sequence = g.req(attr, "valueData")?;
        let mut keys = Vec::new();
        for point in g.list(sequence, "points")? {
            let frame = g.number::<i32>(point, "pos")?;
            let value = g.number::<f64>(point, "doubleValue")?;
            if !value.is_finite()
                || value.fract() != 0.
                || !(0. ..=i32::MAX as f64).contains(&value)
            {
                return Err(invalid("Invalid native CAN3 Frame Step count"));
            }
            if keys.last().is_some_and(|&(previous, _)| previous >= frame) {
                return Err(invalid("CAN3 Frame Step keys must be strictly ordered"));
            }
            keys.push((frame, value as u32));
        }
        Ok(Self(keys))
    }
    fn setting(&self, frame: i32) -> Option<(i32, u32)> {
        self.0
            .get(self.0.partition_point(|k| k.0 <= frame).checked_sub(1)?)
            .copied()
    }
    fn stepped_position(&self, position: i32) -> i32 {
        match self.setting(position) {
            Some((origin, 0)) => origin,
            Some((origin, n)) => {
                (i64::from(origin)
                    + (i64::from(position) - i64::from(origin)) / i64::from(n) * i64::from(n))
                    as i32
            }
            None => position,
        }
    }
    pub fn active(&self) -> bool {
        self.0.iter().any(|&(_, n)| n != 1)
    }

    pub fn apply(
        &self,
        track: &mut MotionTrack,
        fps: f32,
        restricted: bool,
        budget: &mut usize,
    ) -> Result<()> {
        if self.0.is_empty() || !self.active() {
            return Ok(());
        }
        let frame = |time: f32| (f64::from(time) * f64::from(fps)).round() as i32;
        let mut counts = Vec::new();
        let mut start = frame(track.initial.time);
        let mut compatible = true;
        for segment in track.segments.iter() {
            let end = frame(segment.end.time);
            let nonmonotonic = matches!(segment.curve, MotionCurve::Bezier { control1, control2, .. }
                if !restricted && !zipzi_animation::motion_bezier_time_is_monotonic(
                    (f64::from(start) / f64::from(fps)) as f32,
                    control1.time, control2.time, segment.end.time));
            let (origin, n) = self.setting(start).unwrap_or((start, 1));
            let next = self.0.partition_point(|k| k.0 <= start);
            compatible &= n != 0
                && (n == 1 || (i64::from(start) - i64::from(origin)) % i64::from(n) == 0)
                && !self.0.get(next).is_some_and(|k| k.0 < end)
                && self.stepped_position(end) == end
                && !matches!(segment.curve, MotionCurve::InverseStepped {})
                && !nonmonotonic;
            counts.push(MotionFrameStep::from_frames(n).unwrap_or_default());
            start = end;
        }
        if compatible {
            for (segment, step) in std::sync::Arc::make_mut(&mut track.segments)
                .iter_mut()
                .zip(counts)
            {
                segment.frame_step = step;
            }
            return Ok(());
        }
        // A native frame-step key can lie inside a numeric segment or keep its
        // phase across ordinary keys. Those phases have no segment-local N
        // equivalent. Preserve native displayed values as ordinary Step spans.
        let start = frame(track.initial.time).min(self.0[0].0).min(0);
        let end = frame(track.keys().last().unwrap().time).max(self.0.last().unwrap().0);
        // Native stepping can delay the last numeric anchor past its own frame.
        // Include that final update before the imported curve holds its tail.
        let end = match self.setting(end) {
            Some((origin, n @ 2..)) => {
                let delta = i64::from(end) - i64::from(origin);
                i32::try_from(
                    i64::from(origin) + (delta + i64::from(n) - 1) / i64::from(n) * i64::from(n),
                )
                .map_err(|_| invalid("CAN3 Frame Step tail exceeds the frame range"))?
            }
            _ => end,
        };
        let count = usize::try_from(i64::from(end) - i64::from(start) + 1)
            .map_err(|_| invalid("CAN3 Frame Step import size overflow"))?;
        if count > 1_000_000 {
            return Err(invalid(
                "CAN3 Frame Step import exceeds one million samples",
            ));
        }
        *budget = budget
            .checked_sub(count)
            .ok_or_else(|| invalid("CAN3 Frame Step import exceeds its workspace budget"))?;
        let source = CompiledCurve::from_track(track, fps);
        let anchors: Vec<_> = track
            .keys()
            .map(|key| (frame(key.time), key.value))
            .collect();
        let sample = |position| {
            let stepped = self.stepped_position(position);
            match anchors.binary_search_by_key(&stepped, |key| key.0) {
                Ok(index) => anchors[index].1,
                Err(_) => sample_motion_curve_base(
                    &source,
                    (f64::from(stepped) / f64::from(fps)) as f32,
                    restricted,
                ),
            }
        };
        let key = |frame, value| MotionKey {
            id: uuid::Uuid::new_v4().to_string(),
            time: (f64::from(frame) / f64::from(fps)) as f32,
            value,
        };
        let initial = key(start, sample(start));
        let mut previous = initial.value;
        let mut segments = Vec::new();
        for position in i64::from(start) + 1..=i64::from(end) {
            let position = position as i32;
            let value = sample(position);
            if value != previous || position == end {
                segments.push(MotionSegment::new(
                    key(position, value),
                    MotionCurve::Stepped {},
                ));
            }
            previous = value;
        }
        track.initial = initial;
        track.segments = std::sync::Arc::new(segments);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn track() -> MotionTrack {
        let key = |time, value| MotionKey {
            id: uuid::Uuid::new_v4().to_string(),
            time,
            value,
        };
        MotionTrack {
            id: uuid::Uuid::new_v4().to_string(),
            target: MotionTrackTarget::Model {
                runtime_id: "Opacity".into(),
            },
            initial: key(0., 0.),
            segments: std::sync::Arc::new(vec![
                MotionSegment::new(key(4. / 30., 1.), MotionCurve::AutoSmooth {}),
                MotionSegment::new(key(1., 0.2), MotionCurve::AutoSmooth {}),
            ]),
            fade_in: None,
            fade_out: None,
            extensions: Default::default(),
        }
    }
    #[test]
    fn native_phase_across_ordinary_keys_is_baked_instead_of_restarted() {
        let original = track();
        let source = CompiledCurve::from_track(&original, 30.);
        let mut result = original;
        Steps(vec![(0, 3), (30, 1)])
            .apply(&mut result, 30., false, &mut 100)
            .unwrap();
        assert!(result.segments.iter().all(|segment| matches!(
            segment.curve,
            MotionCurve::Stepped {}
        ) && segment.frame_step
            == MotionFrameStep::Off {}));
        let imported = CompiledCurve::from_track(&result, 30.);
        for frame in 0..=30 {
            let q = if frame == 30 { 30 } else { frame / 3 * 3 };
            let expected = sample_motion_curve_base(&source, (q as f64 / 30.) as f32, false);
            assert!(
                (zipzi_animation::sample_motion_curve(
                    &imported,
                    (frame as f64 / 30.) as f32,
                    false
                ) - expected)
                    .abs()
                    < 0.00001
            );
        }
    }
    #[test]
    fn native_changes_inside_a_curve_and_zero_frame_freeze_are_preserved() {
        let original = track();
        let source = CompiledCurve::from_track(&original, 30.);
        let mut result = original;
        Steps(vec![(0, 3), (5, 0), (13, 2), (30, 1)])
            .apply(&mut result, 30., false, &mut 100)
            .unwrap();
        let imported = CompiledCurve::from_track(&result, 30.);
        for frame in 0..=30 {
            let q = match frame {
                0..=4 => frame / 3 * 3,
                5..=12 => 5,
                13..=29 => 13 + (frame - 13) / 2 * 2,
                _ => 30,
            };
            assert!(
                (zipzi_animation::sample_motion_curve(
                    &imported,
                    (frame as f64 / 30.) as f32,
                    false
                ) - sample_motion_curve_base(&source, (q as f64 / 30.) as f32, false))
                .abs()
                    < 0.00001
            );
        }
    }
    #[test]
    fn native_phase_expansion_checks_the_import_budget() {
        let mut source = track();
        let error = Steps(vec![(0, 3), (30, 1)])
            .apply(&mut source, 30., false, &mut 1)
            .unwrap_err();
        assert!(error.to_string().contains("workspace budget"));
    }

    #[test]
    fn native_step_can_delay_the_final_numeric_anchor() {
        let mut original = track();
        original.segments = std::sync::Arc::new(vec![MotionSegment::new(
            MotionKey {
                id: uuid::Uuid::new_v4().to_string(),
                time: 31. / 30.,
                value: 1.,
            },
            MotionCurve::Linear {},
        )]);
        let source = CompiledCurve::from_track(&original, 30.);
        let mut result = original;
        Steps(vec![(0, 3)])
            .apply(&mut result, 30., false, &mut 100)
            .unwrap();
        assert_eq!(result.keys().last().unwrap().time, 33. / 30.);
        let imported = CompiledCurve::from_track(&result, 30.);
        for frame in 0..=36 {
            let time = (frame as f64 / 30.) as f32;
            let quantized = ((frame / 3 * 3) as f64 / 30.) as f32;
            assert!(
                (zipzi_animation::sample_motion_curve(&imported, time, false)
                    - sample_motion_curve_base(&source, quantized, false))
                .abs()
                    < 0.00001
            );
        }
    }

    #[test]
    fn group_overrides_distinguish_inheritance_empty_and_synthetic_root() {
        let attr = |name, n| {
            format!(
            "<CMvAttrI xs.n='{name}'><CIntSequence xs.n='valueData'><list xs.n='points'>{}</list></CIntSequence></CMvAttrI>",
            if n == 0 { String::new() } else { format!("<CSeqPt><i xs.n='pos'>0</i><d xs.n='doubleValue'>{n}</d></CSeqPt>") }
        )
        };
        let group = |id, guid, step| {
            format!(
            "<CMvParameter_Group><CMvParameterGroupId xs.n='id' idstr='{id}'/><CParameterGroupGuid xs.n='guid' uuid='{guid}'/>{step}</CMvParameter_Group>"
        )
        };
        let xml = format!(
            "<source><visual xs.n='visualEffect'>{}</visual><effect xs.n='keyParamEffect'><list xs.n='parameterGroupList'>{}{}{}{}</list></effect></source>",
            attr("attrFrameStep", 3),
            group("ParamGroupRoot", "root", attr("attrFrameStep", 2)),
            group("Own", "own", attr("attrFrameStep", 7)),
            group("Inherit", "inherit", "<null xs.n='attrFrameStep'/>".into()),
            group("Continuous", "off", attr("attrFrameStep", 0)),
        );
        let parsed = roxmltree::Document::parse(&xml).unwrap();
        let graph = Graph::new(&parsed).unwrap();
        let settings = Settings::read(&graph, parsed.root_element()).unwrap();
        assert_eq!(settings.parameter_steps(None).setting(0), Some((0, 2)));
        assert_eq!(
            settings.parameter_steps(Some("own")).setting(0),
            Some((0, 7))
        );
        assert_eq!(
            settings.parameter_steps(Some("inherit")).setting(0),
            Some((0, 3))
        );
        assert!(settings.parameter_steps(Some("off")).0.is_empty());
        assert_eq!(settings.global.setting(0), Some((0, 3)));
    }
}

pub(super) struct Settings {
    global: Steps,
    root: Option<Steps>,
    groups: std::collections::BTreeMap<String, Steps>,
}
impl Settings {
    pub fn read(g: &Graph<'_>, source: Node<'_>) -> Result<Self> {
        let global = if let Some(visual) = g.field(source, "visualEffect")? {
            if let Some(attr) = g.field(visual, "attrFrameStep")? {
                Steps::read(g, attr)?
            } else {
                Steps::default()
            }
        } else {
            Steps::default()
        };
        let mut groups = std::collections::BTreeMap::new();
        let mut root = None;
        if let Some(effect) = g.field(source, "keyParamEffect")? {
            for group in g.list(effect, "parameterGroupList")? {
                if let Some(attr) = g.field(group, "attrFrameStep")? {
                    // Native saves include parameter groups without an override.
                    // Null means inherit the model schedule, not disable it.
                    if attr.has_tag_name("null") {
                        continue;
                    }
                    let steps = Steps::read(g, attr)?;
                    if g.runtime(group)? == "ParamGroupRoot" {
                        // The model importer removes Cubism's synthetic root
                        // folder from CDI; ungrouped parameters still use it.
                        root = Some(steps);
                    } else {
                        groups.insert(g.guid(group, "guid")?, steps);
                    }
                }
            }
        }
        Ok(Self {
            global,
            root,
            groups,
        })
    }
    pub fn active(&self) -> bool {
        self.global.active()
            || self.root.as_ref().is_some_and(Steps::active)
            || self.groups.values().any(Steps::active)
    }
    fn parameter_steps(&self, group: Option<&str>) -> &Steps {
        group
            .map_or(self.root.as_ref(), |group| self.groups.get(group))
            .unwrap_or(&self.global)
    }
    pub fn apply(
        &self,
        track: &mut MotionTrack,
        document: &DocumentState,
        fps: f32,
        restricted: bool,
        budget: &mut usize,
    ) -> Result<()> {
        let mut steps = &self.global;
        if let MotionTrackTarget::Parameter { parameter_id } = &track.target {
            let group = document
                .display_info()
                .parameters
                .as_ref()
                .and_then(|entries| {
                    entries.iter().find_map(|entry| match entry {
                        CdiParameterEntry::Resolved {
                            parameter_id: id,
                            group_id,
                            ..
                        } if id == parameter_id => Some(group_id.as_deref()),
                        _ => None,
                    })
                })
                .flatten();
            steps = self.parameter_steps(group);
        }
        steps.apply(track, fps, restricted, budget)
    }
}

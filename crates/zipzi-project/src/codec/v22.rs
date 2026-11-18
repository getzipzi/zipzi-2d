//! Native v22 kept the curve tag and its endpoint in the same object.
use serde::Deserialize;
use serde_json::Value;
use zipzi_core::document::{MotionClip, MotionCurve, MotionKey, MotionPoint, MotionSegment};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum V22Segment {
    Linear {
        end: MotionKey,
    },
    Bezier {
        control1: MotionPoint,
        control2: MotionPoint,
        end: MotionKey,
    },
    Stepped {
        end: MotionKey,
    },
    InverseStepped {
        end: MotionKey,
    },
}

impl From<V22Segment> for MotionSegment {
    fn from(segment: V22Segment) -> Self {
        match segment {
            V22Segment::Linear { end } => Self::new(end, MotionCurve::Linear {}),
            V22Segment::Bezier {
                control1,
                control2,
                end,
            } => Self::new(
                end,
                MotionCurve::Bezier {
                    control1,
                    control2,
                    preset: None,
                },
            ),
            V22Segment::Stepped { end } => Self::new(end, MotionCurve::Stepped {}),
            V22Segment::InverseStepped { end } => Self::new(end, MotionCurve::InverseStepped {}),
        }
    }
}

#[derive(Deserialize)]
#[serde(try_from = "Value")]
pub(super) struct V22Motion(MotionClip);

impl TryFrom<Value> for V22Motion {
    type Error = serde_json::Error;

    fn try_from(mut value: Value) -> Result<Self, Self::Error> {
        use serde::de::Error;
        // Materialize one legacy clip at a time. Decode its unchanged fields with
        // the current contract, without making geometry or v23 take this path.
        let tracks = value
            .get_mut("tracks")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| Self::Error::custom("v22 motion requires a tracks array"))?;
        let mut converted = Vec::with_capacity(tracks.len());
        for track in tracks {
            let segments = track
                .get_mut("segments")
                .ok_or_else(|| Self::Error::missing_field("segments"))?;
            let legacy = std::mem::replace(segments, Value::Array(vec![]));
            let segments = Vec::<V22Segment>::deserialize(legacy)?;
            converted.push(segments.into_iter().map(MotionSegment::from).collect());
        }
        let mut clip = MotionClip::deserialize(value)?;
        for (track, segments) in clip.tracks.iter_mut().zip(converted) {
            track.segments = std::sync::Arc::new(segments);
        }
        crate::motion::upgrade_v22_content_hash(&mut clip);
        Ok(Self(clip))
    }
}

impl From<V22Motion> for MotionClip {
    fn from(motion: V22Motion) -> Self {
        motion.0
    }
}

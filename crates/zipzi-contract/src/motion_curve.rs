//! Shared curve-authoring vocabulary and the versioned, normalized easing table.
//!
//! Presets name a shortcut for explicit Bezier controls, rather than an analytic
//! easing function. Persist the mapped controls as well as their preset source;
//! changing this table must never change the shape of an existing motion.

use serde::{Deserialize, Serialize};

/// Source of explicit Bezier controls. Absence of a source means Custom; imported
/// or manually edited controls must not be matched back to a preset by tolerance.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(rename_all = "snake_case")]
pub enum MotionEasingPreset {
    /// CSS-style ease, defined directly by its Bezier controls.
    Ease,
    /// CSS-style ease in, defined directly by its Bezier controls.
    EaseIn,
    /// CSS-style ease out, defined directly by its Bezier controls.
    EaseOut,
    /// CSS-style ease in out, defined directly by its Bezier controls.
    EaseInOut,
    /// Cubic acceleration, exact apart from floating-point rounding.
    CubicIn,
    /// Cubic deceleration, exact apart from floating-point rounding.
    CubicOut,
    /// Single-Bezier approximation of piecewise cubic easing.
    CubicInOut,
    /// Single-Bezier approximation of sine acceleration.
    SineIn,
    /// Single-Bezier approximation of sine deceleration.
    SineOut,
    /// Single-Bezier approximation of sine easing at both ends.
    SineInOut,
    /// Quadratic acceleration, exact apart from floating-point rounding.
    QuadIn,
    /// Quadratic deceleration, exact apart from floating-point rounding.
    QuadOut,
    /// Single-Bezier approximation of piecewise quadratic easing.
    QuadInOut,
    /// Single-Bezier approximation of quartic acceleration.
    QuartIn,
    /// Single-Bezier approximation of quartic deceleration.
    QuartOut,
    /// Single-Bezier approximation of piecewise quartic easing.
    QuartInOut,
    /// Single-Bezier approximation of quintic acceleration.
    QuintIn,
    /// Single-Bezier approximation of quintic deceleration.
    QuintOut,
    /// Single-Bezier approximation of piecewise quintic easing.
    QuintInOut,
    /// Single-Bezier approximation of exponential acceleration.
    ExpoIn,
    /// Single-Bezier approximation of exponential deceleration.
    ExpoOut,
    /// Single-Bezier approximation of piecewise exponential easing.
    ExpoInOut,
    /// Single-Bezier approximation of circular acceleration.
    CircIn,
    /// Single-Bezier approximation of circular deceleration.
    CircOut,
    /// Single-Bezier approximation of piecewise circular easing.
    CircInOut,
}

/// Static presentation and shape information shared by all authoring clients.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotionEasingPresetDefinition {
    /// Canonical English name; clients may localize it using the preset identity.
    pub name: &'static str,
    /// Normalized `(time, value)` pairs for the first and second controls.
    pub controls: [[f32; 2]; 2],
    /// Whether the controls approximate the named analytic easing function.
    pub approximate: bool,
}

impl MotionEasingPreset {
    /// Map normalized controls to finite increasing-time endpoints using f64
    /// intermediate arithmetic. Shared with document validation and authoring.
    pub fn mapped_controls(
        self,
        start: crate::document::MotionPoint,
        end: crate::document::MotionPoint,
    ) -> Option<[crate::document::MotionPoint; 2]> {
        if ![start.time, start.value, end.time, end.value]
            .into_iter()
            .all(f32::is_finite)
            || start.time >= end.time
        {
            return None;
        }
        Some(
            self.definition()
                .controls
                .map(|[x, y]| crate::document::MotionPoint {
                    time: (f64::from(start.time)
                        + f64::from(x) * (f64::from(end.time) - f64::from(start.time)))
                        as f32,
                    value: (f64::from(start.value)
                        + f64::from(y) * (f64::from(end.value) - f64::from(start.value)))
                        as f32,
                }),
        )
    }

    /// Restricted clips parameterize time linearly and can retain only presets
    /// whose normalized time controls are the two thirds.
    pub fn supports_restricted_time(self) -> bool {
        let [[first, _], [second, _]] = self.definition().controls;
        first == 1.0 / 3.0 && second == 2.0 / 3.0
    }

    /// Stable menu order: basic, Cubic, then Sine/Quad/Quart/Quint/Expo/Circ.
    pub const ALL: [Self; 25] = [
        Self::Ease,
        Self::EaseIn,
        Self::EaseOut,
        Self::EaseInOut,
        Self::CubicIn,
        Self::CubicOut,
        Self::CubicInOut,
        Self::SineIn,
        Self::SineOut,
        Self::SineInOut,
        Self::QuadIn,
        Self::QuadOut,
        Self::QuadInOut,
        Self::QuartIn,
        Self::QuartOut,
        Self::QuartInOut,
        Self::QuintIn,
        Self::QuintOut,
        Self::QuintInOut,
        Self::ExpoIn,
        Self::ExpoOut,
        Self::ExpoInOut,
        Self::CircIn,
        Self::CircOut,
        Self::CircInOut,
    ];

    /// Version-one control table. Time controls may cross while the cubic time
    /// coordinate remains monotone; do not sort or clamp them against each other.
    pub const fn definition(self) -> MotionEasingPresetDefinition {
        use MotionEasingPreset::*;
        let (name, controls, approximate) = match self {
            Ease => ("Ease", [[0.25, 0.1], [0.25, 1.0]], false),
            EaseIn => ("Ease In", [[0.42, 0.0], [1.0, 1.0]], false),
            EaseOut => ("Ease Out", [[0.0, 0.0], [0.58, 1.0]], false),
            EaseInOut => ("Ease In Out", [[0.42, 0.0], [0.58, 1.0]], false),
            CubicIn => ("Cubic In", [[1.0 / 3.0, 0.0], [2.0 / 3.0, 0.0]], false),
            CubicOut => ("Cubic Out", [[1.0 / 3.0, 1.0], [2.0 / 3.0, 1.0]], false),
            CubicInOut => ("Cubic In Out", [[0.65, 0.0], [0.35, 1.0]], true),
            SineIn => ("Sine In", [[0.12, 0.0], [0.39, 0.0]], true),
            SineOut => ("Sine Out", [[0.61, 1.0], [0.88, 1.0]], true),
            SineInOut => ("Sine In Out", [[0.37, 0.0], [0.63, 1.0]], true),
            QuadIn => ("Quad In", [[1.0 / 3.0, 0.0], [2.0 / 3.0, 1.0 / 3.0]], false),
            QuadOut => (
                "Quad Out",
                [[1.0 / 3.0, 2.0 / 3.0], [2.0 / 3.0, 1.0]],
                false,
            ),
            QuadInOut => ("Quad In Out", [[0.45, 0.0], [0.55, 1.0]], true),
            QuartIn => ("Quart In", [[0.5, 0.0], [0.75, 0.0]], true),
            QuartOut => ("Quart Out", [[0.25, 1.0], [0.5, 1.0]], true),
            QuartInOut => ("Quart In Out", [[0.76, 0.0], [0.24, 1.0]], true),
            QuintIn => ("Quint In", [[0.64, 0.0], [0.78, 0.0]], true),
            QuintOut => ("Quint Out", [[0.22, 1.0], [0.36, 1.0]], true),
            QuintInOut => ("Quint In Out", [[0.83, 0.0], [0.17, 1.0]], true),
            ExpoIn => ("Expo In", [[0.7, 0.0], [0.84, 0.0]], true),
            ExpoOut => ("Expo Out", [[0.16, 1.0], [0.3, 1.0]], true),
            ExpoInOut => ("Expo In Out", [[0.87, 0.0], [0.13, 1.0]], true),
            CircIn => ("Circ In", [[0.55, 0.0], [1.0, 0.45]], true),
            CircOut => ("Circ Out", [[0.0, 0.55], [0.45, 1.0]], true),
            CircInOut => ("Circ In Out", [[0.85, 0.0], [0.15, 1.0]], true),
        };
        MotionEasingPresetDefinition {
            name,
            controls,
            approximate,
        }
    }
}

/// Segment-local sample-and-hold, independent of the base interpolation.
/// One frame means continuous sampling and is canonicalized to Off.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MotionFrameStep {
    /// Evaluate continuously, including at subframe times.
    Off {},
    /// Hold each base-curve sample for this many frames at the clip's FPS.
    EveryFrames {
        /// Positive frame count. Zero is invalid; one normalizes to Off.
        frames: u32,
    },
}

impl MotionFrameStep {
    /// Construct canonical authoring data, rejecting zero without limiting custom
    /// counts to the two- and three-frame shortcuts.
    pub const fn from_frames(frames: u32) -> Option<Self> {
        match frames {
            0 => None,
            1 => Some(Self::Off {}),
            _ => Some(Self::EveryFrames { frames }),
        }
    }

    /// Validate and canonicalize data received through a codec or direct enum
    /// construction. Contract deserialization alone does not validate documents.
    pub const fn normalized(self) -> Option<Self> {
        match self {
            Self::Off {} => Some(Self::Off {}),
            Self::EveryFrames { frames } => Self::from_frames(frames),
        }
    }
}

impl Default for MotionFrameStep {
    fn default() -> Self {
        Self::Off {}
    }
}

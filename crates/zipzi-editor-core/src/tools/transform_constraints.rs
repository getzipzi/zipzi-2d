//! Live modifier policy shared by object frames and native deformer controls.
use crate::control_edit::ControlChange;
use crate::{ModelVector, ViewportPoint};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct TransformConstraints {
    pub shift: bool,
    pub alt: bool,
}

#[derive(Clone, Copy)]
pub(super) enum TranslationAxis {
    Horizontal,
    Vertical,
}

impl TranslationAxis {
    pub fn constrain(self, start: ViewportPoint, point: ViewportPoint) -> ViewportPoint {
        match self {
            Self::Horizontal => ViewportPoint {
                y: start.y,
                ..point
            },
            Self::Vertical => ViewportPoint {
                x: start.x,
                ..point
            },
        }
    }
}

impl TransformConstraints {
    pub fn new(shift: bool, alt: bool) -> Self {
        Self { shift, alt }
    }

    /// Keep the chosen axis while Shift stays held; releasing Shift retires it.
    pub fn translation_axis(
        self,
        start: ViewportPoint,
        point: ViewportPoint,
    ) -> Option<TranslationAxis> {
        self.shift.then(|| {
            if (point.x - start.x).abs() >= (point.y - start.y).abs() {
                TranslationAxis::Horizontal
            } else {
                TranslationAxis::Vertical
            }
        })
    }

    fn translation(self, delta: ModelVector) -> ModelVector {
        if self.shift && self.alt {
            ModelVector {
                x: delta.x.round(),
                y: delta.y.round(),
            }
        } else {
            delta
        }
    }

    fn angle(self, degrees: f32) -> f32 {
        if self.shift {
            let step = if self.alt { 1. } else { 15. };
            (degrees / step).round() * step
        } else {
            degrees
        }
    }

    fn scale(self, ratio: f32) -> f32 {
        if self.shift && self.alt {
            (1. + ((ratio - 1.) * 100.).round() / 100.).max(0.01)
        } else {
            ratio
        }
    }

    /// Apply once, after the pointer geometry has produced a transform. Pivots,
    /// parent-space conversion and shape scope remain owned by the gesture.
    pub fn apply(self, change: ControlChange) -> ControlChange {
        match change {
            ControlChange::Translate(delta) => ControlChange::Translate(self.translation(delta)),
            ControlChange::MoveAll(delta) => ControlChange::MoveAll(self.translation(delta)),
            ControlChange::Transform {
                scope,
                pivot,
                scale,
                radians,
                translation,
                axes,
            } => ControlChange::Transform {
                scope,
                pivot,
                scale: scale.map(|ratio| self.scale(ratio)),
                radians: if self.shift {
                    self.angle(radians.to_degrees()).to_radians()
                } else {
                    radians
                },
                translation: self.translation(translation),
                axes,
            },
            ControlChange::Rotation {
                translation,
                angle,
                scale,
            } => ControlChange::Rotation {
                translation: self.translation(translation),
                angle: self.angle(angle),
                scale: self.scale(scale),
            },
        }
    }
}

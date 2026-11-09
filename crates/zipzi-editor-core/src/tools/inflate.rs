//! Continuous radial deformation of Mesh vertices and Warp lattice points.
use super::{BrushSelectionSettings, gesture::Press, hints::*, region::brush_points};
use crate::control_edit::ControlEdit;
use crate::{ComponentRef, Document, FrameVersion, ModelPoint, ModelVector, ViewportPoint};
use anyhow::Result;
use std::{collections::HashMap, time::Duration};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BrushMode {
    #[default]
    Select,
    Deform,
    Inflate,
}

const STEP: Duration = Duration::from_nanos(8_333_333);
const MAX_ELAPSED: Duration = Duration::from_millis(100);

pub(super) struct InflateGesture {
    pub press: Press,
    settings: BrushSelectionSettings,
    candidates: Vec<ComponentRef>,
    targets: Option<Vec<ControlEdit>>,
    baseline: Vec<ModelPoint>,
    points: Vec<ViewportPoint>,
    elapsed: Duration,
    pub shrinking: bool,
    editing: bool,
}

impl InflateGesture {
    pub fn new(
        document: &Document,
        mut press: Press,
        settings: BrushSelectionSettings,
    ) -> Option<Self> {
        // Shift changes the flow direction. Alt never turns this into selection erasure.
        press.operation = super::region::SelectionOperation::Replace;
        let candidates: Vec<_> = brush_points(document, &press)
            .into_iter()
            .map(|(component, _)| component)
            .collect();
        if candidates.is_empty() || settings.weight == 0. {
            return None;
        }
        Some(Self {
            shrinking: press.shift,
            press,
            settings,
            candidates,
            targets: None,
            baseline: Vec::new(),
            points: Vec::new(),
            elapsed: Duration::ZERO,
            editing: false,
        })
    }

    pub fn set_modifiers(&mut self, shift: bool) -> bool {
        let changed = self.shrinking != shift;
        self.shrinking = shift;
        changed
    }

    fn prepare(&mut self, document: &Document) -> Result<()> {
        if self.targets.is_some() {
            return Ok(());
        }
        let mut objects = Vec::new();
        for component in &self.candidates {
            if !objects.contains(&component.object) {
                objects.push(component.object.clone());
            }
        }
        let targets = ControlEdit::prepare_targets(
            document,
            &self.press.scene,
            &objects,
            self.press.original_points,
        )?;
        let mut positions = HashMap::new();
        for target in &targets {
            for (slot, &id) in target.ids.iter().enumerate() {
                let point = if let Some(mesh) = &target.mesh {
                    if mesh.original_points {
                        target.to_model(mesh.local[slot])
                    } else {
                        mesh.displayed[slot]
                    }
                } else {
                    target.to_model(target.positions[slot])
                };
                positions.insert(
                    ComponentRef {
                        object: target.object.clone(),
                        id,
                    },
                    point,
                );
            }
        }
        for component in &self.candidates {
            let point = positions[component];
            self.baseline.push(point);
            self.points.push(self.press.transform.to_viewport(point));
        }
        self.targets = Some(targets);
        Ok(())
    }

    pub fn tick(&mut self, document: &mut Document, elapsed: Duration) -> Result<bool> {
        self.elapsed += elapsed.min(MAX_ELAPSED);
        if self.elapsed < STEP {
            return Ok(false);
        }
        self.prepare(document)?;
        let previous = self.points.clone();
        let center = self.press.current;
        while self.elapsed >= STEP {
            self.elapsed -= STEP;
            for point in &mut self.points {
                let distance = self.settings.normalized_distance(*point, center);
                let influence = self.settings.influence(*point, center, center);
                // Taper velocity at the footprint boundary even at 100% hardness.
                // This keeps long holds finite, with no crossing of the center or boundary.
                let amount = 2. * STEP.as_secs_f32() * influence * (1. - distance).max(0.);
                let amount = if self.shrinking { -amount } else { amount };
                point.x += (point.x - center.x) * amount;
                point.y += (point.y - center.y) * amount;
            }
        }
        if self.points == previous {
            return Ok(false);
        }
        let displacement: HashMap<_, _> = self
            .candidates
            .iter()
            .zip(&self.baseline)
            .zip(&self.points)
            .map(|((component, before), point)| {
                // Transform the displacement, avoiding a rounded inverse position
                // that could nudge untouched points outside the brush footprint.
                let origin = self.press.transform.to_viewport(*before);
                (
                    component.clone(),
                    ModelVector {
                        x: (point.x - origin.x) / self.press.transform.scale,
                        y: (origin.y - point.y) / self.press.transform.scale,
                    },
                )
            })
            .collect();
        let targets = self.targets.as_ref().unwrap();
        if !self.editing {
            document.begin_control_edit(targets)?;
            self.editing = true;
        }
        if let Err(error) = document.preview_brush_edit(targets, &displacement) {
            self.points = previous;
            return Err(error);
        }
        Ok(true)
    }

    pub fn accepts_frame(&self, version: FrameVersion) -> bool {
        let origin = self.press.version;
        self.editing
            && version.binding.document == origin.binding.document
            && version.scene > origin.scene
            && (version.binding.viewport != origin.binding.viewport
                || version.viewport == origin.viewport && version.request > origin.request)
    }
    pub fn finish(&mut self, document: &mut Document) -> Result<()> {
        if self.editing {
            document.finish_edit()?;
        }
        self.editing = false;
        Ok(())
    }
    pub fn cancel(&self, document: &mut Document) -> bool {
        if !self.editing {
            return false;
        }
        document.cancel_edit();
        true
    }
}

pub(super) const HINTS: &[InteractionHint] = &[
    InteractionHint::Drag(HintAction::Inflate),
    InteractionHint::Shift(HintAction::Contract),
    InteractionHint::Primary(HintAction::Transform),
    InteractionHint::Command(crate::CommandId::CancelGesture),
];

impl super::ToolController {
    pub(crate) fn brush_inflating(&self) -> bool {
        matches!(self.gesture, Some(super::gesture::Gesture::Inflate(_)))
    }
}

//! Physics configuration edits and session-only audition controls.
mod cursor;
pub(crate) use cursor::CursorPhysicsPreview;
pub use cursor::{CursorPreviewAxis, CursorPreviewBinding, CursorPreviewBindings};

use crate::*;
use anyhow::{Result, ensure};
use zipzi_core::document::PhysicsAsset;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PhysicsPreviewSettings {
    pub manual: bool,
    pub bypass: bool,
    pub inputs: Vec<PhysicsPreviewInput>,
    pub baseline: std::collections::BTreeMap<String, f32>,
    pub stabilized: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsPreviewInput {
    pub parameter: String,
    pub time: f32,
    pub value: f32,
}

impl Editor {
    pub fn set_physics_manual(&mut self, document: DocumentId, manual: bool) -> Result<()> {
        self.ensure_animation_target(document)?;
        if self.animation().unwrap().physics_preview.manual == manual {
            if self.cursor_physics_active() {
                self.cancel_cursor_preview_drag();
                self.active_session_mut()
                    .unwrap()
                    .animation
                    .playback
                    .cursor_physics = None;
                self.reset_physics_inputs(document)?;
            }
            return Ok(());
        }
        self.cancel_cursor_preview_drag();
        self.cancel_timeline_edit();
        self.active_session_mut()
            .unwrap()
            .animation
            .playback
            .set_manual_physics(manual);
        Ok(())
    }
    pub fn set_physics_bypass(&mut self, document: DocumentId, bypass: bool) -> Result<()> {
        self.ensure_animation_target(document)?;
        let animation = &mut self.active_session_mut().unwrap().animation.playback;
        if animation.physics_preview.bypass != bypass {
            animation.physics_preview.bypass = bypass;
            animation.rebuild();
        }
        Ok(())
    }
    pub fn set_physics_input(
        &mut self,
        document: DocumentId,
        parameter: &str,
        value: f32,
    ) -> Result<()> {
        self.ensure_animation_target(document)?;
        ensure!(value.is_finite(), "Invalid physics input");
        let model = self.document().unwrap();
        let p = model.parameter_draft(parameter)?;
        let animation = &mut self.active_session_mut().unwrap().animation.playback;
        ensure!(
            animation.physics_preview.manual,
            "Manual physics preview is inactive"
        );
        let value = value.clamp(p.minimum, p.maximum);
        if !animation
            .physics_preview
            .push_input(parameter, animation.time, value, p.default_value)
        {
            return Ok(());
        }
        // Treat running input changes as a live stream: keep in-flight samples
        // alive so continuous dragging cannot repeatedly cancel evaluation.
        if animation.playing {
            animation.physics_input_changed();
        } else {
            animation.rebuild();
        }
        Ok(())
    }
    pub fn reset_physics_inputs(&mut self, document: DocumentId) -> Result<()> {
        self.ensure_animation_target(document)?;
        self.cancel_cursor_preview_drag();
        let animation = &mut self.active_session_mut().unwrap().animation.playback;
        ensure!(
            animation.physics_preview.manual,
            "Manual physics preview is inactive"
        );
        animation.physics_preview.inputs.clear();
        animation.physics_preview.baseline.clear();
        animation.physics_preview.stabilized = false;
        if let Some(cursor) = &mut animation.cursor_physics {
            cursor.reset();
        }
        animation.time = 0.;
        animation.rebuild();
        Ok(())
    }
    pub fn stabilize_physics_preview(&mut self, document: DocumentId) -> Result<()> {
        self.ensure_animation_target(document)?;
        let animation = &mut self.active_session_mut().unwrap().animation.playback;
        ensure!(
            animation.physics_preview.manual,
            "Manual physics preview is inactive"
        );
        for input in animation.physics_preview.inputs.drain(..) {
            animation
                .physics_preview
                .baseline
                .insert(input.parameter, input.value);
        }
        animation.physics_preview.stabilized = true;
        animation.time = 0.;
        animation.rebuild();
        Ok(())
    }
}

impl AnimationSession {
    pub(crate) fn set_manual_physics(&mut self, manual: bool) {
        self.cursor_physics = None;
        if manual {
            self.physics_return = Some((self.time, self.edit_time));
            self.time = 0.;
        } else if let Some((time, edit)) = self.physics_return.take() {
            self.time = time;
            self.edit_time = edit;
        }
        self.physics_preview.inputs.clear();
        self.physics_preview.baseline.clear();
        self.physics_preview.stabilized = false;
        self.physics_preview.manual = manual;
        self.rebuild();
        self.playing = manual;
    }
}

impl PhysicsPreviewSettings {
    fn push_input(&mut self, parameter: &str, time: f32, value: f32, default: f32) -> bool {
        let current = self
            .inputs
            .iter()
            .rev()
            .find(|input| input.parameter == parameter)
            .map(|input| input.value)
            .or_else(|| self.baseline.get(parameter).copied())
            .unwrap_or(default);
        if current == value {
            return false;
        }
        // Append even at the same clock time so the evaluator can keep its history.
        self.inputs.push(PhysicsPreviewInput {
            parameter: parameter.into(),
            time,
            value,
        });
        true
    }
}

/// Keep only referenced bindings and derive wire counts after structural edits.
pub fn reconcile_physics_asset(asset: &mut PhysicsAsset) {
    asset.reconcile_counts();
}

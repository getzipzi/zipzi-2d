//! Physics configuration edits and session-only audition controls.
use crate::*;
use anyhow::{Result, ensure};
use kasane_core::document::PhysicsAsset;

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
            return Ok(());
        }
        self.cancel_timeline_edit();
        let animation = &mut self.active_session_mut().unwrap().animation.playback;
        if manual {
            animation.physics_return = Some((animation.time, animation.edit_time));
            animation.time = 0.;
            animation.physics_preview.inputs.clear();
            animation.physics_preview.baseline.clear();
            animation.physics_preview.stabilized = false;
        } else if let Some((time, edit)) = animation.physics_return.take() {
            animation.time = time;
            animation.edit_time = edit;
        }
        if !manual {
            animation.physics_preview.inputs.clear();
            animation.physics_preview.baseline.clear();
            animation.physics_preview.stabilized = false;
        }
        animation.physics_preview.manual = manual;
        animation.rebuild();
        animation.playing = manual;
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
        let time = animation.time;
        animation
            .physics_preview
            .inputs
            .retain(|input| input.time != time || input.parameter != parameter);
        animation.physics_preview.inputs.push(PhysicsPreviewInput {
            parameter: parameter.into(),
            time,
            value: value.clamp(p.minimum, p.maximum),
        });
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
        let animation = &mut self.active_session_mut().unwrap().animation.playback;
        ensure!(
            animation.physics_preview.manual,
            "Manual physics preview is inactive"
        );
        animation.physics_preview.inputs.clear();
        animation.physics_preview.baseline.clear();
        animation.physics_preview.stabilized = false;
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

/// Keep only referenced bindings and derive wire counts after structural edits.
pub fn reconcile_physics_asset(asset: &mut PhysicsAsset) {
    let referenced: std::collections::HashSet<_> = asset
        .referenced_runtime_ids()
        .into_iter()
        .map(str::to_owned)
        .collect();
    asset
        .parameter_bindings
        .retain(|id, _| referenced.contains(id));
    let (settings, inputs, outputs, vertices) = asset.data.actual_counts();
    asset.data.meta.setting_count = settings;
    asset.data.meta.input_count = inputs;
    asset.data.meta.output_count = outputs;
    asset.data.meta.vertex_count = vertices;
}

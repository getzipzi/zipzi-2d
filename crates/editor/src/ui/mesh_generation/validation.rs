//! Form parsing and checks against the current authoring target.
use super::*;

impl MeshGenerationForm {
    pub(super) fn validate(&self, cx: &App) -> anyhow::Result<MeshGenerationOptions> {
        let owner = self
            .owner
            .upgrade()
            .ok_or_else(|| anyhow::anyhow!(t!("mesh_generation.stale")))?;
        let state = owner.read(cx);
        anyhow::ensure!(
            state.editor.active_id() == Some(self.document)
                && !state.modal
                && !state.editor.is_busy()
                && !state.editor.exiting(),
            t!("mesh_generation.stale")
        );
        let doc = state.editor.document().unwrap();
        anyhow::ensure!(
            doc.mesh_generation_revision() == self.revision && !doc.has_pending_edit(),
            t!("mesh_generation.stale")
        );
        anyhow::ensure!(
            !doc.object_state(&self.target).locked(),
            t!("mesh_generation.locked")
        );
        if self.is_alpha(cx) {
            anyhow::ensure!(
                self.preview.source.alpha_error().is_none(),
                "{}",
                self.preview.source.alpha_error().unwrap_or_default()
            );
            let values: Vec<_> = self
                .alpha_inputs
                .iter()
                .map(|input| input.read(cx).value())
                .collect();
            let invalid = || anyhow::anyhow!(t!("mesh_generation.invalid_alpha"));
            let number = |i: usize| truncated_number(&values[i]).ok_or_else(invalid);
            let options = AlphaMeshOptions {
                outside_spacing: number(0)?,
                inside_spacing: number(1)?,
                outside_margin: number(2)?,
                inside_margin: number(3)?,
                minimum_margin: number(4)?,
                minimum_boundary_points: number(5)?.to_string().parse().map_err(|_| invalid())?,
                alpha_threshold: number(6)?.to_string().parse().map_err(|_| invalid())?,
                preserve_holes: self.preserve_holes,
                clip_to_image: false,
                remove_faint_islands: self.remove_faint_islands,
                ..AlphaMeshOptions::default()
            };
            options.validate().map_err(|_| invalid())?;
            return Ok(MeshGenerationOptions::Alpha(options));
        }
        let parse = |i: usize| {
            truncated_number(&self.inputs[i].read(cx).value())
                .and_then(|value| value.to_string().parse::<usize>().ok())
                .ok_or_else(|| anyhow::anyhow!(t!("mesh_generation.invalid_counts")))
        };
        let options = MeshGridOptions {
            columns: parse(0)?,
            rows: parse(1)?,
        };
        options
            .validate()
            .map_err(|_| anyhow::anyhow!(t!("mesh_generation.invalid_counts")))?;
        anyhow::ensure!(
            !self.square || options.columns == options.rows,
            t!("mesh_generation.square_required")
        );
        doc.validate_mesh_grid(&self.target, options)?;
        Ok(MeshGenerationOptions::Grid(options))
    }
}

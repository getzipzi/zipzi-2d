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
            state.editor.editing_context_token_valid(self.context)
                && !state.modal
                && !state.editor.is_busy()
                && !state.editor.exiting(),
            t!("mesh_generation.stale")
        );
        let doc = state.editor.document().unwrap();
        anyhow::ensure!(
            self.preview.source.is_current(doc) && doc.mesh_generation_revision() == self.revision,
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
            let number = |i: usize| parsed_number(&values[i]).ok_or_else(invalid);
            let threshold = number(5)?;
            anyhow::ensure!(
                threshold.fract() == 0. && (1. ..=255.).contains(&threshold),
                invalid()
            );
            let mut fill_parameters = self.fill_parameters;
            *fill_parameters.get_mut(self.fill_kind(cx)) =
                self.read_fill_parameters(cx).ok_or_else(invalid)?;
            let options = MeshSettings {
                edge_mode: self.edge_kind(cx),
                edge_spacing: number(0)?,
                interior_spacing: number(1)?,
                outer_margin: number(2)?,
                edge_width: number(3)?,
                wrap: number(4)?,
                alpha_threshold: threshold as u8,
                fill_algorithm: self.fill_kind(cx),
                fill_parameters,
                suppress_boundary_diagonals: self.suppress_boundary_diagonals,
                clip_to_image: self.preview.source.manual_available(),
                ..MeshSettings::default()
            };
            options.validate().map_err(|_| invalid())?;
            return Ok(self.wrap_alpha(options, cx));
        }
        if self.is_reconnect(cx) {
            return Ok(MeshGenerationOptions::ManualReconnect);
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

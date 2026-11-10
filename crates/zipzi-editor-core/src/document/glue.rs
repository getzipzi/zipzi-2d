use super::*;
use zipzi_core::VertexId;

impl Document {
    /// Publish topology and its binding together so a click has one undo step.
    pub(crate) fn stitch_glue_vertices(
        &mut self,
        target: Option<&str>,
        sides: [&crate::ManualMeshTarget; 2],
        pair: (VertexId, VertexId),
        operations: Vec<(
            crate::ManualMeshTarget,
            zipzi_contract::authoring::ManualMeshOperation,
        )>,
    ) -> Result<String> {
        for side in sides {
            self.validate_manual_target(side)?;
        }
        self.finish_edit()?;
        let zipzi_sdk::QuerySource::Published(version) = sides[0].source else {
            anyhow::bail!("Mesh Edit requires a published source");
        };
        let mut draft = self
            .session
            .begin_draft("Stitch seam", Some(version))?
            .interactive()?;
        if !operations.is_empty() {
            draft.apply_manual_mesh_batch(
                operations
                    .into_iter()
                    .map(|(t, op)| (t.object.id, op))
                    .collect(),
            )?;
        }
        let id = draft.apply(|commands| {
            Ok(commands.bind_glue_vertices(
                target,
                &sides[0].object.id,
                &sides[1].object.id,
                &[pair],
            )?)
        })?;
        let receipt = draft.commit_to(&mut self.session)?;
        self.consume_authoring_receipt(&receipt, None);
        Ok(id)
    }

    pub(crate) fn preview_glue_weights(
        &mut self,
        version: zipzi_sdk::Version,
        id: &str,
        weights: &[(&str, [f32; 2])],
    ) -> Result<()> {
        ensure!(
            self.session.version() == version,
            "Glue changed during the stroke"
        );
        if self.pending.is_none() {
            let glue = self
                .model()
                .glue(id)
                .cloned()
                .ok_or_else(|| anyhow!("Glue no longer exists"))?;
            self.pending = Some(PendingEdit {
                kind: Gesture::GlueWeights,
                original: PendingOriginal::GlueWeights(glue),
                edit: self
                    .session
                    .begin_draft("Paint Glue weights", Some(version))?
                    .interactive()?,
                preview: PreviewState::default(),
                changed: OnceLock::new(),
                topology_changed: false,
                draw_groups_changed: false,
                property_write: None,
            });
        }
        let pending = self.pending.as_mut().unwrap();
        ensure!(
            pending.kind == Gesture::GlueWeights && pending.original.id() == id,
            "Glue stroke lost its edit"
        );
        pending
            .edit
            .apply(|commands| Ok(commands.set_glue_weights(id, weights)?))?;
        pending.changed.take();
        self.invalidate_projection(false);
        Ok(())
    }

    pub(crate) fn cancel_glue_weights(&mut self) -> bool {
        if self.has_glue_weight_edit() {
            self.cancel_edit();
            true
        } else {
            false
        }
    }

    pub(crate) fn has_glue_weight_edit(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.kind == Gesture::GlueWeights)
    }

    pub(crate) fn bind_glue_vertices(
        &mut self,
        target: Option<&str>,
        a: &str,
        b: &str,
        pairs: &[(VertexId, VertexId)],
        version: zipzi_sdk::Version,
    ) -> Result<String> {
        self.finish_edit()?;
        let (id, receipt) = self
            .session
            .edit("Bind Glue vertices", Some(version), |commands| {
                commands.bind_glue_vertices(target, a, b, pairs)
            })?;
        self.consume_authoring_receipt(&receipt, None);
        Ok(id)
    }

    pub(crate) fn unbind_glue_vertices(
        &mut self,
        target: &str,
        pairs: &[String],
        version: zipzi_sdk::Version,
    ) -> Result<bool> {
        self.finish_edit()?;
        let (_, receipt) =
            self.session
                .edit("Unbind Glue vertices", Some(version), |commands| {
                    commands.unbind_glue_vertices(target, pairs)
                })?;
        self.consume_authoring_receipt(&receipt, None);
        Ok(receipt.kind != zipzi_sdk::PublicationKind::Unchanged)
    }
}

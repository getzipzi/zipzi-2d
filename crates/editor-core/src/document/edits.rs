//! Candidate edits, commit/cancel and history reconciliation.
use super::*;

impl Document {
    fn begin_gesture(&mut self, kind: Gesture) -> Result<()> {
        self.ensure_editable(&self.context.selections)?;
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.kind == kind)
        {
            return Ok(());
        }
        self.finish_edit()?;
        let original = self
            .selected
            .as_deref()
            .and_then(|id| self.session.mesh(id))
            .ok_or_else(|| anyhow!("Select a layer first"))?;
        let label = match kind {
            Gesture::Rename => "Rename layer",
            Gesture::Opacity => "Change opacity",
            Gesture::Controls => "Edit canvas controls",
            Gesture::Properties(_) | Gesture::ExpressionEntry(_) => unreachable!(),
        };
        self.pending = Some(PendingEdit {
            kind,
            original: PendingOriginal::Controls(crate::control_edit::EditOriginal::Mesh(original)),
            edit: self.session.begin_owned_edit(label, None)?,
            preview: PreviewState::default(),
            changed: OnceLock::new(),
            topology_changed: false,
            draw_groups_changed: false,
            property_write: None,
        });
        Ok(())
    }

    pub(crate) fn begin_control_edit(
        &mut self,
        targets: &[crate::control_edit::ControlEdit],
    ) -> Result<()> {
        self.ensure_editable(&self.context.selections)?;
        self.ensure_editable(&targets.iter().map(|t| t.object.clone()).collect::<Vec<_>>())?;
        self.finish_edit()?;
        self.pending = Some(PendingEdit {
            kind: Gesture::Controls,
            original: PendingOriginal::Controls(crate::control_edit::EditOriginal::Batch(
                targets.iter().map(|t| t.original.clone()).collect(),
            )),
            edit: self
                .session
                .begin_owned_edit("Edit canvas controls", None)?,
            preview: PreviewState::default(),
            changed: OnceLock::new(),
            topology_changed: false,
            draw_groups_changed: false,
            property_write: None,
        });
        Ok(())
    }
    pub(crate) fn preview_control_edit(
        &mut self,
        targets: &[crate::control_edit::ControlEdit],
        selected: &[crate::ComponentRef],
        change: crate::control_edit::ControlChange,
    ) -> Result<()> {
        let pending = self
            .pending
            .as_ref()
            .ok_or_else(|| anyhow!("No canvas edit"))?;
        ensure!(pending.kind == Gesture::Controls, "Another edit is active");
        let previous_revision = self.projection_revision;
        self.invalidate_projection(false);
        let pending = self.pending.as_mut().unwrap();
        pending.changed.take();
        for target in targets {
            let ids = selected
                .iter()
                .filter(|c| c.object == target.object)
                .map(|c| c.id)
                .collect::<Vec<_>>();
            let shape = || {
                pending
                    .edit
                    .candidate_document()
                    .get_transform(&target.object.id)
                    .and_then(|t| t.warp())
                    .map(|w| (w.rows, w.columns))
            };
            let before = shape();
            target.preview(&mut pending.edit, &ids, change)?;
            let after = pending
                .edit
                .candidate_document()
                .get_transform(&target.object.id)
                .and_then(|t| t.warp())
                .map(|w| (w.rows, w.columns));
            if before != after {
                pending.topology_changed = true;
                self.canvas_topology.get_mut().unwrap().take();
            }
        }
        if let Some((revision, index)) = self.object_index.get_mut().unwrap() {
            if *revision == previous_revision {
                for target in targets {
                    if target.object.kind == crate::ObjectKind::Deformer {
                        Arc::make_mut(index)
                            .refresh_deformer(pending.edit.candidate_document(), &target.object);
                    }
                }
                // Mesh positions and keyform poses are not index properties;
                // axes, keys, topology IDs and relation edges remain unchanged.
                *revision = self.projection_revision;
            }
        }
        Ok(())
    }
    pub fn preview_name(&mut self, name: &str) -> Result<()> {
        self.begin_gesture(Gesture::Rename)?;
        self.invalidate_projection(true);
        let pending = self.pending.as_mut().unwrap();
        pending.changed.take();
        let result = pending.edit.rename_mesh(pending.original.id(), name);
        if result.is_err() {
            self.pending = None;
        }
        Ok(result?)
    }

    pub fn preview_opacity(&mut self, opacity: f32) -> Result<()> {
        ensure!(
            opacity.is_finite() && (0.0..=1.0).contains(&opacity),
            "Opacity must be between 0 and 1"
        );
        self.begin_gesture(Gesture::Opacity)?;
        self.invalidate_projection(false);
        let pending = self.pending.as_mut().unwrap();
        pending.changed.take();
        let result = pending
            .edit
            .set_mesh_opacity(pending.original.id(), opacity);
        if result.is_err() {
            self.pending = None;
        }
        Ok(result?)
    }

    pub(super) fn pending_changed(&self) -> bool {
        self.pending.as_ref().is_some_and(|pending| {
            *pending.changed.get_or_init(|| {
                !pending
                    .original
                    .unchanged(pending.edit.candidate_document())
            })
        })
    }

    pub fn finish_edit(&mut self) -> Result<()> {
        if let Some(token) = self.expression_edit_token() {
            self.commit_expression_entry_edit(token)?;
            return Ok(());
        }
        let changed = self.pending_changed();
        if let Some(pending) = self.pending.take() {
            let previous_revision = self.projection_revision;
            let property = match &pending.original {
                PendingOriginal::Properties(object, _, _) => Some(object.clone()),
                _ => None,
            };
            self.invalidate_projection(
                matches!(pending.kind, Gesture::Rename) || pending.topology_changed,
            );
            if changed {
                let parameter = matches!(&pending.original, PendingOriginal::Properties(object, _, _) if object.kind == crate::ObjectKind::Parameter);
                pending.edit.commit_to(&mut self.session)?;
                if parameter {
                    self.reconcile_parameter_preview()?;
                }
                self.session.drain_events();
                self.pristine_blank = false;
            }
            if let Some(object) = property {
                self.refresh_property_index(
                    previous_revision,
                    &object,
                    pending.draw_groups_changed,
                );
            }
        }
        Ok(())
    }

    pub fn cancel_edit(&mut self) {
        if let Some(pending) = self.pending.take() {
            if let PendingOriginal::ExpressionEntry(address) = &pending.original {
                let references_changed = pending
                    .edit
                    .candidate_document()
                    .get_expression_entry(address.entry_id())
                    .unwrap()
                    .1
                    .target
                    != address.expected().target;
                let previous_revision = self.projection_revision;
                self.invalidate_expression_projection();
                if !references_changed {
                    self.refresh_expression_index(
                        previous_revision,
                        &[address.owner_id().to_owned()],
                    );
                }
                return;
            }
            let previous_revision = self.projection_revision;
            let property = match &pending.original {
                PendingOriginal::Properties(object, _, _) => Some(object.clone()),
                _ => None,
            };
            self.invalidate_projection(
                matches!(pending.kind, Gesture::Rename) || pending.topology_changed,
            );
            if let Some(object) = property {
                self.refresh_property_index(
                    previous_revision,
                    &object,
                    pending.draw_groups_changed,
                );
            }
        }
    }

    pub fn undo(&mut self) -> Result<()> {
        self.apply_history(AuthoringSession::undo)
    }

    pub fn redo(&mut self) -> Result<()> {
        self.apply_history(AuthoringSession::redo)
    }

    fn apply_history(
        &mut self,
        change: fn(&mut AuthoringSession) -> Result<EditReceipt, SdkError>,
    ) -> Result<()> {
        self.finish_edit()?;
        let before = self.evaluation_revision();
        let state = self.model().editor_state().clone();
        let receipt = change(&mut self.session)?;
        self.refresh_textures()?;
        self.reconcile_parameter_preview()?;
        self.consume_authoring_receipt(
            &receipt,
            before != self.evaluation_revision() || state == *self.model().editor_state(),
        );
        Ok(())
    }

    pub(super) fn after_history(&mut self, topology_changed: bool) {
        self.invalidate_projection(topology_changed);
        self.session.drain_events();
        let index = self.object_index();
        self.context.reconcile(&index);
        self.selected = self
            .context
            .selection
            .as_ref()
            .filter(|r| r.kind == crate::ObjectKind::Mesh)
            .map(|r| r.id.clone());
    }
}

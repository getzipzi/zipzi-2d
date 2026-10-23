//! Candidate edits, commit/cancel and history reconciliation.
use super::*;

impl PendingEdit {
    fn property_index_target(&self) -> Option<crate::ObjectRef> {
        match &self.original {
            PendingOriginal::Properties(object, _, _) => Some(object.clone()),
            PendingOriginal::Controls(crate::control_edit::EditOriginal::Mesh(mesh))
                if self.kind == Gesture::Opacity =>
            {
                Some(crate::ObjectRef::new(crate::ObjectKind::Mesh, &mesh.id))
            }
            _ => None,
        }
    }
}

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
        let previous_revision = self.projection_revision;
        self.invalidate_projection(false);
        let pending = self.pending.as_mut().unwrap();
        let object = crate::ObjectRef::new(crate::ObjectKind::Mesh, pending.original.id());
        pending.changed.take();
        let result = pending
            .edit
            .set_mesh_opacity(pending.original.id(), opacity);
        if result.is_err() {
            self.pending = None;
        }
        // Read the accepted candidate, or the committed model after a failure.
        // Publishing the new cache revision keeps inspector title reads local.
        self.refresh_property_index(previous_revision, &object, false);
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
            let property = pending.property_index_target();
            if changed
                && property
                    .as_ref()
                    .is_some_and(|p| p.kind == crate::ObjectKind::PhysicsGroup)
            {
                let receipt = match pending.edit.commit_to(&mut self.session) {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        if let Some(object) = &property {
                            self.refresh_property_index(
                                previous_revision,
                                object,
                                pending.draw_groups_changed,
                            );
                        }
                        return Err(error.into());
                    }
                };
                if receipt.changed {
                    self.pristine_blank = false;
                }
                self.consume_authoring_receipt(&receipt, false);
                return Ok(());
            }
            self.invalidate_projection(
                matches!(pending.kind, Gesture::Rename) || pending.topology_changed,
            );
            if changed {
                let parameter = matches!(&pending.original, PendingOriginal::Properties(object, _, _) if object.kind == crate::ObjectKind::Parameter);
                let receipt = match pending.edit.commit_to(&mut self.session) {
                    Ok(receipt) => receipt,
                    Err(error) => {
                        if let Some(object) = &property {
                            self.refresh_property_index(
                                previous_revision,
                                object,
                                pending.draw_groups_changed,
                            );
                        }
                        return Err(error.into());
                    }
                };
                if property
                    .as_ref()
                    .is_none_or(|p| p.kind != crate::ObjectKind::Motion)
                    || receipt
                        .changes
                        .categories
                        .contains(&kasane_sdk::ChangeCategory::EvaluationInputs)
                {
                    self.animation_input_revision = receipt.after.revision;
                }
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
            let property = pending.property_index_target();
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
        // Visibility affects Editor preview even though it is not a runtime
        // evaluation input. Lock-only history keeps the current playback source.
        if !state
            .hidden_objects()
            .eq(self.model().editor_state().hidden_objects())
        {
            self.animation_input_revision = self.session.version().revision;
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ObjectKind, ObjectRef};
    use kasane_core::Vec2;
    use std::sync::atomic::Ordering;

    #[test]
    fn opacity_gestures_refresh_indexes_and_restore_committed_values_on_every_exit() -> Result<()> {
        for outcome in ["cancel", "commit", "failed_commit", "unchanged"] {
            let mut doc = Document::new_blank()?;
            let mesh = ObjectRef::new(ObjectKind::Mesh, uuid::Uuid::new_v4().to_string());
            let peer = ObjectRef::new(ObjectKind::Mesh, uuid::Uuid::new_v4().to_string());
            doc.session.edit("Fixture", None, |edit| {
                for object in [&mesh, &peer] {
                    edit.create_mesh(kasane_core::Mesh {
                        id: object.id.clone(),
                        runtime_id: object.id.clone(),
                        name: "Mesh".into(),
                        vertex_ids: vec![0, 1, 2],
                        base_positions: vec![
                            Vec2::new(0., 0.),
                            Vec2::new(1., 0.),
                            Vec2::new(0., 1.),
                        ],
                        uvs: vec![Vec2::default(); 3],
                        triangles: vec![[0, 1, 2]],
                        ..Default::default()
                    })?;
                }
                Ok(())
            })?;
            doc.invalidate_projection(true);
            doc.select(&mesh.id)?;
            let original = doc.object_index();
            let original_payload = original.get(&mesh).unwrap().properties.clone();
            let topology = doc.canvas_topology();
            let builds = doc.object_index_builds.load(Ordering::Relaxed);
            let history = doc.history_lengths();
            for sample in 1..=12 {
                let opacity = sample as f32 / 16.;
                doc.preview_opacity(opacity)?;
                let index = doc.object_index();
                assert_eq!(doc.object_index_builds.load(Ordering::Relaxed), builds);
                assert_eq!(
                    index.get(&mesh).unwrap().properties["appearance"]["opacity"],
                    serde_json::json!(opacity)
                );
                assert_eq!(original.get(&mesh).unwrap().properties, original_payload);
                assert!(std::ptr::eq(
                    original.get(&peer).unwrap(),
                    index.get(&peer).unwrap()
                ));
                assert_eq!(original.bindings.as_ptr(), index.bindings.as_ptr());
                for (a, b) in original.relations.iter().zip(&index.relations) {
                    assert!(std::ptr::eq(a, b));
                }
                assert!(Arc::ptr_eq(&topology, &doc.canvas_topology()));
                assert_eq!(
                    index.get(&mesh).unwrap().properties,
                    crate::ObjectIndex::build(doc.model())
                        .get(&mesh)
                        .unwrap()
                        .properties
                );
            }
            let candidate = doc.object_index();
            let candidate_payload = candidate.get(&mesh).unwrap().properties.clone();
            let expected = match outcome {
                "cancel" => {
                    doc.cancel_edit();
                    1.
                }
                "failed_commit" => {
                    assert!(
                        doc.pending
                            .as_mut()
                            .unwrap()
                            .edit
                            .set_mesh_opacity(&mesh.id, f32::NAN)
                            .is_err()
                    );
                    assert!(doc.finish_edit().is_err());
                    1.
                }
                "unchanged" => {
                    doc.preview_opacity(1.)?;
                    doc.finish_edit()?;
                    1.
                }
                _ => {
                    doc.finish_edit()?;
                    0.75
                }
            };
            assert!(doc.pending.is_none());
            let restored = doc.object_index();
            assert_eq!(doc.object_index_builds.load(Ordering::Relaxed), builds);
            assert_eq!(
                restored.get(&mesh).unwrap().properties["appearance"]["opacity"],
                serde_json::json!(expected)
            );
            assert_eq!(
                restored.get(&mesh).unwrap().properties,
                crate::ObjectIndex::build(doc.model())
                    .get(&mesh)
                    .unwrap()
                    .properties
            );
            assert_eq!(original.get(&mesh).unwrap().properties, original_payload);
            assert_eq!(candidate.get(&mesh).unwrap().properties, candidate_payload);
            assert!(std::ptr::eq(
                original.get(&peer).unwrap(),
                restored.get(&peer).unwrap()
            ));
            assert_eq!(
                doc.history_lengths(),
                if outcome == "commit" {
                    (history.0 + 1, 0)
                } else {
                    history
                }
            );
        }
        Ok(())
    }
}

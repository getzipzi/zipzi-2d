//! Expression-owned edits install fragments into the same tracked candidate.
use super::*;

impl DocumentState {
    pub fn expression_projection(&self, id: &str) -> Result<ExpressionProjection, Status> {
        self.content.model().expression_projection(id)
    }

    pub fn expression_entry(&self, id: &str) -> Option<(&ExpressionAsset, &ExpressionEntry)> {
        self.content
            .expressions
            .shared_map()
            .values()
            .find_map(|asset| {
                asset
                    .entries
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| (asset.as_ref(), e))
            })
    }
    pub fn references_to(&self, id: &str) -> Vec<String> {
        self.content.model().references_to(id)
    }
}
impl DocumentBuilder {
    pub fn expression_projection(&self, id: &str) -> Result<ExpressionProjection, Status> {
        self.content.model().expression_projection(id)
    }

    pub fn expression_entry(&self, id: &str) -> Option<(&ExpressionAsset, &ExpressionEntry)> {
        self.content
            .expressions
            .shared_map()
            .values()
            .find_map(|asset| {
                asset
                    .entries
                    .iter()
                    .find(|e| e.id == id)
                    .map(|e| (asset.as_ref(), e))
            })
    }
    pub fn references_to(&self, id: &str) -> Vec<String> {
        self.content.model().references_to(id)
    }

    fn put_expression(&mut self, asset: ExpressionAsset) {
        self.touched
            .entry(RecordDomain::Expression)
            .or_default()
            .insert(asset.id.clone());
        self.content.expressions.insert(asset.id.clone(), asset);
    }
    pub(super) fn clear_owned_editor_objects(&mut self, removed: &[String]) {
        if removed
            .iter()
            .any(|id| self.editor_state().objects.contains_key(id))
        {
            self.roots_touched.insert(RootDomain::EditorState);
            let state = Arc::make_mut(&mut self.content.editor_state);
            for id in removed {
                state.objects.remove(id);
            }
        }
    }
    pub fn create_expression(&mut self, asset: ExpressionAsset) -> Result<(), Status> {
        self.write(|this| {
            this.check_new_id(&asset.id)?;
            this.put_expression(asset);
            Ok(())
        })
    }
    pub fn replace_expression(&mut self, asset: ExpressionAsset) -> Result<(), Status> {
        self.write(|this| {
            let old = this
                .expression(&asset.id)
                .ok_or_else(|| Status::error("MISSING_EXPRESSION", &asset.id))?;
            let model = this.content.model();
            model.validate_expression_unlocked(&asset)?;
            let removed: Vec<_> = old
                .entries
                .iter()
                .filter(|e| !asset.entries.iter().any(|next| next.id == e.id))
                .map(|e| e.id.clone())
                .collect();
            this.clear_owned_editor_objects(&removed);
            this.put_expression(asset);
            Ok(())
        })
    }
    pub fn remove_expression(&mut self, id: &str) -> Result<(), Status> {
        self.edit_expression(ExpressionEdit::Delete { id: id.into() })
    }

    /// Plan against this exact working candidate and immediately install the
    /// affected records. Publication and query provenance belong to SDK Draft.
    pub fn edit_expression(&mut self, edit: ExpressionEdit) -> Result<(), Status> {
        self.write(|this| {
            let changes = this.content.model().expression_changes(edit)?;
            if let ExpressionEdit::PatchEntry { id, fields, .. } = changes.edit {
                let (asset, _) = this.expression_entry(&id).unwrap();
                let mut asset = asset.clone();
                let entry = asset.entries.iter_mut().find(|e| e.id == id).unwrap();
                entry.target = fields.target;
                entry.value = fields.value;
                entry.blend = fields.blend;
                this.put_expression(asset);
            } else {
                // Retain metadata of entries moving between owners; only truly
                // removed identities lose their EditorState.
                let mut removed: Vec<_> = changes
                    .affected_entries
                    .iter()
                    .filter(|id| {
                        !changes
                            .replacements
                            .iter()
                            .any(|a| a.entries.iter().any(|e| &e.id == *id))
                    })
                    .cloned()
                    .collect();
                removed.extend(changes.removed.iter().cloned());
                this.clear_owned_editor_objects(&removed);
                for id in changes.removed {
                    this.touched
                        .entry(RecordDomain::Expression)
                        .or_default()
                        .insert(id.clone());
                    this.content.expressions.remove(&id);
                }
                for asset in changes.replacements {
                    this.put_expression(asset);
                }
                if this.expression_order() != changes.order {
                    this.reorder(RecordDomain::Expression, changes.order)?;
                }
            }
            Ok(())
        })
    }
}

//! Domain operations for expression-owned entries. Order is semantic: duplicate
//! parameter targets are legal, and the first matching entry wins at runtime.
use super::*;
use std::sync::Arc;

#[derive(
    Debug,
    Clone,
    PartialEq,
    serde::Serialize,
    serde::Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct ExpressionEntryFields {
    /// Typed target reference; unresolved imported runtime labels remain explicit.
    pub target: ExpressionTarget,
    /// Channel value or replacement payload in the domain declared by this field type.
    pub value: f32,
    /// Optional expression blend mode; None uses the native Add default.
    pub blend: Option<ExpressionBlend>,
}
impl From<&ExpressionEntry> for ExpressionEntryFields {
    fn from(e: &ExpressionEntry) -> Self {
        Self {
            target: e.target.clone(),
            value: e.value,
            blend: e.blend,
        }
    }
}

/// All positions use stable entry IDs; None appends to the destination.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ExpressionEdit {
    Create {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
    },
    CreateEntry {
        /// Stable expression asset ID owning the selected entries.
        owner: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    PatchEntry {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Exact current field values required by the optimistic edit; stale values reject it.
        expected: ExpressionEntryFields,
        /// Typed replacement fields for the addressed object, applied atomically.
        fields: ExpressionEntryFields,
    },
    MoveEntries {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    DuplicateEntries {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// New stable identities paired with the selected source objects.
        new_ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    DeleteEntries {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    Duplicate {
        /// Typed parameter reference or stable source object ID, as declared by the enclosing operation.
        source: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
        /// New stable entry IDs paired with the source expression entries.
        entry_ids: Vec<String>,
    },
    Delete {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
    },
    Reorder {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct ExpressionPlan {
    snapshot_identity: Arc<()>,
    revision: u64,
    document_id: String,
    pub(super) edit: ExpressionEdit,
    replacements: Vec<ExpressionAsset>,
    removed: Vec<String>,
    order: Vec<String>,
    affected_expressions: Vec<String>,
    affected_entries: Vec<String>,
}

impl ExpressionPlan {
    pub fn affected_expressions(&self) -> &[String] {
        &self.affected_expressions
    }
    pub fn affected_entries(&self) -> &[String] {
        &self.affected_entries
    }
}

impl Document {
    /// Read-only planning shares validation and default construction with execution.
    pub fn plan_expression_edit(&self, edit: ExpressionEdit) -> Result<ExpressionPlan, Status> {
        if !self.initialized() {
            return Err(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        let mut plan = ExpressionPlan {
            snapshot_identity: self.snapshot_identity.clone(),
            document_id: self.id().into(),
            revision: self.revision(),
            edit: edit.clone(),
            replacements: vec![],
            removed: vec![],
            order: self.expression_order().to_vec(),
            affected_expressions: vec![],
            affected_entries: vec![],
        };
        let owner = |id: &str| {
            self.get_expression(id)
                .cloned()
                .ok_or_else(|| Status::error("MISSING_EXPRESSION", id))
        };
        let entries = |ids: &[String]| -> Result<Vec<(String, ExpressionEntry)>, Status> {
            let selected: HashSet<_> = ids.iter().collect();
            if ids.is_empty() || selected.len() != ids.len() {
                return Err(Status::error(
                    "INVALID_SELECTION",
                    "Select distinct entries",
                ));
            }
            let found: Vec<_> = self
                .expression_order()
                .iter()
                .flat_map(|id| &self.content.expressions[id].entries)
                .filter(|e| selected.contains(&e.id))
                .map(|e| {
                    (
                        self.get_expression_entry(&e.id).unwrap().0.id.clone(),
                        e.clone(),
                    )
                })
                .collect();
            if found.len() != ids.len() {
                return Err(Status::error(
                    "MISSING_EXPRESSION_ENTRY",
                    "Selected entry no longer exists",
                ));
            }
            Ok(found)
        };
        match &edit {
            ExpressionEdit::Create { id, name } => {
                if self.contains_expression_identity(id) {
                    return Err(Status::error("DUPLICATE_ID", id));
                }
                plan.order.push(id.clone());
                plan.replacements.push(ExpressionAsset {
                    id: id.clone(),
                    name: name.clone(),
                    file_type: Some("Live2D Expression".into()),
                    fade_in: None,
                    fade_out: None,
                    entries: vec![],
                    extensions: Default::default(),
                    opaque_source_ids: None,
                    opaque_source_content_hash: None,
                });
            }
            ExpressionEdit::CreateEntry {
                owner: id,
                id: entry_id,
                before,
            } => {
                if self.contains_expression_identity(entry_id) {
                    return Err(Status::error("DUPLICATE_ID", entry_id));
                }
                let mut asset = owner(id)?;
                let index = insertion_index(&asset, before)?;
                asset.entries.insert(
                    index,
                    ExpressionEntry {
                        id: entry_id.clone(),
                        target: ExpressionTarget::Unspecified,
                        value: 0.,
                        blend: None,
                        extensions: Default::default(),
                    },
                );
                plan.affected_entries.push(entry_id.clone());
                plan.replacements.push(asset);
            }
            ExpressionEdit::PatchEntry {
                id,
                expected,
                fields,
            } => {
                let (asset, entry) = self
                    .get_expression_entry(id)
                    .ok_or_else(|| Status::error("MISSING_EXPRESSION_ENTRY", id))?;
                if ExpressionEntryFields::from(entry) != *expected {
                    return Err(Status::error("STALE_FIELD", id));
                }
                self.validate_expression_entry_fields(fields)?;
                plan.affected_expressions.push(asset.id.clone());
                plan.affected_entries.push(id.clone());
                // Patches never clone/replace an owner's entry array.
            }
            ExpressionEdit::MoveEntries {
                ids,
                destination,
                before,
            }
            | ExpressionEdit::DuplicateEntries {
                ids,
                destination,
                before,
                ..
            } => {
                let selected = entries(ids)?;
                let copying = matches!(&edit, ExpressionEdit::DuplicateEntries { .. });
                let mut assets = BTreeMap::new();
                assets.insert(destination.clone(), owner(destination)?);
                if !copying {
                    for (id, _) in &selected {
                        assets.entry(id.clone()).or_insert(owner(id)?);
                    }
                    if before.as_ref().is_some_and(|id| ids.contains(id)) {
                        return Err(Status::error("INVALID_PLACEMENT", "Anchor is selected"));
                    }
                    for asset in assets.values_mut() {
                        asset.entries.retain(|e| !ids.contains(&e.id));
                    }
                }
                let dest = assets.get_mut(destination).unwrap();
                let index = insertion_index(dest, before)?;
                let mut moving: Vec<_> = selected.into_iter().map(|(_, e)| e).collect();
                if let ExpressionEdit::DuplicateEntries { new_ids, .. } = &edit {
                    if new_ids.len() != moving.len() {
                        return Err(Status::error(
                            "INVALID_COPY_IDS",
                            "One new ID per entry in document order is required",
                        ));
                    }
                    for (entry, id) in moving.iter_mut().zip(new_ids) {
                        if self.contains_expression_identity(id) {
                            return Err(Status::error("DUPLICATE_ID", id));
                        }
                        entry.id = id.clone();
                    }
                }
                plan.affected_entries = moving.iter().map(|e| e.id.clone()).collect();
                dest.entries.splice(index..index, moving);
                plan.replacements = assets.into_values().collect();
            }
            ExpressionEdit::DeleteEntries { ids } => {
                let selected = entries(ids)?;
                let mut assets = BTreeMap::new();
                for (id, _) in selected {
                    assets.entry(id.clone()).or_insert(owner(&id)?);
                }
                for asset in assets.values_mut() {
                    asset.entries.retain(|e| !ids.contains(&e.id));
                }
                plan.replacements = assets.into_values().collect();
                plan.affected_entries = ids.clone();
            }
            ExpressionEdit::Duplicate {
                source,
                id,
                name,
                entry_ids,
            } => {
                if self.contains_expression_identity(id) {
                    return Err(Status::error("DUPLICATE_ID", id));
                }
                let mut asset = owner(source)?;
                if asset.entries.len() != entry_ids.len() {
                    return Err(Status::error(
                        "INVALID_COPY_IDS",
                        "One new ID per owned entry is required",
                    ));
                }
                asset.id = id.clone();
                asset.name = name.clone();
                if let Some(mut scene) = asset.source_scene() {
                    scene.id = id.clone();
                    for track in &mut scene.tracks {
                        track.id = uuid::Uuid::new_v4().to_string();
                        track.initial.id = uuid::Uuid::new_v4().to_string();
                    }
                    asset.preserve_source_scene(&scene);
                }
                for (entry, id) in asset.entries.iter_mut().zip(entry_ids) {
                    if self.contains_expression_identity(id) {
                        return Err(Status::error("DUPLICATE_ID", id));
                    }
                    entry.id = id.clone();
                }
                plan.affected_entries = entry_ids.clone();
                plan.order.push(id.clone());
                plan.replacements.push(asset);
            }
            ExpressionEdit::Delete { id } => {
                let asset = owner(id)?;
                let refs = self.references_to(id);
                if !refs.is_empty() {
                    return Err(Status::error("OBJECT_REFERENCED", refs.join(", ")));
                }
                plan.affected_entries = asset.entries.iter().map(|e| e.id.clone()).collect();
                plan.removed.push(id.clone());
                plan.order.retain(|other| other != id);
            }
            ExpressionEdit::Reorder { ids } => {
                let wanted: HashSet<_> = self.expression_order().iter().collect();
                if ids.len() != wanted.len() || ids.iter().collect::<HashSet<_>>() != wanted {
                    return Err(Status::error(
                        "INVALID_ORDER",
                        "Order must contain every expression once",
                    ));
                }
                plan.order = ids.clone();
                plan.affected_expressions = ids.clone();
            }
        }
        let locked_source = |id: &str| -> bool {
            self.editor_state().get(id).locked
                || self
                    .get_expression_entry(id)
                    .is_some_and(|(owner, _)| self.editor_state().get(&owner.id).locked)
        };
        match &edit {
            ExpressionEdit::DuplicateEntries { ids, .. }
                if ids.iter().any(|id| locked_source(id)) =>
            {
                return Err(Status::error(
                    "OBJECT_LOCKED",
                    "A copied entry or its owner is locked",
                ));
            }
            ExpressionEdit::Duplicate { source, .. }
                if locked_source(source)
                    || self
                        .get_expression(source)
                        .unwrap()
                        .entries
                        .iter()
                        .any(|e| locked_source(&e.id)) =>
            {
                return Err(Status::error("OBJECT_LOCKED", source));
            }
            ExpressionEdit::Reorder { ids }
                if ids.iter().enumerate().any(|(index, id)| {
                    self.expression_order().get(index) != Some(id) && locked_source(id)
                }) =>
            {
                return Err(Status::error(
                    "OBJECT_LOCKED",
                    "A reordered expression is locked",
                ));
            }
            _ => {}
        }
        let replaced: HashSet<_> = plan.replacements.iter().map(|e| &e.id).collect();
        let mut final_ids = HashSet::new();
        let mut names = HashSet::new();
        for asset in &plan.replacements {
            let status = self.validate_expression(asset);
            // Old owner IDs are excluded from cross-owner checks below.
            if !status.is_ok() {
                return Err(status);
            }
            if !final_ids.insert(&asset.id) {
                return Err(Status::error("DUPLICATE_ID", &asset.id));
            }
            if !names.insert(&asset.name)
                || self
                    .content
                    .expressions
                    .values()
                    .any(|other| !replaced.contains(&other.id) && other.name == asset.name)
            {
                return Err(Status::error("DUPLICATE_EXPRESSION_NAME", &asset.name));
            }
            for entry in &asset.entries {
                if !final_ids.insert(&entry.id) {
                    return Err(Status::error("DUPLICATE_ID", &entry.id));
                }
                let existing_owner = self.get_expression_entry(&entry.id).map(|(a, _)| &a.id);
                if self.contains_expression_identity(&entry.id)
                    && !existing_owner.is_some_and(|id| replaced.contains(id))
                {
                    return Err(Status::error("DUPLICATE_ID", &entry.id));
                }
            }
        }
        for asset in &plan.replacements {
            self.validate_expression_unlocked(asset)?;
        }
        for id in &plan.removed {
            if self.editor_state().get(id).locked
                || self
                    .get_expression(id)
                    .unwrap()
                    .entries
                    .iter()
                    .any(|e| self.editor_state().get(&e.id).locked)
            {
                return Err(Status::error("OBJECT_LOCKED", id));
            }
        }
        if let ExpressionEdit::PatchEntry { id, .. } = &plan.edit {
            let (owner, _) = self.get_expression_entry(id).unwrap();
            if self.editor_state().get(id).locked || self.editor_state().get(&owner.id).locked {
                return Err(Status::error("OBJECT_LOCKED", id));
            }
        }
        plan.affected_expressions
            .extend(plan.replacements.iter().map(|e| e.id.clone()));
        plan.affected_expressions
            .extend(plan.removed.iter().cloned());
        plan.affected_expressions.sort();
        plan.affected_expressions.dedup();
        Ok(plan)
    }

    pub fn apply_expression_edit(&mut self, plan: &ExpressionPlan) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !Arc::ptr_eq(&plan.snapshot_identity, &self.snapshot_identity)
            || plan.document_id != self.id()
            || plan.revision != self.revision()
        {
            return self.failed(Status::error(
                "STALE_PLAN",
                "Replan against the current document",
            ));
        }
        if let ExpressionEdit::PatchEntry { id, fields, .. } = &plan.edit {
            let (owner, current) = self.get_expression_entry(id).unwrap();
            if ExpressionEntryFields::from(current) == *fields {
                return self.failed(Status::ok());
            }
            let owner = owner.id.clone();
            let slot = self.expression_entry_index()[id].1;
            let entry = &mut self.content.expressions.get_mut(&owner).unwrap().entries[slot];
            entry.target = fields.target.clone();
            entry.value = fields.value;
            entry.blend = fields.blend;
        } else {
            let changed = plan.order != self.content.expression_order
                || !plan.removed.is_empty()
                || plan
                    .replacements
                    .iter()
                    .any(|e| self.get_expression(&e.id) != Some(e));
            if !changed {
                return self.failed(Status::ok());
            }
            for id in &plan.removed {
                self.content.expressions.remove(id);
            }
            for id in &plan.affected_entries {
                if !plan
                    .replacements
                    .iter()
                    .any(|a| a.entries.iter().any(|e| e.id == *id))
                {
                    self.content.editor_state.objects.remove(id);
                }
            }
            for id in &plan.removed {
                self.content.editor_state.objects.remove(id);
            }

            for asset in &plan.replacements {
                self.content
                    .expressions
                    .insert(asset.id.clone(), asset.clone());
            }
            self.content.expression_order.clone_from(&plan.order);
            self.expression_entries.take();
        }
        let mut ids = plan.affected_expressions.clone();
        ids.extend(plan.affected_entries.clone());
        self.changed(ChangeKind::Metadata, vec![], ids)
    }

    fn validate_expression_entry_fields(
        &self,
        fields: &ExpressionEntryFields,
    ) -> Result<(), Status> {
        if !fields.value.is_finite() {
            return Err(Status::error(
                "INVALID_EXPRESSION_VALUE",
                "Value must be finite",
            ));
        }
        match &fields.target {
            ExpressionTarget::Resolved { parameter_id }
                if self.get_parameter(parameter_id).is_none() =>
            {
                Err(Status::error("MISSING_PARAMETER", parameter_id))
            }
            ExpressionTarget::Unresolved { runtime_id }
                if runtime_id.is_empty() || runtime_id.contains('\0') =>
            {
                Err(Status::error("INVALID_EXPRESSION_TARGET", runtime_id))
            }
            _ => Ok(()),
        }
    }
}
fn insertion_index(asset: &ExpressionAsset, before: &Option<String>) -> Result<usize, Status> {
    match before {
        Some(id) => asset
            .entries
            .iter()
            .position(|e| e.id == *id)
            .ok_or_else(|| Status::error("INVALID_PLACEMENT", id)),
        None => Ok(asset.entries.len()),
    }
}

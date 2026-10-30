//! Document properties and explicit repair of imported metadata references.
use super::*;
use crate::ModelSetup;
pub use zipzi_contract::document::{DocumentAuthoringEdit, DocumentField, DocumentValue};

#[derive(Debug, Clone)]
pub struct DocumentPlan {
    snapshot: Arc<()>,
    revision: u64,
    setup: ModelSetup,
    settings: Model3Settings,
    display: DisplayInfo,
    groups: Vec<MotionGroup>,
    roots: Vec<MotionRegistration>,
    missing: Vec<String>,
    affected: Vec<String>,
    noop: bool,
    losses: Vec<String>,
}
impl DocumentPlan {
    pub fn losses(&self) -> &[String] {
        &self.losses
    }
    pub fn is_noop(&self) -> bool {
        self.noop
    }
    pub fn affected_ids(&self) -> &[String] {
        &self.affected
    }
}
impl Document {
    pub fn document_value(&self, field: DocumentField) -> Result<DocumentValue, Status> {
        Ok(match field {
            DocumentField::Layout => DocumentValue::Layout(self.model3_settings().layout.clone()),
            DocumentField::ExportSettings => DocumentValue::ExportSettings(self.export_settings()),
            DocumentField::SourceCanvas => DocumentValue::SourceCanvas(self.source_canvas()),
            _ => {
                return Err(Status::error(
                    "SOURCE_METADATA_READ_ONLY",
                    "Inspect source metadata through its typed read API",
                ));
            }
        })
    }
    pub fn plan_document_edit(
        &self,
        intent: DocumentAuthoringEdit,
    ) -> Result<DocumentPlan, Status> {
        if !self.initialized() {
            return Err(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        let mut p = DocumentPlan {
            snapshot: self.snapshot_identity.clone(),
            revision: self.revision(),
            setup: self.model_setup(),
            settings: self.model3_settings().clone(),
            display: self.display_info().clone(),
            groups: self.motion_groups().to_vec(),
            roots: self.motion_roots().to_vec(),
            missing: self.missing_attachments().to_vec(),
            affected: vec![self.id().into()],
            noop: true,
            losses: vec![],
        };
        match intent {
            DocumentAuthoringEdit::Patch { expected, value } => {
                if expected.field() != value.field() {
                    return Err(Status::error("WRONG_DOCUMENT_FIELD", "Field type changed"));
                }
                if self.document_value(expected.field())? != expected {
                    return Err(Status::error("STALE_FIELD", "Document property changed"));
                }
                match value {
                    DocumentValue::Layout(value) => p.settings.layout = value,
                    DocumentValue::ExportSettings(value) => p.setup.export = value,
                    DocumentValue::SourceCanvas(_) => {
                        return Err(Status::error(
                            "SOURCE_METADATA_READ_ONLY",
                            "Source canvas is import provenance",
                        ));
                    }
                }
            }
            DocumentAuthoringEdit::RepairMissingAttachment {
                path,
                attachment_id,
            } => {
                if let Some(id) = &attachment_id {
                    if self.get_attachment(id).is_none() {
                        return Err(Status::error("MISSING_ATTACHMENT", id));
                    }
                }
                let matches = |reference: &Option<AttachmentRef>| matches!(reference,Some(AttachmentRef::Unresolved { path: old }) if old==&path);
                let replacement = attachment_id.as_ref().map(|id| AttachmentRef::Resolved {
                    attachment_id: id.clone(),
                });
                let mut found = false;
                for e in p
                    .groups
                    .iter_mut()
                    .flat_map(|g| &mut g.entries)
                    .chain(&mut p.roots)
                {
                    if matches(&e.sound) {
                        self.motion_unlocked(&e.id)?;
                        e.sound = replacement.clone();
                        p.affected.push(e.id.clone());
                        found = true;
                    }
                }
                if matches(&p.settings.user_data) {
                    p.settings.user_data = replacement;
                    found = true;
                }
                if !found && (attachment_id.is_some() || !p.missing.contains(&path)) {
                    return Err(Status::error("MISSING_REFERENCE", path));
                }
                p.missing.retain(|s| s != &path);
            }
            DocumentAuthoringEdit::RepairCdiParameter {
                runtime_id,
                parameter_id,
            } => {
                if let Some(id) = &parameter_id {
                    self.authoring_unlocked(id)?;
                    if self.get_parameter(id).is_none() {
                        return Err(Status::error("MISSING_PARAMETER", id));
                    }
                }
                let entries = p.display.parameters.get_or_insert_with(Vec::new);
                let mut found = false;
                let mut next = vec![];
                for entry in entries.drain(..) {
                    match entry {
                        CdiParameterEntry::Unresolved {
                            runtime_id: old,
                            name,
                            group_id,
                            extensions,
                        } if old == runtime_id => {
                            if let Some(group) = &group_id {
                                self.authoring_unlocked(group)?;
                            }
                            found = true;
                            if let Some(id) = &parameter_id {
                                if self.get_parameter(id).unwrap().name != name {
                                    p.losses.push(format!("{runtime_id}: imported display name {name:?} is replaced by the target Parameter name"));
                                }
                                next.push(CdiParameterEntry::Resolved {
                                    parameter_id: id.clone(),
                                    group_id,
                                    extensions,
                                });
                                p.affected.push(id.clone());
                            }
                        }
                        other => next.push(other),
                    }
                }
                *entries = next;
                if !found {
                    return Err(Status::error("MISSING_REFERENCE", runtime_id));
                }
            }
            DocumentAuthoringEdit::RepairCdiPart {
                runtime_id,
                part_id,
            } => {
                if let Some(id) = &part_id {
                    self.authoring_unlocked(id)?;
                    if self.get_part(id).is_none() {
                        return Err(Status::error("MISSING_PART", id));
                    }
                }
                let entries = p.display.parts.get_or_insert_with(Vec::new);
                let mut found = false;
                let mut next = vec![];
                for entry in entries.drain(..) {
                    match entry {
                        CdiPartEntry::Unresolved {
                            runtime_id: old,
                            name,
                            extensions,
                        } if old == runtime_id => {
                            found = true;
                            if let Some(id) = &part_id {
                                if self.get_part(id).unwrap().name != name {
                                    p.losses.push(format!("{runtime_id}: imported display name {name:?} is replaced by the target Part name"));
                                }
                                next.push(CdiPartEntry::Resolved {
                                    part_id: id.clone(),
                                    extensions,
                                });
                                p.affected.push(id.clone());
                            }
                        }
                        other => next.push(other),
                    }
                }
                *entries = next;
                if !found {
                    return Err(Status::error("MISSING_REFERENCE", runtime_id));
                }
            }
        }
        if !p.setup.is_valid() {
            return Err(Status::error(
                "INVALID_MODEL_SETUP",
                "Invalid export settings",
            ));
        }
        let mut candidate = self.fork_candidate();
        p.install(&mut candidate);
        if let Some(issue) = candidate.validate_structure().into_iter().next() {
            return Err(issue.status);
        }
        p.noop = self.same_content(&candidate);
        Ok(p)
    }
    pub fn apply_document_plan(&mut self, p: &DocumentPlan) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Finish the active edit",
            ));
        }
        if self.revision() != p.revision || !Arc::ptr_eq(&self.snapshot_identity, &p.snapshot) {
            return self.failed(Status::error("STALE_PLAN", "Replan document properties"));
        }
        if p.noop {
            return self.failed(Status::ok());
        }
        p.install(self);
        self.changed(ChangeKind::Metadata, vec![], p.affected.clone())
    }
}
impl DocumentPlan {
    fn install(&self, d: &mut Document) {
        if !super::organization::same_metadata_shape(d.display_info(), &self.display) {
            d.organization_index.take();
        }
        d.content.source_canvas = self.setup.source_canvas;
        d.content.export_settings = self.setup.export;
        d.content.model3_settings = self.settings.clone();
        d.content.display_info = self.display.clone();
        d.content.motion_groups = self.groups.clone();
        d.content.motion_roots = self.roots.clone();
        d.content.missing_attachments = self.missing.clone();
        d.reference_index.take();
        d.reference_projection.take();
        d.motion_lookup.take();
    }
}

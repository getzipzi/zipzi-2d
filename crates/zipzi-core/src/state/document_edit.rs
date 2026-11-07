use super::*;
#[derive(Debug, Clone)]
pub struct DocumentOutcome {
    pub affected: Vec<String>,
    pub losses: Vec<String>,
}
impl DocumentState {
    pub fn document_value(&self, field: DocumentField) -> Result<DocumentValue, Status> {
        Ok(match field {
            DocumentField::Layout => DocumentValue::Layout(self.model3_settings().layout.clone()),
            DocumentField::ExportSettings => DocumentValue::ExportSettings(self.setup().export),
            DocumentField::SourceCanvas => DocumentValue::SourceCanvas(self.setup().source_canvas),
            _ => {
                return Err(Status::error(
                    "SOURCE_METADATA_READ_ONLY",
                    "Inspect source metadata through its typed read API",
                ));
            }
        })
    }
}
impl DocumentBuilder {
    pub fn document_value(&self, field: DocumentField) -> Result<DocumentValue, Status> {
        Ok(match field {
            DocumentField::Layout => DocumentValue::Layout(self.model3_settings().layout.clone()),
            DocumentField::ExportSettings => DocumentValue::ExportSettings(self.setup().export),
            DocumentField::SourceCanvas => DocumentValue::SourceCanvas(self.setup().source_canvas),
            _ => {
                return Err(Status::error(
                    "SOURCE_METADATA_READ_ONLY",
                    "Inspect source metadata through its typed read API",
                ));
            }
        })
    }
    pub fn edit_document(
        &mut self,
        intent: DocumentAuthoringEdit,
    ) -> Result<DocumentOutcome, Status> {
        let mut outcome = None;
        self.write(|this| {
            struct Changes {
                setup: ModelSetup,
                settings: Model3Settings,
                display: DisplayInfo,
                groups: Vec<MotionGroup>,
                roots: Vec<MotionRegistration>,
                missing: Vec<String>,
                affected: Vec<String>,
                losses: Vec<String>,
            }
            let mut p = Changes {
                setup: this.setup(),
                settings: this.model3_settings().clone(),
                display: this.display_info().clone(),
                groups: this.motion_groups().clone(),
                roots: this.motion_roots().clone(),
                missing: this.missing_attachments().clone(),
                affected: vec![this.content.id.clone()],
                losses: vec![],
            };
        match intent {
            DocumentAuthoringEdit::Patch { expected, value } => {
                if expected.field() != value.field() {
                    return Err(Status::error("WRONG_DOCUMENT_FIELD", "Field type changed"));
                }
                if this.document_value(expected.field())? != expected {
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
                    if this.package_attachments().iter().find(|a|a.id==*id).is_none() {
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
                        this.authoring_unlocked(&e.id)?;
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
                    this.authoring_unlocked(id)?;
                    if this.parameter(id).is_none() {
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
                                this.authoring_unlocked(group)?;
                            }
                            found = true;
                            if let Some(id) = &parameter_id {
                                if this.parameter(id).unwrap().name != name {
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
                    this.authoring_unlocked(id)?;
                    if this.part(id).is_none() {
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
                                if this.part(id).unwrap().name != name {
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

            this.set_setup(p.setup)?;
            this.replace_model3_settings(p.settings)?;
            this.replace_display_info(p.display)?;
            this.set_motion_groups(p.groups)?;
            this.set_motion_roots(p.roots)?;
            this.set_missing_attachments(p.missing)?;
            outcome = Some(DocumentOutcome { affected: p.affected, losses: p.losses });
            Ok(())
        })?;
        Ok(outcome.unwrap())
    }
}

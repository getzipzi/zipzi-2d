use super::*;
pub(crate) struct AttachmentChanges {
    pub attachments: Vec<PackageAttachment>,
    pub groups: Vec<MotionGroup>,
    pub roots: Vec<MotionRegistration>,
    pub settings: Model3Settings,
}
impl Model<'_> {
    pub fn package_attachments(&self) -> &[PackageAttachment] {
        self.content.package_attachments
    }
    pub(crate) fn attachment_changes(
        &self,
        edit: AttachmentEdit,
    ) -> Result<AttachmentChanges, Status> {
        let mut c = AttachmentChanges {
            attachments: self.package_attachments().to_vec(),
            groups: self.motion_groups().to_vec(),
            roots: self.motion_roots().to_vec(),
            settings: self.model3_settings().clone(),
        };
        match edit {
            AttachmentEdit::Create { id, path } => {
                if self.contains_id(&id) || self.motion_identity_exists(&id) {
                    return Err(Status::error("DUPLICATE_ID", id));
                }
                let path = if path.is_empty() {
                    let mut path = "attachments/Attachment.bin".to_owned();
                    let mut n = 2;
                    while self.package_attachments().iter().any(|a| a.path == path) {
                        path = format!("attachments/Attachment ({n}).bin");
                        n += 1;
                    }
                    path
                } else {
                    path
                };
                c.attachments.push(PackageAttachment {
                    id,
                    path,
                    bytes: None,
                });
            }
            AttachmentEdit::Replace {
                expected,
                attachment,
            } => {
                if self.get_attachment(&expected.id) != Some(&expected)
                    || expected.id != attachment.id
                {
                    return Err(Status::error("STALE_FIELD", &expected.id));
                }
                *c.attachments
                    .iter_mut()
                    .find(|a| a.id == expected.id)
                    .unwrap() = attachment;
            }
            AttachmentEdit::Duplicate { source, id, path } => {
                if self.contains_id(&id) || self.motion_identity_exists(&id) {
                    return Err(Status::error("DUPLICATE_ID", id));
                }
                self.motion_unlocked(&source)?;
                let a = self
                    .get_attachment(&source)
                    .ok_or_else(|| Status::error("MISSING_ATTACHMENT", source))?;
                c.attachments.push(PackageAttachment {
                    id,
                    path,
                    bytes: a.bytes.clone(),
                });
            }
            AttachmentEdit::Delete { id } => {
                if self.get_attachment(&id).is_none() {
                    return Err(Status::error("MISSING_ATTACHMENT", id));
                }
                c.attachments.retain(|a| a.id != id);
            }
            AttachmentEdit::Reorder { ids, before } => {
                if ids.iter().collect::<HashSet<_>>().len() != ids.len() {
                    return Err(Status::error("DUPLICATE_SELECTION", "Repeated attachment"));
                }
                for id in &ids {
                    if self.get_attachment(id).is_none() {
                        return Err(Status::error("MISSING_ATTACHMENT", id));
                    }
                    self.motion_unlocked(id)?;
                }
                if let Some(anchor) = &before {
                    if self.get_attachment(anchor).is_none() {
                        return Err(Status::error("INVALID_PLACEMENT", anchor));
                    }
                    self.motion_unlocked(anchor)?;
                }
                if !before.as_ref().is_some_and(|id| ids.contains(id)) {
                    let selected: Vec<_> = c
                        .attachments
                        .as_slice()
                        .iter()
                        .filter(|a| ids.contains(&a.id))
                        .cloned()
                        .collect();
                    c.attachments.retain(|a| !ids.contains(&a.id));
                    let at = before
                        .as_ref()
                        .map_or(c.attachments.as_slice().len(), |id| {
                            c.attachments
                                .as_slice()
                                .iter()
                                .position(|a| a.id == *id)
                                .unwrap()
                        });
                    c.attachments.splice(at..at, selected);
                }
            }
            AttachmentEdit::DeleteWithReplacement { id, replacement } => {
                if self.get_attachment(&id).is_none() {
                    return Err(Status::error("MISSING_ATTACHMENT", &id));
                }
                if let Some(target) = &replacement {
                    if target == &id || self.get_attachment(target).is_none() {
                        return Err(Status::error("INVALID_ATTACHMENT_TARGET", target));
                    }
                }
                let changed = |r: &Option<AttachmentRef>| matches!(r, Some(AttachmentRef::Resolved { attachment_id }) if attachment_id == &id);
                let reference =
                    replacement.map(|attachment_id| AttachmentRef::Resolved { attachment_id });
                for e in c
                    .groups
                    .iter_mut()
                    .flat_map(|g| &mut g.entries)
                    .chain(&mut c.roots)
                {
                    if changed(&e.sound) {
                        self.motion_unlocked(&e.id)?;
                        e.sound = reference.clone();
                    }
                }
                if changed(&c.settings.user_data) {
                    c.settings.user_data = reference;
                }
                c.attachments.retain(|a| a.id != id);
            }
            AttachmentEdit::SetSound {
                registration,
                reference,
            } => {
                self.motion_unlocked(&registration)?;
                let mut found = false;
                for e in c
                    .groups
                    .iter_mut()
                    .flat_map(|g| &mut g.entries)
                    .chain(&mut c.roots)
                {
                    if e.id == registration {
                        e.sound = reference.clone();
                        found = true;
                    }
                }
                if !found {
                    return Err(Status::error("MISSING_MOTION_REGISTRATION", registration));
                }
            }
            AttachmentEdit::SetUserData { reference } => c.settings.user_data = reference,
        }

        for old in self.package_attachments() {
            if c.attachments.iter().find(|a| a.id == old.id) != Some(old) {
                self.motion_unlocked(&old.id)?;
            }
        }
        let final_model = Model::new(ModelContent {
            package_attachments: &c.attachments,
            motion_groups: &c.groups,
            motion_roots: &c.roots,
            model3_settings: &c.settings,
            ..self.content.clone()
        });
        final_model.validate_attachment_records()?;
        for old in self.package_attachments() {
            if !c.attachments.iter().any(|a| a.id == old.id) {
                let refs = final_model.references_to(&old.id);
                if !refs.is_empty() {
                    return Err(Status::error("OBJECT_REFERENCED", refs.join(", ")));
                }
            }
        }
        for status in [
            final_model.validate_motion_groups(&c.groups),
            final_model.validate_model3_settings(&c.settings),
        ] {
            if !status.is_ok() {
                return Err(status);
            }
        }
        Ok(c)
    }
    pub(crate) fn validate_attachment_records(&self) -> Result<(), Status> {
        let mut ids = HashSet::new();
        let mut paths = HashSet::new();
        for a in self.package_attachments() {
            if !valid_uuid(&a.id) || !ids.insert(&a.id) {
                return Err(Status::error("INVALID_ATTACHMENT_ID", &a.id));
            }
            if !valid_attachment_path(&a.path) || !paths.insert(&a.path) {
                return Err(Status::error("INVALID_ATTACHMENT_PATH", &a.path));
            }
            if a.bytes.as_ref().is_some_and(|b| b.len() > 50 * 1024 * 1024) {
                return Err(Status::error("ATTACHMENT_TOO_LARGE", &a.path));
            }
        }
        Ok(())
    }
}

//! Self-contained non-texture files referenced by model3 registrations.
use std::collections::HashSet;
use std::path::Component;

use super::Document;
use crate::types::{ChangeKind, EditResult, Status};

pub use zipzi_contract::document::{AttachmentEdit, AttachmentRef, PackageAttachment};

pub fn valid_attachment_path(path: &str) -> bool {
    !path.is_empty()
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && !path.contains('\\')
        && !path.contains('\0')
        && std::path::Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

impl Document {
    pub fn get_attachment(&self, id: &str) -> Option<&PackageAttachment> {
        self.package_attachments().iter().find(|a| a.id == id)
    }
    pub fn attachment_path<'a>(&'a self, reference: &'a AttachmentRef) -> Option<&'a str> {
        match reference {
            AttachmentRef::Resolved { attachment_id } => {
                self.get_attachment(attachment_id).map(|a| a.path.as_str())
            }
            AttachmentRef::Unresolved { path } => Some(path),
        }
    }
    pub fn attachment_available(&self, reference: &AttachmentRef) -> bool {
        match reference {
            AttachmentRef::Resolved { attachment_id } => self
                .get_attachment(attachment_id)
                .is_some_and(|a| a.bytes.is_some()),
            AttachmentRef::Unresolved { .. } => false,
        }
    }

    /// Import boundary: unresolved external paths become stable references once loaded.
    pub fn resolve_attachment_paths(&mut self) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        let paths: std::collections::HashMap<_, _> = self
            .package_attachments()
            .iter()
            .map(|a| (a.path.clone(), a.id.clone()))
            .collect();
        for e in self.motion_registrations() {
            if matches!(&e.sound,Some(AttachmentRef::Unresolved{path}) if paths.contains_key(path))
            {
                if let Err(status) = self.motion_unlocked(&e.id) {
                    return self.failed(status);
                }
            }
        }
        let mut changed = false;
        let mut resolve = |r: &mut AttachmentRef| {
            if let AttachmentRef::Unresolved { path } = r {
                if let Some(id) = paths.get(path) {
                    *r = AttachmentRef::Resolved {
                        attachment_id: id.clone(),
                    };
                    changed = true;
                }
            }
        };
        for e in self
            .content
            .motion_groups
            .iter_mut()
            .flat_map(|g| &mut g.entries)
            .chain(&mut self.content.motion_roots)
        {
            if let Some(r) = &mut e.sound {
                resolve(r);
            }
        }
        if let Some(r) = &mut self.content.model3_settings.user_data {
            resolve(r);
        }
        if changed {
            self.changed(ChangeKind::Metadata, vec![], paths.into_values().collect())
        } else {
            self.failed(Status::ok())
        }
    }
    pub fn package_attachments(&self) -> &[PackageAttachment] {
        &self.content.package_attachments
    }

    pub fn set_package_attachments(&mut self, attachments: Vec<PackageAttachment>) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        let mut paths = HashSet::new();
        let mut ids = HashSet::new();
        for attachment in &attachments {
            if !super::valid_uuid(&attachment.id)
                || !ids.insert(&attachment.id)
                || (self.contains_id(&attachment.id)
                    && self.get_attachment(&attachment.id).is_none())
            {
                return self.failed(Status::error("INVALID_ATTACHMENT_ID", &attachment.id));
            }
            if !valid_attachment_path(&attachment.path) || !paths.insert(attachment.path.as_str()) {
                return self.failed(Status::error("INVALID_ATTACHMENT_PATH", &attachment.path));
            }
            if attachment
                .bytes
                .as_ref()
                .is_some_and(|b| b.len() > 50 * 1024 * 1024)
            {
                return self.failed(Status::error("ATTACHMENT_TOO_LARGE", &attachment.path));
            }
        }
        if let Err(status) = self.validate_attachment_replacement(&attachments) {
            return self.failed(status);
        }
        if self.content.package_attachments == attachments {
            return self.failed(Status::ok());
        }
        let removed: Vec<_> = self
            .package_attachments()
            .iter()
            .filter(|a| !attachments.iter().any(|n| n.id == a.id))
            .map(|a| a.id.clone())
            .collect();
        for id in removed {
            self.content.editor_state.objects.remove(&id);
        }
        self.content.package_attachments = attachments;
        self.changed(
            ChangeKind::Metadata,
            Vec::new(),
            self.package_attachments()
                .iter()
                .map(|a| a.id.clone())
                .collect(),
        )
    }
}

#[derive(Debug, Clone)]
pub struct AttachmentPlan {
    snapshot: std::sync::Arc<()>,
    revision: u64,
    attachments: Vec<PackageAttachment>,
    groups: Vec<super::MotionGroup>,
    roots: Vec<super::MotionRegistration>,
    settings: super::Model3Settings,
}

impl Document {
    pub fn plan_attachment_edit(&self, edit: AttachmentEdit) -> Result<AttachmentPlan, Status> {
        if !self.initialized() {
            return Err(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        let mut c = self.motion_authoring_candidate();
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
                c.content.package_attachments.push(PackageAttachment {
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
                *c.content
                    .package_attachments
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
                c.content.package_attachments.push(PackageAttachment {
                    id,
                    path,
                    bytes: a.bytes.clone(),
                });
            }
            AttachmentEdit::Delete { id } => {
                if self.get_attachment(&id).is_none() {
                    return Err(Status::error("MISSING_ATTACHMENT", id));
                }
                c.content.package_attachments.retain(|a| a.id != id);
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
                        .package_attachments()
                        .iter()
                        .filter(|a| ids.contains(&a.id))
                        .cloned()
                        .collect();
                    c.content
                        .package_attachments
                        .retain(|a| !ids.contains(&a.id));
                    let at = before.as_ref().map_or(c.package_attachments().len(), |id| {
                        c.package_attachments()
                            .iter()
                            .position(|a| a.id == *id)
                            .unwrap()
                    });
                    c.content.package_attachments.splice(at..at, selected);
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
                    .content
                    .motion_groups
                    .iter_mut()
                    .flat_map(|g| &mut g.entries)
                    .chain(&mut c.content.motion_roots)
                {
                    if changed(&e.sound) {
                        self.motion_unlocked(&e.id)?;
                        e.sound = reference.clone();
                    }
                }
                if changed(&c.content.model3_settings.user_data) {
                    c.content.model3_settings.user_data = reference;
                }
                c.content.package_attachments.retain(|a| a.id != id);
            }
            AttachmentEdit::SetSound {
                registration,
                reference,
            } => {
                self.motion_unlocked(&registration)?;
                let mut found = false;
                for e in c
                    .content
                    .motion_groups
                    .iter_mut()
                    .flat_map(|g| &mut g.entries)
                    .chain(&mut c.content.motion_roots)
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
            AttachmentEdit::SetUserData { reference } => {
                c.content.model3_settings.user_data = reference
            }
        }
        // Validate resource records on an isolated metadata workspace, then the
        // complete final graph. Removed consumers were explicitly redirected above.
        let mut final_state = self.fork_candidate();
        final_state.content.motion_groups = c.content.motion_groups.clone();
        final_state.content.motion_roots = c.content.motion_roots.clone();
        final_state.content.model3_settings = c.content.model3_settings.clone();
        let attachments = c.package_attachments().to_vec();
        let r = final_state.set_package_attachments(attachments);
        if !r.status.is_ok() {
            return Err(r.status);
        }
        for old in self.package_attachments() {
            if final_state.get_attachment(&old.id) != Some(old) {
                self.motion_unlocked(&old.id)?;
            }
        }
        if let Some(issue) = final_state.validate_structure().into_iter().next() {
            return Err(issue.status);
        }
        Ok(AttachmentPlan {
            snapshot: self.snapshot_identity.clone(),
            revision: self.revision(),
            attachments: c.content.package_attachments,
            groups: c.content.motion_groups,
            roots: c.content.motion_roots,
            settings: c.content.model3_settings,
        })
    }
    pub fn apply_attachment_edit(&mut self, plan: &AttachmentPlan) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if self.revision() != plan.revision
            || !std::sync::Arc::ptr_eq(&self.snapshot_identity, &plan.snapshot)
        {
            return self.failed(Status::error(
                "STALE_PLAN",
                "Replan against current document",
            ));
        }
        if self.content.package_attachments == plan.attachments
            && self.content.motion_groups == plan.groups
            && self.content.motion_roots == plan.roots
            && self.content.model3_settings == plan.settings
        {
            return self.failed(Status::ok());
        }
        let ids = self
            .package_attachments()
            .iter()
            .chain(&plan.attachments)
            .map(|a| a.id.clone())
            .chain(self.motion_registrations().map(|e| e.id.clone()))
            .collect();
        let removed: Vec<_> = self
            .package_attachments()
            .iter()
            .filter(|a| !plan.attachments.iter().any(|n| n.id == a.id))
            .map(|a| a.id.clone())
            .collect();
        for id in removed {
            self.content.editor_state.objects.remove(&id);
        }
        self.content.package_attachments = plan.attachments.clone();
        self.content.motion_groups = plan.groups.clone();
        self.content.motion_roots = plan.roots.clone();
        self.content.model3_settings = plan.settings.clone();
        self.changed(ChangeKind::Metadata, vec![], ids)
    }
    fn validate_attachment_replacement(
        &self,
        attachments: &[PackageAttachment],
    ) -> Result<(), Status> {
        for old in self.package_attachments() {
            let new = attachments.iter().find(|a| a.id == old.id);
            if Some(old) != new {
                self.motion_unlocked(&old.id)?;
            }
            if new.is_none() {
                let refs = self.references_to(&old.id);
                if !refs.is_empty() {
                    return Err(Status::error("OBJECT_REFERENCED", refs.join(", ")));
                }
            }
        }
        Ok(())
    }
}

//! Self-contained non-texture files referenced by model3 registrations.
use std::collections::HashSet;
use std::path::Component;

use serde::{Deserialize, Serialize};

use super::Document;
use crate::types::{ChangeKind, EditResult, Status};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageAttachment {
    pub id: String,
    pub path: String,
    pub bytes: Option<std::sync::Arc<Vec<u8>>>,
}

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

/// Imported paths remain explicitly unresolved until a matching attachment is loaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AttachmentRef {
    Resolved { attachment_id: String },
    Unresolved { path: String },
}
impl From<String> for AttachmentRef {
    fn from(path: String) -> Self {
        Self::Unresolved { path }
    }
}
impl From<&str> for AttachmentRef {
    fn from(path: &str) -> Self {
        path.to_owned().into()
    }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum AttachmentEdit {
    Create {
        id: String,
        path: String,
    },
    Replace {
        expected: PackageAttachment,
        attachment: PackageAttachment,
    },
    Duplicate {
        source: String,
        id: String,
        path: String,
    },
    Delete {
        id: String,
    },
    SetSound {
        registration: String,
        reference: Option<AttachmentRef>,
    },
    SetUserData {
        reference: Option<AttachmentRef>,
    },
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
        // Validate complete final state, without temporarily publishing invalid references.
        let attachments = c.content.package_attachments.clone();
        let result = self
            .motion_authoring_candidate()
            .set_package_attachments(attachments);
        if !result.status.is_ok() {
            return Err(result.status);
        }
        self.validate_attachment_replacement(c.package_attachments())?;
        let status = c.validate_motion_groups(c.motion_groups());
        if !status.is_ok() {
            return Err(status);
        }
        // Only UserData can change here. Scene references remain validated by
        // the original document, whose geometry is not copied into this workspace.
        let status = c.validate_model3_user_data(c.model3_settings().user_data.as_ref());
        if !status.is_ok() {
            return Err(status);
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

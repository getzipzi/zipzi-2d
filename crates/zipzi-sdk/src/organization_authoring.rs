//! SDK session authority for parameter folders and Part organization.
use crate::{
    AuthoringSession, CapabilityState, EditReceipt, EditSession, ObjectHandle, ObjectKind,
    SdkError, Version,
};
use std::collections::BTreeMap;
use zipzi_core::document::OrganizationPlan;
pub use zipzi_core::document::{
    OrganizationCollection, OrganizationDeletePolicy, OrganizationEdit, OrganizationEffects,
};
#[derive(Debug, Clone)]
pub struct OrganizationEditPlan {
    version: Version,
    domain: OrganizationPlan,
}
impl OrganizationEditPlan {
    pub fn is_noop(&self) -> bool {
        self.domain.is_noop()
    }
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn affected_ids(&self) -> &[String] {
        self.domain.affected_ids()
    }
    pub fn copied_ids(&self) -> &BTreeMap<String, String> {
        self.domain.copied_ids()
    }
    pub fn effects(&self) -> &OrganizationEffects {
        self.domain.effects()
    }
}
#[derive(Debug, Clone, zipzi_api_derive::ApiShape, serde::Serialize)]
pub struct OrganizationCapabilities {
    /// Current capability to read the current typed record; inspect its state/reason before
    /// invoking the operation.
    pub inspect: CapabilityState,
    /// Current capability to modify fields through a captured address; inspect its state/reason
    /// before invoking the operation.
    pub patch: CapabilityState,
    /// Current capability to create children in this collection; inspect its state/reason
    /// before invoking the operation.
    pub create_child: CapabilityState,
    /// Current capability to move this object between collections; inspect its state/reason
    /// before invoking the operation.
    pub move_object: CapabilityState,
    /// Current capability to change its collection order; inspect its state/reason before
    /// invoking the operation.
    pub reorder: CapabilityState,
    /// Current capability to duplicate this object; inspect its state/reason before invoking
    /// the operation.
    pub duplicate: CapabilityState,
    /// Current capability to delete this object; inspect its state/reason before invoking the
    /// operation.
    pub delete: CapabilityState,
    /// Current capability to remove its container while preserving contents; inspect its
    /// state/reason before invoking the operation.
    pub dissolve: CapabilityState,
}
#[derive(Debug, Clone)]
pub struct OrganizationCollectionCapabilities {
    pub create: CapabilityState,
    pub move_into: CapabilityState,
    pub reorder: CapabilityState,
}
#[derive(Debug, Clone)]
pub struct OrganizationAddress {
    entity: ObjectHandle,
    owner: Option<ObjectHandle>,
    epoch: u64,
}
impl OrganizationAddress {
    pub fn id(&self) -> &str {
        self.entity.id()
    }
}
pub(crate) fn kind(d: &zipzi_core::Document, id: &str) -> Option<ObjectKind> {
    if d.get_parameter(id).is_some() {
        Some(ObjectKind::Parameter)
    } else if d.get_parameter_group(id).is_some() {
        Some(ObjectKind::CdiParameterGroup)
    } else if d.get_part(id).is_some() {
        Some(ObjectKind::Part)
    } else if d.get_mesh(id).is_some() {
        Some(ObjectKind::Mesh)
    } else if d.get_transform(id).is_some() {
        Some(ObjectKind::Transform)
    } else if d.get_offscreen(id).is_some() {
        Some(ObjectKind::Offscreen)
    } else {
        None
    }
}
impl AuthoringSession {
    pub fn plan_organization_edit(
        &self,
        intent: OrganizationEdit,
    ) -> Result<OrganizationEditPlan, SdkError> {
        let users = match &intent {
            OrganizationEdit::Delete { ids, policy } => self
                .document()
                .organization_delete_users(ids, *policy)
                .unwrap_or_default(),
            _ => vec![],
        };
        let domain = self
            .document()
            .plan_organization_edit(intent)
            .map_err(|s| {
                let mut error = SdkError::from_status(s, "plan_organization_edit", vec![]);
                if error.code.as_ref() == "OBJECT_REFERENCED" {
                    error.referrers = users.into();
                }
                error
            })?;
        Ok(OrganizationEditPlan {
            version: self.version(),
            domain,
        })
    }
    pub fn execute_organization_plan(
        &mut self,
        plan: &OrganizationEditPlan,
    ) -> Result<EditReceipt, SdkError> {
        let (_, receipt) = self.edit("Edit organization", Some(plan.version), |e| {
            e.apply_organization_plan(plan)
        })?;
        Ok(receipt)
    }
    pub fn organization_collection(
        &self,
        collection: OrganizationCollection,
    ) -> Result<Vec<ObjectHandle>, SdkError> {
        let ids = self
            .document()
            .organization_collection(&collection)
            .map_err(|s| SdkError::from_status(s, "organization_collection", vec![]))?;
        ids.iter()
            .map(|id| self.handle(kind(self.document(), id).unwrap(), id))
            .collect()
    }
    pub fn organization_collection_capabilities(
        &self,
        c: &OrganizationCollection,
    ) -> Result<OrganizationCollectionCapabilities, SdkError> {
        let owner = match c {
            OrganizationCollection::Parameters { group_id } => group_id.as_deref(),
            OrganizationCollection::ParameterGroups { parent_id } => parent_id.as_deref(),
            OrganizationCollection::PartMembers { part_id } => part_id.as_deref(),
        };
        if let Some(owner) = owner {
            self.handle(
                if matches!(c, OrganizationCollection::PartMembers { .. }) {
                    ObjectKind::Part
                } else {
                    ObjectKind::CdiParameterGroup
                },
                owner,
            )?;
        }
        let state = if owner.is_some_and(|id| self.document().organization_unlocked(id).is_err()) {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        Ok(OrganizationCollectionCapabilities {
            create: state.clone(),
            move_into: state.clone(),
            reorder: state,
        })
    }
    pub fn organization_capabilities(
        &self,
        h: &ObjectHandle,
    ) -> Result<OrganizationCapabilities, SdkError> {
        self.resolve_handle(h)?;
        let d = self.document();
        let supported = kind(d, h.id()) == Some(h.kind());
        let unavailable = |code| CapabilityState::Unavailable { code };
        let unsupported = |code| CapabilityState::Unsupported { code };
        let state = if !supported {
            unsupported("WRONG_OBJECT_KIND")
        } else if d.organization_unlocked(h.id()).is_err() {
            unavailable("OBJECT_LOCKED")
        } else {
            CapabilityState::Available
        };
        let subtree = if supported && d.organization_subtree_unlocked(&[h.id().into()]).is_err() {
            unavailable("OBJECT_LOCKED")
        } else {
            state.clone()
        };
        let container = matches!(h.kind(), ObjectKind::Part | ObjectKind::CdiParameterGroup);
        let unresolved_group_copy = h.kind() == ObjectKind::CdiParameterGroup && d.organization_subtree(&[h.id().into()]).is_ok_and(|ids| {
            d.display_info().parameters.iter().flatten().any(|entry| matches!(entry,
                zipzi_core::document::CdiParameterEntry::Unresolved { group_id: Some(group), .. } if ids.contains(group)))
        });
        Ok(OrganizationCapabilities {
            inspect: CapabilityState::Available,
            patch: state.clone(),
            create_child: if container {
                state.clone()
            } else {
                unsupported("NO_CHILD_COLLECTION")
            },
            move_object: subtree.clone(),
            reorder: subtree.clone(),
            duplicate: if unresolved_group_copy {
                unavailable("UNRESOLVED_PARAMETER_COPY_CONFLICT")
            } else {
                subtree.clone()
            },
            delete: if supported
                && h.kind() != ObjectKind::Part
                && !d
                    .organization_delete_users(&[h.id().into()], OrganizationDeletePolicy::Contents)
                    .unwrap_or_default()
                    .is_empty()
            {
                unavailable("OBJECT_REFERENCED")
            } else {
                subtree.clone()
            },
            dissolve: if h.kind() == ObjectKind::Part
                && !d.part_dissolve_state_representable(h.id())
            {
                unavailable("PART_DISSOLVE_STATE_UNREPRESENTABLE")
            } else if container {
                subtree
            } else {
                unsupported("DISSOLVE_REQUIRES_CONTAINER")
            },
        })
    }
    pub fn organization_address(&self, id: &str) -> Result<OrganizationAddress, SdkError> {
        let d = self.document();
        let entity = self.handle(
            kind(d, id).ok_or_else(|| {
                SdkError::new("MISSING_ORGANIZATION_OBJECT", id, "organization_address")
            })?,
            id,
        )?;
        let owner = d
            .organization_owner(id)
            .map(|id| self.handle(kind(d, id).unwrap(), id))
            .transpose()?;
        Ok(OrganizationAddress {
            entity,
            owner,
            epoch: self.organization_contexts.get(id).map_or(0, |v| v.1),
        })
    }
    pub fn plan_organization_edit_at(
        &self,
        address: &OrganizationAddress,
        intent: OrganizationEdit,
    ) -> Result<OrganizationEditPlan, SdkError> {
        self.validate_field_address(
            std::iter::once(&address.entity).chain(&address.owner),
            || {
                self.organization_contexts
                    .get(address.id())
                    .map_or(0, |v| v.1)
                    != address.epoch
                    || self.document().organization_owner(address.id())
                        != address.owner.as_ref().map(ObjectHandle::id)
            },
            "Capture a new organization address",
            "plan_organization_edit_at",
        )?;
        let targets = match &intent {
            OrganizationEdit::SetParameterGroup { parameter_id, .. } => {
                parameter_id == address.id()
            }
            OrganizationEdit::PatchGroup { group, .. } => group.id == address.id(),
            OrganizationEdit::Move { ids, .. }
            | OrganizationEdit::Duplicate { ids, .. }
            | OrganizationEdit::Delete { ids, .. } => ids.iter().any(|id| id == address.id()),
            _ => false,
        };
        if !targets {
            return Err(SdkError::new(
                "WRONG_FIELD_ADDRESS",
                "Intent targets another entity",
                "plan_organization_edit_at",
            ));
        }
        self.plan_organization_edit(intent)
    }
}
impl EditSession<'_> {
    pub fn apply_organization_plan(&mut self, plan: &OrganizationEditPlan) -> Result<(), SdkError> {
        self.ensure_plan_version(
            plan.version,
            "apply_organization_plan",
            "Replan organization",
        )?;
        self.apply_authoring_change(
            "apply_organization_plan",
            "",
            plan.domain.preserves_identities(),
            |document| document.apply_organization_plan(&plan.domain),
        )?;
        Ok(())
    }
    pub fn edit_organization(&mut self, intent: OrganizationEdit) -> Result<(), SdkError> {
        self.ensure_active("edit_organization")?;
        let users = match &intent {
            OrganizationEdit::Delete { ids, policy } => self
                .candidate_document()
                .organization_delete_users(ids, *policy)
                .unwrap_or_default(),
            _ => vec![],
        };
        let plan = self
            .candidate_document()
            .plan_organization_edit(intent)
            .map_err(|s| {
                let mut error = SdkError::from_status(s, "edit_organization", vec![]);
                if error.code.as_ref() == "OBJECT_REFERENCED" {
                    error.referrers = users.into();
                }
                self.abort_with(error)
            })?;
        self.apply_authoring_change(
            "edit_organization",
            "",
            plan.preserves_identities(),
            |document| document.apply_organization_plan(&plan),
        )?;
        Ok(())
    }
}

impl crate::AuthoringSnapshot {
    pub fn plan_organization_edit(
        &self,
        intent: OrganizationEdit,
    ) -> Result<OrganizationEditPlan, SdkError> {
        Ok(OrganizationEditPlan {
            version: self.version(),
            domain: self
                .document()
                .plan_organization_edit(intent)
                .map_err(|s| SdkError::from_status(s, "plan_organization_edit", vec![]))?,
        })
    }
}

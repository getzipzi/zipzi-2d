//! Pose planning is owned by core; SDK adds session lifetime and field baselines.
use crate::{
    AuthoringSession, CapabilityState, EditReceipt, EditSession, ObjectHandle, ObjectKind,
    SdkError, Version,
};
use kasane_core::document::{DisabledPoseItem, PosePlan};
pub use kasane_core::document::{PoseCollection, PoseEdit, PoseFields};
#[derive(Debug, Clone)]
pub struct PoseEditPlan {
    version: Version,
    domain: PosePlan,
}
impl PoseEditPlan {
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn affected_ids(&self) -> &[String] {
        self.domain.affected_ids()
    }
}
#[derive(Debug, Clone)]
pub struct PoseCapabilities {
    pub inspect: CapabilityState,
    pub patch: CapabilityState,
    pub create_child: CapabilityState,
    pub duplicate: CapabilityState,
    pub delete: CapabilityState,
    pub move_object: CapabilityState,
    pub reorder: CapabilityState,
    pub repair: CapabilityState,
    pub dissolve: CapabilityState,
    pub preview: CapabilityState,
}
#[derive(Debug, Clone)]
pub struct PoseCollectionCapabilities {
    pub create: CapabilityState,
    pub move_into: CapabilityState,
    pub reorder: CapabilityState,
}
#[derive(Debug, Clone)]
pub struct PoseFieldAddress {
    entity: ObjectHandle,
    owner: Option<ObjectHandle>,
    epoch: u64,
    expected: PoseFields,
}
impl PoseFieldAddress {
    pub fn id(&self) -> &str {
        self.entity.id()
    }
    pub fn expected(&self) -> &PoseFields {
        &self.expected
    }
}
#[derive(Debug, Clone)]
pub struct PoseExportPlan {
    version: Version,
    pub omitted: Vec<DisabledPoseItem>,
}
impl AuthoringSession {
    pub fn plan_pose_edit(&self, intent: PoseEdit) -> Result<PoseEditPlan, SdkError> {
        let domain = self
            .document()
            .plan_pose_edit(intent)
            .map_err(|s| SdkError::from_status(s, "plan_pose_edit", vec![]))?;
        Ok(PoseEditPlan {
            version: self.version(),
            domain,
        })
    }
    pub fn execute_pose_plan(&mut self, plan: &PoseEditPlan) -> Result<EditReceipt, SdkError> {
        let (_, receipt) =
            self.edit("Edit Pose", Some(plan.version), |e| e.apply_pose_plan(plan))?;
        Ok(receipt)
    }
    pub fn pose_collection_capabilities(
        &self,
        collection: &PoseCollection,
    ) -> Result<PoseCollectionCapabilities, SdkError> {
        let owner = match collection {
            PoseCollection::Groups => self.document().pose().map(|p| p.id.as_str()),
            PoseCollection::Entries { group_id } => Some(group_id.as_str()),
            PoseCollection::Links { entry_id } => Some(entry_id.as_str()),
        };
        let state = if let Some(id) = owner {
            let kind = pose_kind(self.document(), id);
            self.handle(kind, id)?;
            let valid = matches!(
                (collection, kind),
                (PoseCollection::Groups, ObjectKind::Pose)
                    | (PoseCollection::Entries { .. }, ObjectKind::PoseGroup)
                    | (PoseCollection::Links { .. }, ObjectKind::PoseEntry)
            );
            if !valid {
                return Err(SdkError::new(
                    "WRONG_POSE_COLLECTION",
                    id,
                    "pose_collection_capabilities",
                ));
            }
            if self.document().pose_unlocked(id).is_err() {
                CapabilityState::Unavailable {
                    code: "OBJECT_LOCKED",
                }
            } else {
                CapabilityState::Available
            }
        } else {
            CapabilityState::Available
        };
        let structural = if owner.is_none() {
            CapabilityState::Unavailable {
                code: "MISSING_POSE",
            }
        } else {
            state.clone()
        };
        Ok(PoseCollectionCapabilities {
            create: state,
            move_into: structural.clone(),
            reorder: structural,
        })
    }
    pub fn pose_collection(
        &self,
        collection: PoseCollection,
    ) -> Result<Vec<ObjectHandle>, SdkError> {
        let d = self.document();
        let missing = |id: &str| SdkError::new("MISSING_POSE_OBJECT", id, "pose_collection");
        let (kind, ids): (ObjectKind, Vec<&str>) = match collection {
            PoseCollection::Groups => (
                ObjectKind::PoseGroup,
                d.pose()
                    .map(|p| p.groups.iter().map(|g| g.id.as_str()).collect())
                    .unwrap_or_default(),
            ),
            PoseCollection::Entries { group_id } => (
                ObjectKind::PoseEntry,
                d.get_pose_group(&group_id)
                    .ok_or_else(|| missing(&group_id))?
                    .entries
                    .iter()
                    .map(|e| e.id.as_str())
                    .collect(),
            ),
            PoseCollection::Links { entry_id } => (
                ObjectKind::PoseLink,
                d.get_pose_entry(&entry_id)
                    .ok_or_else(|| missing(&entry_id))?
                    .1
                    .links
                    .iter()
                    .flatten()
                    .map(|l| l.id.as_str())
                    .collect(),
            ),
        };
        ids.into_iter().map(|id| self.handle(kind, id)).collect()
    }
    pub fn pose_capabilities(&self, handle: &ObjectHandle) -> Result<PoseCapabilities, SdkError> {
        self.resolve_handle(handle)?;
        let kind = handle.kind();
        let supported = matches!(
            kind,
            ObjectKind::Pose | ObjectKind::PoseGroup | ObjectKind::PoseEntry | ObjectKind::PoseLink
        );
        let unsupported = |code| CapabilityState::Unsupported { code };
        let state = if !supported {
            unsupported("WRONG_OBJECT_KIND")
        } else if self.document().pose_unlocked(handle.id()).is_err() {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        let subtree = if supported && self.document().pose_subtree_unlocked(handle.id()).is_err() {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            state.clone()
        };
        Ok(PoseCapabilities {
            inspect: CapabilityState::Available,
            patch: state.clone(),
            create_child: if matches!(
                kind,
                ObjectKind::Pose | ObjectKind::PoseGroup | ObjectKind::PoseEntry
            ) {
                state.clone()
            } else {
                unsupported("NO_CHILD_COLLECTION")
            },
            duplicate: if kind == ObjectKind::Pose {
                unsupported("POSE_SINGLETON")
            } else {
                subtree.clone()
            },
            delete: subtree.clone(),
            move_object: if kind == ObjectKind::Pose {
                unsupported("POSE_SINGLETON")
            } else {
                subtree.clone()
            },
            reorder: if kind == ObjectKind::Pose {
                unsupported("POSE_SINGLETON")
            } else {
                subtree.clone()
            },
            repair: if matches!(kind, ObjectKind::PoseEntry | ObjectKind::PoseLink) {
                state
            } else {
                unsupported("NO_REFERENCE_SLOT")
            },
            dissolve: if kind == ObjectKind::PoseGroup
                && self.document().pose().is_some_and(|p| p.groups.len() < 2)
            {
                CapabilityState::Unavailable {
                    code: "POSE_RECEIVING_GROUP_REQUIRED",
                }
            } else if kind == ObjectKind::PoseGroup {
                subtree
            } else {
                unsupported("REQUIRES_POSE_GROUP")
            },
            preview: if kind == ObjectKind::Pose {
                CapabilityState::Available
            } else {
                unsupported("PREVIEW_POSE_OWNER")
            },
        })
    }
    pub fn pose_field_address(&self, id: &str) -> Result<PoseFieldAddress, SdkError> {
        let expected = self
            .document()
            .pose_fields(id)
            .ok_or_else(|| SdkError::new("MISSING_POSE_OBJECT", id, "pose_field_address"))?;
        let owner = self
            .document()
            .pose_owner(id)
            .map(|owner| self.handle(pose_kind(self.document(), owner), owner))
            .transpose()?;
        Ok(PoseFieldAddress {
            entity: self.handle(pose_kind(self.document(), id), id)?,
            owner,
            epoch: self.pose_contexts.get(id).map_or(0, |c| c.1),
            expected,
        })
    }
    pub fn plan_pose_field_patch(
        &self,
        address: &PoseFieldAddress,
        fields: PoseFields,
    ) -> Result<PoseEditPlan, SdkError> {
        self.resolve_handle(&address.entity)?;
        if let Some(owner) = &address.owner {
            self.resolve_handle(owner)?;
        }
        if self.pose_contexts.get(address.id()).map_or(0, |c| c.1) != address.epoch
            || self.document().pose_owner(address.id()) != address.owner.as_ref().map(|o| o.id())
            || self.document().pose_fields(address.id()).as_ref() != Some(&address.expected)
        {
            return Err(SdkError::new(
                "STALE_FIELD_ADDRESS",
                "Capture a new Pose address",
                "plan_pose_field_patch",
            ));
        }
        self.plan_pose_edit(PoseEdit::Patch {
            id: address.id().into(),
            expected: address.expected.clone(),
            fields,
        })
    }
    pub fn plan_available_pose_export(&self) -> Result<PoseExportPlan, SdkError> {
        let omitted = self
            .document()
            .pose_projection()
            .map_err(|s| SdkError::from_status(s, "plan_available_pose_export", vec![]))?
            .map_or_else(Vec::new, |p| p.disabled);
        Ok(PoseExportPlan {
            version: self.version(),
            omitted,
        })
    }
    pub fn export_pose_plan(&self, plan: &PoseExportPlan) -> Result<Option<String>, SdkError> {
        if self.version() != plan.version {
            return Err(SdkError::new(
                "STALE_PLAN",
                "Replan export omissions",
                "export_pose_plan",
            ));
        }
        kasane_project::export_available_pose3(self.document())
            .map_err(|e| SdkError::new(&e.code, &e.message, "export_pose_plan"))
    }
}
fn pose_kind(d: &kasane_core::Document, id: &str) -> ObjectKind {
    if d.get_pose_group(id).is_some() {
        ObjectKind::PoseGroup
    } else if d.get_pose_entry(id).is_some() {
        ObjectKind::PoseEntry
    } else if d.get_pose_link(id).is_some() {
        ObjectKind::PoseLink
    } else {
        ObjectKind::Pose
    }
}
impl EditSession<'_> {
    pub fn apply_pose_plan(&mut self, plan: &PoseEditPlan) -> Result<(), SdkError> {
        self.ensure_active("apply_pose_plan")?;
        if self.base_version() != plan.version {
            return Err(self.abort_with(SdkError::new(
                "STALE_PLAN",
                "Replan Pose",
                "apply_pose_plan",
            )));
        }
        let before = (!plan.domain.preserves_identities())
            .then(|| crate::session::keys_in(self.candidate_document()));
        let result = self.document().apply_pose_plan(&plan.domain);
        self.record(result, "apply_pose_plan", "")?;
        if let Some(before) = before {
            self.track_removed_objects(before);
        }
        Ok(())
    }
    pub fn edit_pose(&mut self, intent: PoseEdit) -> Result<(), SdkError> {
        self.ensure_active("edit_pose")?;
        let plan = self
            .candidate_document()
            .plan_pose_edit(intent)
            .map_err(|s| self.abort_with(SdkError::from_status(s, "edit_pose", vec![])))?;
        let before = (!plan.preserves_identities())
            .then(|| crate::session::keys_in(self.candidate_document()));
        let result = self.document().apply_pose_plan(&plan);
        self.record(result, "edit_pose", "")?;
        if let Some(before) = before {
            self.track_removed_objects(before);
        }
        Ok(())
    }
}

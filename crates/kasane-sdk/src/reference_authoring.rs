//! Native reference plans with SDK identity and monotonic field contexts.
use crate::{
    AuthoringSession, CapabilityState, EditReceipt, EditSession, ObjectHandle, ObjectKind,
    SdkError, Version,
};
pub use kasane_core::document::{
    DisabledReferenceItem, ReferenceCollection, ReferenceDisabledReason, ReferenceEdit,
    ReferenceEffects, ReferenceFields, ReferenceIdentity, ReferenceKind, ReferenceProjection,
    ReferenceSelectionField,
};
use kasane_core::{document::ReferencePlan, Document};
#[derive(Debug, Clone)]
pub struct ReferenceEditPlan {
    version: Version,
    domain: ReferencePlan,
}
impl ReferenceEditPlan {
    pub fn effects(&self) -> &kasane_core::document::ReferenceEffects {
        self.domain.effects()
    }
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn is_noop(&self) -> bool {
        self.domain.is_noop()
    }
    pub fn affected_ids(&self) -> &[String] {
        self.domain.affected_ids()
    }
    pub fn copied_ids(&self) -> &std::collections::BTreeMap<String, String> {
        self.domain.copied_ids()
    }
}
#[derive(Debug, Clone)]
pub struct ReferenceFieldAddress {
    target: Option<ObjectHandle>,
    entity: ObjectHandle,
    owner: Option<ObjectHandle>,
    epoch: u64,
    expected: ReferenceFields,
}
impl ReferenceFieldAddress {
    pub fn id(&self) -> &str {
        self.entity.id()
    }
    pub fn expected(&self) -> &ReferenceFields {
        &self.expected
    }
}
#[derive(Debug, Clone)]
pub struct ReferenceSelectionAddress {
    targets: Vec<ObjectHandle>,
    entity: ObjectHandle,
    field: ReferenceSelectionField,
    epoch: u64,
    context: u64,
    expected: Vec<String>,
}
impl ReferenceSelectionAddress {
    pub fn owner_id(&self) -> &str {
        self.entity.id()
    }
    pub fn field(&self) -> ReferenceSelectionField {
        self.field
    }
    pub fn expected(&self) -> &[String] {
        &self.expected
    }
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReferenceCapabilities {
    pub inspect: CapabilityState,
    pub patch: CapabilityState,
    pub create_child: CapabilityState,
    pub move_object: CapabilityState,
    pub duplicate: CapabilityState,
    pub delete: CapabilityState,
    pub dissolve: CapabilityState,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReferenceCollectionCapabilities {
    pub create: CapabilityState,
    pub move_into: CapabilityState,
    pub reorder: CapabilityState,
}
pub(crate) fn kind(d: &Document, id: &str) -> Option<ObjectKind> {
    d.reference_identity(id).map(|i| match i.kind {
        ReferenceKind::ModelParameterGroup => ObjectKind::ModelParameterGroup,
        ReferenceKind::ModelParameterMember => ObjectKind::ModelParameterMember,
        ReferenceKind::ModelHitArea => ObjectKind::ModelHitArea,
        ReferenceKind::CdiCombinedSet => ObjectKind::CdiCombinedSet,
        ReferenceKind::CdiCombinedMember => ObjectKind::CdiCombinedMember,
    })
}
impl AuthoringSession {
    pub fn plan_reference_edit(
        &self,
        intent: ReferenceEdit,
    ) -> Result<ReferenceEditPlan, SdkError> {
        Ok(ReferenceEditPlan {
            version: self.version(),
            domain: self
                .document()
                .plan_reference_edit(intent)
                .map_err(|s| err(s, "plan_reference_edit"))?,
        })
    }
    pub fn execute_reference_plan(
        &mut self,
        p: &ReferenceEditPlan,
    ) -> Result<EditReceipt, SdkError> {
        let (_, r) = self.edit("Edit references", Some(p.version), |e| {
            e.apply_reference_plan(p)
        })?;
        Ok(r)
    }
    pub fn reference_collection(
        &self,
        c: &ReferenceCollection,
    ) -> Result<Vec<ObjectHandle>, SdkError> {
        self.document()
            .reference_collection(c)
            .map_err(|s| err(s, "reference_collection"))?
            .iter()
            .map(|id| self.handle(kind(self.document(), id).unwrap(), id))
            .collect()
    }
    pub fn reference_projection(&self) -> std::sync::Arc<ReferenceProjection> {
        self.document().reference_projection()
    }
    pub fn reference_capabilities(
        &self,
        h: &ObjectHandle,
    ) -> Result<ReferenceCapabilities, SdkError> {
        self.resolve_handle(h)?;
        let d = self.document();
        let i = d
            .reference_identity(h.id())
            .filter(|_| kind(d, h.id()) == Some(h.kind()))
            .ok_or_else(|| {
                SdkError::new("WRONG_REFERENCE_OBJECT", h.id(), "reference_capabilities")
            })?;
        let state = if d.reference_unlocked(h.id()).is_ok() {
            CapabilityState::Available
        } else {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        };
        let root = matches!(
            i.kind,
            ReferenceKind::ModelParameterGroup | ReferenceKind::CdiCombinedSet
        );
        Ok(ReferenceCapabilities {
            inspect: CapabilityState::Available,
            patch: if i.kind == ReferenceKind::CdiCombinedSet {
                CapabilityState::Unsupported {
                    code: "NO_REFERENCE_FIELDS",
                }
            } else {
                state.clone()
            },
            create_child: if root {
                state.clone()
            } else {
                CapabilityState::Unsupported {
                    code: "REFERENCE_HAS_NO_CHILDREN",
                }
            },
            move_object: state.clone(),
            duplicate: state.clone(),
            delete: state.clone(),
            dissolve: if root {
                state
            } else {
                CapabilityState::Unsupported {
                    code: "REFERENCE_CANNOT_DISSOLVE",
                }
            },
        })
    }
    pub fn reference_collection_capabilities(
        &self,
        c: &ReferenceCollection,
    ) -> Result<ReferenceCollectionCapabilities, SdkError> {
        self.reference_collection(c)?;
        let state = if self.document().reference_collection_unlocked(c).is_ok() {
            CapabilityState::Available
        } else {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        };
        Ok(ReferenceCollectionCapabilities {
            create: state.clone(),
            move_into: state.clone(),
            reorder: state,
        })
    }
    pub fn reference_field_address(&self, id: &str) -> Result<ReferenceFieldAddress, SdkError> {
        let d = self.document();
        let k = kind(d, id).ok_or_else(|| {
            SdkError::new("MISSING_REFERENCE_OBJECT", id, "reference_field_address")
        })?;
        let identity = d.reference_identity(id).unwrap();
        let expected = d
            .reference_fields(id)
            .map_err(|s| err(s, "reference_field_address"))?;
        let target = match &expected {
            ReferenceFields::ModelHitArea {
                mesh: kasane_core::document::ModelTargetRef::Resolved { object_id },
                ..
            } => Some(self.handle(ObjectKind::Mesh, object_id)?),
            ReferenceFields::ModelParameterMember {
                target: kasane_core::document::ModelTargetRef::Resolved { object_id },
            } => Some(self.handle(ObjectKind::Parameter, object_id)?),
            ReferenceFields::CdiCombinedMember {
                target: kasane_core::document::CdiParameterRef::Resolved { parameter_id },
            } => Some(self.handle(ObjectKind::Parameter, parameter_id)?),
            _ => None,
        };
        Ok(ReferenceFieldAddress {
            target,
            entity: self.handle(k, id)?,
            owner: identity
                .owner
                .as_deref()
                .map(|o| self.handle(kind(d, o).unwrap(), o))
                .transpose()?,
            epoch: *self.reference_epochs.get(id).unwrap_or(&0),
            expected,
        })
    }
    pub fn plan_reference_field_patch(
        &self,
        a: &ReferenceFieldAddress,
        fields: ReferenceFields,
    ) -> Result<ReferenceEditPlan, SdkError> {
        self.validate_field_address(
            std::iter::once(&a.entity).chain(&a.target).chain(&a.owner),
            || {
                *self.reference_epochs.get(a.id()).unwrap_or(&0) != a.epoch
                    || self
                        .document()
                        .reference_identity(a.id())
                        .and_then(|i| i.owner.as_deref())
                        != a.owner.as_ref().map(ObjectHandle::id)
            },
            a.id(),
            "plan_reference_field_patch",
        )?;
        self.plan_reference_edit(ReferenceEdit::Patch {
            id: a.id().into(),
            expected: a.expected.clone(),
            fields,
        })
    }
    pub fn reference_selection_address(
        &self,
        owner: &str,
        field: ReferenceSelectionField,
    ) -> Result<ReferenceSelectionAddress, SdkError> {
        let kind = match field {
            ReferenceSelectionField::MeshMasks => ObjectKind::Mesh,
            ReferenceSelectionField::OffscreenMasks => ObjectKind::Offscreen,
            ReferenceSelectionField::BlendConstraints => ObjectKind::BlendBinding,
        };
        let target_kind = if field == ReferenceSelectionField::BlendConstraints {
            ObjectKind::BlendConstraint
        } else {
            ObjectKind::Mesh
        };
        let targets = self
            .document()
            .reference_selection(owner, field)
            .map_err(|s| err(s, "reference_selection_address"))?
            .iter()
            .map(|id| self.handle(target_kind, id))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ReferenceSelectionAddress {
            targets,
            entity: self.handle(kind, owner)?,
            field,
            epoch: *self
                .selection_epochs
                .get(&(owner.into(), field))
                .unwrap_or(&0),
            context: self.selection_context(owner),
            expected: self
                .document()
                .reference_selection(owner, field)
                .map_err(|s| err(s, "reference_selection_address"))?
                .to_vec(),
        })
    }
    fn selection_context(&self, owner: &str) -> u64 {
        self.scene_field_epochs
            .get(&(owner.into(), kasane_core::document::SceneField::Context))
            .copied()
            .unwrap_or(0)
            .max(self.binding_contexts.get(owner).map_or(0, |v| v.1))
    }
    pub fn plan_reference_selection_patch(
        &self,
        a: &ReferenceSelectionAddress,
        targets: Vec<String>,
    ) -> Result<ReferenceEditPlan, SdkError> {
        self.validate_field_address(
            std::iter::once(&a.entity).chain(&a.targets),
            || {
                *self
                    .selection_epochs
                    .get(&(a.owner_id().into(), a.field))
                    .unwrap_or(&0)
                    != a.epoch
                    || self.selection_context(a.owner_id()) != a.context
            },
            a.owner_id(),
            "plan_reference_selection_patch",
        )?;
        self.plan_reference_edit(ReferenceEdit::ReplaceSelection {
            owner: a.owner_id().into(),
            field: a.field,
            expected: a.expected.clone(),
            targets,
        })
    }
}
impl EditSession<'_> {
    pub fn apply_reference_plan(&mut self, p: &ReferenceEditPlan) -> Result<(), SdkError> {
        self.ensure_plan_version(p.version, "apply_reference_plan", "Replan references")?;
        self.apply_authoring_change("apply_reference_plan", "", false, |document| {
            document.apply_reference_plan(&p.domain)
        })?;
        Ok(())
    }
    pub fn edit_references(&mut self, intent: ReferenceEdit) -> Result<(), SdkError> {
        self.ensure_active("edit_references")?;
        let p = self
            .candidate_document()
            .plan_reference_edit(intent)
            .map_err(|s| self.abort_with(err(s, "edit_references")))?;
        self.apply_authoring_change("edit_references", "", false, |document| {
            document.apply_reference_plan(&p)
        })?;
        Ok(())
    }
}
fn err(s: kasane_core::Status, op: &'static str) -> SdkError {
    SdkError::from_status(s, op, vec![])
}

//! Version-bound scene lifecycle plans and stable field addresses.
use crate::{
    AuthoringSession, CapabilityState, EditReceipt, EditSession, ObjectHandle, ObjectKind,
    SdkError, Version,
};
use kasane_core::document::{ConversionControl, ConversionReport, OrganizationEffects, ScenePlan};
pub use kasane_core::document::{
    DisabledSceneItem, SceneCollection, SceneCopyScope, SceneCreate, SceneDisabledReason,
    SceneEdit, SceneField, SceneFieldValue, ScenePairSlot, SceneProjection, SceneUse,
};
pub use kasane_core::SceneReference;
use std::collections::BTreeMap;
#[derive(Debug, Clone)]
pub struct SceneEditPlan {
    version: Version,
    domain: ScenePlan,
    control: ConversionControl,
}
impl SceneEditPlan {
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn is_noop(&self) -> bool {
        self.domain.is_noop()
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
    pub fn cost(&self) -> Option<kasane_core::document::ConversionCost> {
        self.domain.cost()
    }
    pub fn conversion_report(&self) -> Option<&ConversionReport> {
        self.domain.conversion_report()
    }
}
#[derive(Debug, Clone)]
pub struct SceneFieldAddress {
    entity: ObjectHandle,
    owner: Option<ObjectHandle>,
    owner_epoch: u64,
    context_epoch: u64,
    field_epoch: u64,
    expected: SceneFieldValue,
}
impl SceneFieldAddress {
    pub fn id(&self) -> &str {
        self.entity.id()
    }
    pub fn expected(&self) -> &SceneFieldValue {
        &self.expected
    }
}
#[derive(Debug, Clone)]
pub struct DrawingGroupAddress {
    entity: ObjectHandle,
    range: [i32; 2],
    epoch: u64,
}
impl DrawingGroupAddress {
    pub fn id(&self) -> &str {
        self.entity.id()
    }
    pub fn range(&self) -> [i32; 2] {
        self.range
    }
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct SceneCapabilities {
    pub inspect: CapabilityState,
    pub patch: CapabilityState,
    pub move_object: CapabilityState,
    pub reparent: CapabilityState,
    pub reorder: CapabilityState,
    pub duplicate: CapabilityState,
    pub delete: CapabilityState,
    pub dissolve: CapabilityState,
}
#[derive(Debug, Clone, serde::Serialize)]
pub struct SceneCollectionCapabilities {
    pub create: CapabilityState,
    pub move_into: CapabilityState,
    pub reorder: CapabilityState,
}
pub(crate) fn kind(d: &kasane_core::Document, id: &str) -> Option<ObjectKind> {
    if d.get_mesh(id).is_some() {
        Some(ObjectKind::Mesh)
    } else if d.get_transform(id).is_some() {
        Some(ObjectKind::Transform)
    } else if d.get_offscreen(id).is_some() {
        Some(ObjectKind::Offscreen)
    } else if d.get_glue(id).is_some() {
        Some(ObjectKind::Glue)
    } else {
        None
    }
}
fn error(s: kasane_core::Status, operation: &'static str) -> SdkError {
    SdkError::from_status(s, operation, vec![])
}
fn users(d: &kasane_core::Document, intent: &SceneEdit) -> Vec<String> {
    match intent {
        SceneEdit::Delete { ids, scope } => d.scene_delete_users(ids, *scope).unwrap_or_default(),
        _ => vec![],
    }
}
impl AuthoringSession {
    pub fn plan_scene_edit(&self, intent: SceneEdit) -> Result<SceneEditPlan, SdkError> {
        self.plan_scene_edit_with_control(intent, &ConversionControl::default())
    }
    pub fn plan_scene_edit_with_control(
        &self,
        intent: SceneEdit,
        control: &ConversionControl,
    ) -> Result<SceneEditPlan, SdkError> {
        let refs = users(self.document(), &intent);
        let domain = self
            .document()
            .plan_scene_edit_with_control(intent, self.authoring_limits(), control)
            .map_err(|s| {
                let mut e = error(s, "plan_scene_edit");
                if e.code.as_ref() == "OBJECT_REFERENCED" {
                    e.referrers = refs.into();
                }
                e
            })?;
        Ok(SceneEditPlan {
            version: self.version(),
            domain,
            control: control.clone(),
        })
    }
    pub fn execute_scene_plan(&mut self, plan: &SceneEditPlan) -> Result<EditReceipt, SdkError> {
        let (_, receipt) = self.edit("Edit scene", Some(plan.version), |edit| {
            edit.apply_scene_plan(plan)
        })?;
        Ok(receipt)
    }
    pub fn scene_projection(&self) -> std::sync::Arc<SceneProjection> {
        self.document().scene_projection()
    }
    pub fn scene_collection(&self, c: &SceneCollection) -> Result<Vec<ObjectHandle>, SdkError> {
        self.document()
            .scene_collection(c)
            .map_err(|s| error(s, "scene_collection"))?
            .into_iter()
            .map(|id| {
                if matches!(c, SceneCollection::DrawingGroups) {
                    self.handle(
                        ObjectKind::DrawOrderGroup,
                        if id.is_empty() {
                            self.document().id()
                        } else {
                            &id
                        },
                    )
                } else {
                    self.handle(
                        kind(self.document(), &id)
                            .or_else(|| self.document().get_part(&id).map(|_| ObjectKind::Part))
                            .unwrap(),
                        &id,
                    )
                }
            })
            .collect()
    }
    pub fn scene_collection_capabilities(
        &self,
        c: &SceneCollection,
    ) -> Result<SceneCollectionCapabilities, SdkError> {
        self.scene_collection(c)?;
        let owner = match c {
            SceneCollection::Organization { part_id } => part_id.as_deref(),
            SceneCollection::Deformation { parent_id } => parent_id.as_deref(),
            SceneCollection::Drawing { owner } => Some(owner.as_str()),
            _ => None,
        };
        let state = if owner.is_some_and(|id| self.document().organization_unlocked(id).is_err()) {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        Ok(SceneCollectionCapabilities {
            create: state.clone(),
            move_into: state.clone(),
            reorder: if matches!(c, SceneCollection::DrawingGroups) {
                CapabilityState::Unsupported {
                    code: "DRAWING_GROUP_ORDER_DERIVED",
                }
            } else {
                state
            },
        })
    }
    pub fn scene_capabilities(&self, handle: &ObjectHandle) -> Result<SceneCapabilities, SdkError> {
        self.resolve_handle(handle)?;
        let d = self.document();
        let role = handle.kind() == ObjectKind::DrawOrderGroup;
        let supported = role || kind(d, handle.id()) == Some(handle.kind());
        let root = role && handle.id() == d.id();
        let state = if !supported {
            CapabilityState::Unsupported {
                code: "WRONG_OBJECT_KIND",
            }
        } else if if role {
            d.drawing_group_unlocked(if root { "" } else { handle.id() })
                .is_err()
        } else {
            d.scene_lifecycle_unlocked(&[handle.id().into()], SceneCopyScope::Selected)
                .is_err()
        } {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        let container = matches!(handle.kind(), ObjectKind::Mesh | ObjectKind::Transform);
        let delete = if root {
            CapabilityState::Unsupported {
                code: "SYSTEM_DRAWING_ROOT",
            }
        } else if role && d.offscreen_for_part(handle.id()).is_some() {
            CapabilityState::Unavailable {
                code: "DRAWING_GROUP_HAS_OFFSCREEN",
            }
        } else if !role
            && !d
                .scene_delete_users(&[handle.id().into()], SceneCopyScope::Selected)
                .unwrap_or_default()
                .is_empty()
        {
            CapabilityState::Unavailable {
                code: "OBJECT_REFERENCED",
            }
        } else {
            state.clone()
        };
        Ok(SceneCapabilities {
            inspect: CapabilityState::Available,
            patch: state.clone(),
            move_object: state.clone(),
            reparent: if container {
                state.clone()
            } else {
                CapabilityState::Unsupported {
                    code: "NO_DEFORMATION_PARENT",
                }
            },
            reorder: state.clone(),
            duplicate: state.clone(),
            delete: delete.clone(),
            dissolve: if role {
                delete
            } else {
                CapabilityState::Unsupported {
                    code: "NOT_DRAWING_GROUP",
                }
            },
        })
    }
    pub fn scene_field_address(
        &self,
        id: &str,
        field: SceneField,
    ) -> Result<SceneFieldAddress, SdkError> {
        let d = self.document();
        let entity = self.handle(
            kind(d, id)
                .ok_or_else(|| SdkError::new("MISSING_SCENE_OBJECT", id, "scene_field_address"))?,
            id,
        )?;
        let expected = d
            .scene_field(id, field)
            .map_err(|s| error(s, "scene_field_address"))?;
        let owner = d
            .organization_owner(id)
            .map(|id| self.handle(ObjectKind::Part, id))
            .transpose()?;
        Ok(SceneFieldAddress {
            entity,
            owner,
            owner_epoch: self.organization_contexts.get(id).map_or(0, |v| v.1),
            context_epoch: self
                .scene_field_epochs
                .get(&(id.into(), SceneField::Context))
                .copied()
                .unwrap_or(0),
            field_epoch: self
                .scene_field_epochs
                .get(&(id.into(), field))
                .copied()
                .unwrap_or(0),
            expected,
        })
    }
    pub fn plan_scene_field_patch(
        &self,
        address: &SceneFieldAddress,
        value: SceneFieldValue,
        clear_glue_pairs: bool,
    ) -> Result<SceneEditPlan, SdkError> {
        let id = address.id();
        self.validate_field_address(
            std::iter::once(&address.entity).chain(&address.owner),
            || {
                self.document().organization_owner(id)
                    != address.owner.as_ref().map(ObjectHandle::id)
                    || self.organization_contexts.get(id).map_or(0, |v| v.1) != address.owner_epoch
                    || self
                        .scene_field_epochs
                        .get(&(id.into(), SceneField::Context))
                        .copied()
                        .unwrap_or(0)
                        != address.context_epoch
                    || self
                        .scene_field_epochs
                        .get(&(id.into(), address.expected.field()))
                        .copied()
                        .unwrap_or(0)
                        != address.field_epoch
            },
            "Capture a new scene field address",
            "plan_scene_field_patch",
        )?;
        self.plan_scene_edit(SceneEdit::Patch {
            id: id.into(),
            expected: address.expected.clone(),
            value,
            clear_glue_pairs,
        })
    }
    pub fn drawing_group_address(&self, owner: &str) -> Result<DrawingGroupAddress, SdkError> {
        let owner = if owner == self.document().id() {
            ""
        } else {
            owner
        };
        let id = if owner.is_empty() {
            self.document().id()
        } else {
            owner
        };
        let entity = self.handle(ObjectKind::DrawOrderGroup, id)?;
        let groups = self
            .document()
            .draw_order_groups()
            .map(|g| g.to_vec())
            .unwrap_or_else(|| kasane_core::draw_order::resolved_groups(self.document()));
        let group = groups
            .into_iter()
            .find(|g| g.owner == owner)
            .ok_or_else(|| {
                SdkError::new("MISSING_DRAWING_GROUP", owner, "drawing_group_address")
            })?;
        Ok(DrawingGroupAddress {
            entity,
            range: [group.min_order, group.max_order],
            epoch: self
                .scene_field_epochs
                .get(&(id.into(), SceneField::Context))
                .copied()
                .unwrap_or(0),
        })
    }
    pub fn plan_drawing_range_patch(
        &self,
        address: &DrawingGroupAddress,
        range: [i32; 2],
    ) -> Result<SceneEditPlan, SdkError> {
        self.validate_field_address(
            [&address.entity],
            || {
                self.scene_field_epochs
                    .get(&(address.id().into(), SceneField::Context))
                    .copied()
                    .unwrap_or(0)
                    != address.epoch
            },
            "Drawing group membership or range changed",
            "plan_drawing_range_patch",
        )?;
        self.plan_scene_edit(SceneEdit::SetDrawingRange {
            owner: if address.id() == self.document().id() {
                String::new()
            } else {
                address.id().into()
            },
            expected: address.range,
            range,
        })
    }
}
impl EditSession<'_> {
    pub fn apply_scene_plan(&mut self, plan: &SceneEditPlan) -> Result<(), SdkError> {
        self.ensure_plan_version(plan.version, "apply_scene_plan", "Replan scene edit")?;
        plan.control
            .check()
            .map_err(|s| self.abort_with(error(s, "apply_scene_plan")))?;
        if let Some(cost) = plan.domain.cost() {
            crate::binding_authoring::check_cost(cost, self.authoring_limits)
                .map_err(|e| self.abort_with(e))?;
        }
        if let Some(cost) = plan.domain.cost() {
            self.binding_cost = Some(cost);
        }
        self.apply_authoring_change("apply_scene_plan", false, |document| {
            document.apply_scene_plan(&plan.domain)
        })?;
        self.binding_controls.push(plan.control.clone());
        Ok(())
    }
    pub fn edit_scene(&mut self, intent: SceneEdit) -> Result<(), SdkError> {
        self.ensure_active("edit_scene")?;
        let refs = users(self.candidate_document(), &intent);
        let control = ConversionControl::default();
        let domain = self
            .candidate_document()
            .plan_scene_edit_with_control(intent, self.authoring_limits, &control)
            .map_err(|s| {
                let mut e = error(s, "edit_scene");
                if e.code.as_ref() == "OBJECT_REFERENCED" {
                    e.referrers = refs.into();
                }
                self.abort_with(e)
            })?;
        if let Some(cost) = domain.cost() {
            self.binding_cost = Some(cost);
        }
        self.apply_authoring_change("edit_scene", false, |document| {
            document.apply_scene_plan(&domain)
        })?;
        self.binding_controls.push(control);
        Ok(())
    }
}

//! Snapshot-bound content plans and monotonic topology addresses.
use crate::{
    AuthoringSession, CapabilityState, EditReceipt, EditSession, ObjectHandle, ObjectKind,
    SdkError, Version,
};
use zipzi_core::document::{ConversionControl, SceneContentPlan};
pub use zipzi_core::document::{
    NewMeshVertex, OffscreenDeletePolicy, OffscreenMappingPatch, PartKeyformSlot,
    SceneContentDomain, SceneContentEdit, SceneContentReport, SceneContentTarget,
    SceneContentValue, ShapeTarget, VertexContentPatch, VertexDeletePolicy, VertexSeed,
    VertexWeight, WarpPointPatch,
};
#[derive(Debug, Clone)]
pub struct SceneContentEditPlan {
    version: Version,
    domain: SceneContentPlan,
    control: ConversionControl,
}
impl SceneContentEditPlan {
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn is_noop(&self) -> bool {
        self.domain.is_noop()
    }
    pub fn affected_ids(&self) -> &[String] {
        self.domain.affected_ids()
    }
    pub fn report(&self) -> &SceneContentReport {
        self.domain.report()
    }
    pub fn cost(&self) -> zipzi_core::document::ConversionCost {
        self.domain.cost()
    }
}
#[derive(Debug, Clone)]
pub struct SceneContentAddress {
    owner: ObjectHandle,
    dependencies: Vec<ObjectHandle>,
    epoch: u64,
    context: u64,
    target: SceneContentTarget,
    expected: SceneContentValue,
}
impl SceneContentAddress {
    pub fn owner_id(&self) -> &str {
        self.owner.id()
    }
    pub fn target(&self) -> &SceneContentTarget {
        &self.target
    }
    pub fn expected(&self) -> &SceneContentValue {
        &self.expected
    }
}
#[derive(Debug, Clone, serde::Serialize, zipzi_api_derive::ApiShape)]
pub struct SceneContentCapabilities {
    pub inspect: CapabilityState,
    pub edit: CapabilityState,
    pub topology: CapabilityState,
}
impl AuthoringSession {
    pub fn plan_scene_content(
        &self,
        intent: SceneContentEdit,
    ) -> Result<SceneContentEditPlan, SdkError> {
        self.plan_scene_content_with_control(intent, &ConversionControl::default())
    }
    pub fn plan_scene_content_with_control(
        &self,
        intent: SceneContentEdit,
        control: &ConversionControl,
    ) -> Result<SceneContentEditPlan, SdkError> {
        let domain = self
            .document()
            .plan_scene_content_with_control(intent, self.authoring_limits, control)
            .map_err(|s| error(s, "plan_scene_content"))?;
        Ok(SceneContentEditPlan {
            version: self.version(),
            domain,
            control: control.clone(),
        })
    }
    pub fn execute_scene_content_plan(
        &mut self,
        plan: &SceneContentEditPlan,
    ) -> Result<EditReceipt, SdkError> {
        let (_, r) = self.edit("Edit scene content", Some(plan.version), |e| {
            e.apply_scene_content_plan(plan)
        })?;
        Ok(r)
    }
    pub fn scene_content_capabilities(
        &self,
        owner: &ObjectHandle,
    ) -> Result<SceneContentCapabilities, SdkError> {
        self.resolve_handle(owner)?;
        let d = self.document();
        let supported = matches!(
            owner.kind(),
            ObjectKind::Mesh
                | ObjectKind::Glue
                | ObjectKind::GlueVertexPair
                | ObjectKind::Offscreen
        ) || (owner.kind() == ObjectKind::Transform
            && d.get_transform(owner.id()).and_then(|t| t.warp()).is_some());
        let state = if !supported {
            CapabilityState::Unsupported {
                code: "WRONG_CONTENT_OWNER",
            }
        } else if d
            .scene_lifecycle_unlocked(
                &[d.glue_pair_owner(owner.id()).unwrap_or(owner.id()).into()],
                zipzi_core::document::SceneCopyScope::Selected,
            )
            .is_err()
        {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        Ok(SceneContentCapabilities {
            inspect: if supported {
                CapabilityState::Available
            } else {
                state.clone()
            },
            edit: state.clone(),
            topology: state,
        })
    }
    pub fn scene_content_address(
        &self,
        owner_id: &str,
        target: SceneContentTarget,
    ) -> Result<SceneContentAddress, SdkError> {
        let kind = match target.domain() {
            SceneContentDomain::MeshTopology => ObjectKind::Mesh,
            SceneContentDomain::WarpTopology => ObjectKind::Transform,
            SceneContentDomain::GluePairs => ObjectKind::Glue,
            SceneContentDomain::OffscreenKeyforms => ObjectKind::Offscreen,
        };
        let owner = self.handle(kind, owner_id)?;
        let expected = self
            .document()
            .scene_content_value(owner_id, &target)
            .map_err(|s| error(s, "scene_content_address"))?;
        let mut dependencies = Vec::new();
        let mut capture = |kind, id: &str| -> Result<(), SdkError> {
            dependencies.push(self.handle(kind, id)?);
            Ok(())
        };
        let shape = match &target {
            SceneContentTarget::VertexPosition { shape, .. }
            | SceneContentTarget::WarpPoint { shape, .. }
            | SceneContentTarget::WarpBezierControl { shape, .. } => Some(shape),
            _ => None,
        };
        match shape {
            Some(ShapeTarget::Ordinary {
                binding_id,
                samples,
            }) => {
                capture(
                    if kind == ObjectKind::Mesh {
                        ObjectKind::MeshBinding
                    } else {
                        ObjectKind::SceneBinding
                    },
                    binding_id,
                )?;
                for sample in samples {
                    capture(ObjectKind::BindingSample, sample)?;
                }
            }
            Some(ShapeTarget::Blend {
                binding_id,
                sample_id,
            }) => {
                capture(ObjectKind::BlendBinding, binding_id)?;
                capture(ObjectKind::BindingSample, sample_id)?;
            }
            _ => {}
        }
        match &target {
            SceneContentTarget::GluePair { pair_id } => {
                capture(ObjectKind::GlueVertexPair, pair_id)?;
            }
            SceneContentTarget::OffscreenMapping {
                slot: PartKeyformSlot::Samples { samples },
            } => {
                let offscreen = self.document().get_offscreen(owner_id).unwrap();
                let binding = self
                    .document()
                    .binding_for_scene(&offscreen.part_id)
                    .unwrap();
                capture(ObjectKind::SceneBinding, &binding.id)?;
                for sample in samples {
                    capture(ObjectKind::BindingSample, sample)?;
                }
            }
            _ => {}
        }
        Ok(SceneContentAddress {
            context: self.content_context(owner_id),
            epoch: *self
                .content_epochs
                .get(&(owner_id.into(), target.domain()))
                .unwrap_or(&0),
            owner,
            dependencies,
            target,
            expected,
        })
    }
    fn content_context(&self, owner: &str) -> u64 {
        self.scene_field_epochs
            .get(&(owner.into(), zipzi_core::document::SceneField::Context))
            .copied()
            .unwrap_or(0)
    }
    pub fn plan_scene_content_patch(
        &self,
        address: &SceneContentAddress,
        value: SceneContentValue,
    ) -> Result<SceneContentEditPlan, SdkError> {
        self.resolve_handle(&address.owner)?;
        // Final values can be identical after delete/recreate in one edit.
        // Referenced shape identities must still belong to their captured lifetime.
        for dependency in &address.dependencies {
            self.resolve_handle(dependency)?;
        }
        if *self
            .content_epochs
            .get(&(address.owner_id().into(), address.target.domain()))
            .unwrap_or(&0)
            != address.epoch
            || self.content_context(address.owner_id()) != address.context
            || self
                .document()
                .scene_content_value(address.owner_id(), &address.target)
                .ok()
                .as_ref()
                != Some(&address.expected)
        {
            return Err(SdkError::new(
                "STALE_CONTENT_ADDRESS",
                "Capture a new content address",
                "plan_scene_content_patch",
            ));
        }
        let edit = self
            .document()
            .scene_content_patch(address.owner_id(), &address.target, value)
            .map_err(|s| error(s, "plan_scene_content_patch"))?;
        self.plan_scene_content(edit)
    }
    pub fn glue_pairs(&self, owner: &str) -> Result<Vec<ObjectHandle>, SdkError> {
        let g = self
            .document()
            .get_glue(owner)
            .ok_or_else(|| SdkError::new("MISSING_GLUE", owner, "glue_pairs"))?;
        g.pairs
            .iter()
            .map(|p| self.handle(ObjectKind::GlueVertexPair, &p.id))
            .collect()
    }
}
impl EditSession<'_> {
    pub fn apply_scene_content_plan(
        &mut self,
        plan: &SceneContentEditPlan,
    ) -> Result<(), SdkError> {
        self.ensure_plan_version(
            plan.version,
            "apply_scene_content_plan",
            "Replan scene content",
        )?;
        plan.control
            .check()
            .map_err(|s| self.abort_with(error(s, "apply_scene_content_plan")))?;
        crate::binding_authoring::check_cost(plan.cost(), self.authoring_limits)
            .map_err(|e| self.abort_with(e))?;
        self.apply_authoring_change("apply_scene_content_plan", "", false, |document| {
            document.apply_scene_content_plan(&plan.domain)
        })?;
        self.binding_controls.push(plan.control.clone());
        self.binding_cost = Some(plan.cost());
        Ok(())
    }
    pub fn edit_scene_content(&mut self, intent: SceneContentEdit) -> Result<(), SdkError> {
        self.ensure_active("edit_scene_content")?;
        let domain = self
            .candidate_document()
            .plan_scene_content_with_control(
                intent,
                self.authoring_limits,
                &ConversionControl::default(),
            )
            .map_err(|s| self.abort_with(error(s, "edit_scene_content")))?;
        self.apply_authoring_change("edit_scene_content", "", false, |document| {
            document.apply_scene_content_plan(&domain)
        })?;
        self.binding_cost = Some(domain.cost());
        Ok(())
    }
}
fn error(s: zipzi_core::Status, op: &'static str) -> SdkError {
    let mut e = SdkError::from_status(s, op, vec![]);
    if matches!(e.code.as_ref(), "VERTEX_REFERENCED" | "KEYFORM_REFERENCED") {
        e.referrers = e
            .message
            .split(", ")
            .map(str::to_owned)
            .collect::<Vec<_>>()
            .into_boxed_slice();
    }
    e
}

impl crate::AuthoringSnapshot {
    pub fn plan_scene_content_with_control(
        &self,
        intent: SceneContentEdit,
        limits: crate::AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<SceneContentEditPlan, SdkError> {
        Ok(SceneContentEditPlan {
            version: self.version(),
            domain: self
                .document()
                .plan_scene_content_with_control(intent, limits, control)
                .map_err(|s| error(s, "plan_scene_content"))?,
            control: control.clone(),
        })
    }
}

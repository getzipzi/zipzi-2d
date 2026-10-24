//! Scene creation defaults and independent organization/deformation positions.
//! The SDK validates owners, locks and object data and publishes one transaction.
use super::moving::PaletteProjection;
use super::*;
use zipzi_sdk::{
    OrganizationCollection, OrganizationEdit, SceneCollection, SceneCreate, SceneEdit,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneObjectType {
    Part,
    Mesh,
    Warp,
    Rotation,
    Offscreen,
}
impl SceneObjectType {
    pub fn object_kind(self) -> ObjectKind {
        match self {
            Self::Part => ObjectKind::Part,
            Self::Mesh => ObjectKind::Mesh,
            Self::Warp | Self::Rotation => ObjectKind::Deformer,
            Self::Offscreen => ObjectKind::Offscreen,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SceneObjectCreationPlan {
    projection: PaletteProjection,
    plan: CreationPlan,
}
impl SceneObjectCreationPlan {
    pub fn projection(&self) -> PaletteProjection {
        self.projection
    }
}
#[derive(Clone, Debug)]
enum CreationPlan {
    Organization(zipzi_sdk::OrganizationEditPlan),
    Scene(zipzi_sdk::SceneEditPlan),
}

impl Document {
    /// Lightweight menu availability. Query native destination locks and
    /// Offscreen validation without allocating identities or building plans.
    /// Candidates are settled by the host before the final creation planner.
    pub fn validate_scene_object_creation_context(
        &self,
        kind: SceneObjectType,
        selection: &[ObjectRef],
    ) -> anyhow::Result<()> {
        self.validate_scene_object_creation_in(PaletteProjection::Organization, kind, selection)
    }

    pub fn validate_scene_object_creation_in(
        &self,
        projection: PaletteProjection,
        kind: SceneObjectType,
        selection: &[ObjectRef],
    ) -> anyhow::Result<()> {
        let (part_id, parent_id) = match projection {
            PaletteProjection::Organization => (self.scene_creation_container(selection)?.0, None),
            PaletteProjection::Deformation => {
                anyhow::ensure!(
                    matches!(
                        kind,
                        SceneObjectType::Mesh | SceneObjectType::Warp | SceneObjectType::Rotation
                    ),
                    AuthoringPlanningError::new("NO_DEFORMATION_COLLECTION")
                );
                let (part, parent, _) = self.deformation_creation_container(selection)?;
                (part, parent)
            }
            PaletteProjection::Parameters => {
                anyhow::bail!(AuthoringPlanningError::new("NO_SCENE_COLLECTION"))
            }
        };
        let state = self
            .session
            .organization_collection_capabilities(&OrganizationCollection::PartMembers {
                part_id: part_id.clone(),
            })?
            .create;
        if let zipzi_sdk::CapabilityState::Unavailable { code }
        | zipzi_sdk::CapabilityState::Unsupported { code } = state
        {
            anyhow::bail!(AuthoringPlanningError::new(code));
        }
        if projection == PaletteProjection::Deformation {
            let state = self
                .session
                .scene_collection_capabilities(&SceneCollection::Deformation { parent_id })?
                .create;
            if let zipzi_sdk::CapabilityState::Unavailable { code }
            | zipzi_sdk::CapabilityState::Unsupported { code } = state
            {
                anyhow::bail!(AuthoringPlanningError::new(code));
            }
        }
        if kind == SceneObjectType::Offscreen {
            let part_id = part_id.ok_or_else(|| {
                anyhow::anyhow!(AuthoringPlanningError::new("OFFSCREEN_REQUIRES_PART"))
            })?;
            // The document UUID cannot be another scene object's identity.
            // Validation uses native defaults, as the final SceneCreate does.
            let status = self.model().validate_offscreen(&zipzi_core::Offscreen {
                id: self.model().id().to_owned(),
                part_id,
                ..Default::default()
            });
            if !status.is_ok() {
                return Err(AuthoringPlanningError::from_status(status).into());
            }
        }
        Ok(())
    }

    /// Capture a location in the organization projection. This deliberately does
    /// not assign a deformation parent: the two tree projections are independent.
    pub fn plan_scene_object_creation(
        &self,
        kind: SceneObjectType,
        name: String,
        selection: &[ObjectRef],
    ) -> anyhow::Result<SceneObjectCreationPlan> {
        self.plan_scene_object_creation_in(PaletteProjection::Organization, kind, name, selection)
    }

    pub fn plan_scene_object_creation_in(
        &self,
        projection: PaletteProjection,
        kind: SceneObjectType,
        name: String,
        selection: &[ObjectRef],
    ) -> anyhow::Result<SceneObjectCreationPlan> {
        self.ensure_authoring_idle()?;
        let (part_id, parent_id, before, deformation_before) = match projection {
            PaletteProjection::Organization => {
                let (part, before) = self.scene_creation_location(kind, selection)?;
                (part, None, before, None)
            }
            PaletteProjection::Deformation => {
                anyhow::ensure!(
                    matches!(
                        kind,
                        SceneObjectType::Mesh | SceneObjectType::Warp | SceneObjectType::Rotation
                    ),
                    AuthoringPlanningError::new("NO_DEFORMATION_COLLECTION")
                );
                let (part, parent, after) = self.deformation_creation_container(selection)?;
                let before = if after {
                    let members = self
                        .session
                        .scene_collection(&SceneCollection::Deformation {
                            parent_id: parent.clone(),
                        })?;
                    let members: Vec<_> = members
                        .iter()
                        .filter(|object| {
                            parent.is_some()
                                || (object.kind() == zipzi_sdk::ObjectKind::Transform)
                                    == (kind != SceneObjectType::Mesh)
                        })
                        .collect();
                    let selected: std::collections::HashSet<_> =
                        selection.iter().map(|object| object.id.as_str()).collect();
                    members
                        .iter()
                        .rposition(|object| selected.contains(object.id()))
                        .and_then(|i| members.get(i + 1))
                        .map(|object| object.id().to_owned())
                } else {
                    None
                };
                (part, parent, None, before)
            }
            PaletteProjection::Parameters => {
                anyhow::bail!(AuthoringPlanningError::new("NO_SCENE_COLLECTION"))
            }
        };
        let name = self.scene_creation_name(name, part_id.as_deref())?;
        let id = Uuid::new_v4().to_string();
        Ok(SceneObjectCreationPlan {
            projection,
            plan: if kind == SceneObjectType::Part {
                CreationPlan::Organization(self.plan_organization_authoring(
                    OrganizationEdit::CreatePart {
                        part: zipzi_core::Part {
                            id,
                            name,
                            parent_id: part_id.unwrap_or_default(),
                            ..Default::default()
                        },
                        before,
                    },
                )?)
            } else {
                CreationPlan::Scene(self.plan_scene_authoring(SceneEdit::Create {
                    id,
                    name,
                    object: match kind {
                        SceneObjectType::Mesh => SceneCreate::Mesh {
                            texture: Default::default(),
                        },
                        SceneObjectType::Warp => SceneCreate::Warp { bounds: None },
                        SceneObjectType::Rotation => SceneCreate::Rotation,
                        SceneObjectType::Offscreen => SceneCreate::Offscreen,
                        SceneObjectType::Part => unreachable!(),
                    },
                    part_id,
                    parent_id,
                    before,
                    deformation_before,
                })?)
            },
        })
    }

    fn deformation_creation_container(
        &self,
        selection: &[ObjectRef],
    ) -> anyhow::Result<(Option<String>, Option<String>, bool)> {
        let mut parents = Vec::new();
        let mut owners = Vec::new();
        for object in selection {
            anyhow::ensure!(
                matches!(object.kind, ObjectKind::Mesh | ObjectKind::Deformer),
                AuthoringPlanningError::new("EXPECTED_DEFORMATION_MEMBERS")
            );
            self.organization_authoring_capabilities(object)?;
            let owner = self
                .model()
                .organization_owner(&object.id)
                .map(str::to_owned);
            if selection.len() == 1 && object.kind == ObjectKind::Deformer {
                return Ok((owner, Some(object.id.clone()), false));
            }
            let parent = self
                .model()
                .scene_deformer_parent(&object.id)
                .ok_or_else(|| {
                    anyhow::anyhow!(AuthoringPlanningError::new("EXPECTED_DEFORMATION_MEMBERS"))
                })?;
            parents.push((!parent.is_empty()).then(|| parent.to_owned()));
            owners.push(owner);
        }
        let Some(parent) = parents.first().cloned() else {
            return Ok((None, None, false));
        };
        if parents.iter().any(|other| *other != parent) {
            return Ok((None, None, false));
        }
        let owner = owners.first().cloned().flatten();
        let owner = if owners.iter().all(|other| *other == owner) {
            owner
        } else {
            None
        };
        Ok((owner, parent, true))
    }

    fn scene_creation_name(&self, name: String, owner: Option<&str>) -> anyhow::Result<String> {
        let name = name.trim();
        anyhow::ensure!(!name.is_empty(), "Object name is required");
        let index = self.object_index();
        let names: std::collections::HashSet<_> = index
            .objects
            .iter()
            .filter(|info| {
                matches!(
                    info.reference.kind,
                    ObjectKind::Part
                        | ObjectKind::Mesh
                        | ObjectKind::Deformer
                        | ObjectKind::Offscreen
                ) && self.model().organization_owner(&info.reference.id) == owner
            })
            .map(|info| info.name.as_str())
            .collect();
        if !names.contains(name) {
            return Ok(name.to_owned());
        }
        for suffix in 2.. {
            let candidate = format!("{name} {suffix}");
            if !names.contains(candidate.as_str()) {
                return Ok(candidate);
            }
        }
        unreachable!()
    }

    // `after_selection` distinguishes sibling insertion from container append.
    // Availability only needs the owner; collection order is queried on creation.
    fn scene_creation_container(
        &self,
        selection: &[ObjectRef],
    ) -> anyhow::Result<(Option<String>, bool)> {
        let mut owners = Vec::new();
        for object in selection {
            anyhow::ensure!(
                matches!(
                    object.kind,
                    ObjectKind::Part
                        | ObjectKind::Mesh
                        | ObjectKind::Deformer
                        | ObjectKind::Offscreen
                ),
                "Expected organization tree members"
            );
            self.organization_authoring_capabilities(object)?;
            if selection.len() == 1 && object.kind == ObjectKind::Part {
                return Ok((Some(object.id.clone()), false));
            }
            owners.push(
                self.model()
                    .organization_owner(&object.id)
                    .map(str::to_owned),
            );
        }
        let Some(owner) = owners.first().cloned() else {
            return Ok((None, false));
        };
        if owners.iter().any(|candidate| *candidate != owner) {
            return Ok((None, false));
        }
        Ok((owner, true))
    }

    fn scene_creation_location(
        &self,
        kind: SceneObjectType,
        selection: &[ObjectRef],
    ) -> anyhow::Result<(Option<String>, Option<String>)> {
        let (owner, after_selection) = self.scene_creation_container(selection)?;
        if !after_selection {
            return Ok((owner, None));
        }
        let order = self
            .session
            .organization_collection(OrganizationCollection::PartMembers {
                part_id: owner.clone(),
            })?;
        // The root projection displays Parts and unowned scene objects in
        // separate branches; each branch retains the SDK's complete order.
        let order: Vec<_> = order
            .iter()
            .filter(|handle| {
                owner.is_some()
                    || (handle.kind() == zipzi_sdk::ObjectKind::Part)
                        == (kind == SceneObjectType::Part)
            })
            .collect();
        let selected: std::collections::HashSet<_> =
            selection.iter().map(|object| object.id.as_str()).collect();
        let before = order
            .iter()
            .rposition(|handle| selected.contains(handle.id()))
            .and_then(|index| order.get(index + 1))
            .map(|handle| handle.id().to_owned());
        Ok((owner, before))
    }

    pub fn execute_scene_object_creation(
        &mut self,
        plan: &SceneObjectCreationPlan,
    ) -> anyhow::Result<OperationResult> {
        match &plan.plan {
            CreationPlan::Organization(plan) => self.execute_organization_authoring(plan),
            CreationPlan::Scene(plan) => self.execute_scene_authoring(plan),
        }
    }
}

impl crate::Editor {
    pub fn execute_scene_object_creation(
        &mut self,
        document: crate::DocumentId,
        plan: &SceneObjectCreationPlan,
    ) -> anyhow::Result<OperationResult> {
        self.ensure_authoring_target(document, "Scene creation")?;
        anyhow::ensure!(
            self.editing_context() == Some(crate::EditingContext::Modeling),
            "Modeling context changed"
        );
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_scene_object_creation(plan))
    }
}

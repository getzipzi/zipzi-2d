//! Organization-tree creation defaults and full-collection insertion anchors.
//! The SDK validates owners, locks and object data and publishes one transaction.
use super::*;
use kasane_sdk::{OrganizationCollection, OrganizationEdit, SceneCreate, SceneEdit};

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
pub struct SceneObjectCreationPlan(CreationPlan);
#[derive(Clone, Debug)]
enum CreationPlan {
    Organization(kasane_sdk::OrganizationEditPlan),
    Scene(kasane_sdk::SceneEditPlan),
}

impl Document {
    /// Capture a location in the organization projection. This deliberately does
    /// not assign a deformation parent: the two tree projections are independent.
    pub fn plan_scene_object_creation(
        &self,
        kind: SceneObjectType,
        name: String,
        selection: &[ObjectRef],
    ) -> anyhow::Result<SceneObjectCreationPlan> {
        self.ensure_authoring_idle()?;
        let (part_id, before) = self.scene_creation_location(kind, selection)?;
        let name = self.scene_creation_name(name, part_id.as_deref())?;
        let id = Uuid::new_v4().to_string();
        Ok(SceneObjectCreationPlan(if kind == SceneObjectType::Part {
            CreationPlan::Organization(self.plan_organization_authoring(
                OrganizationEdit::CreatePart {
                    part: kasane_core::Part {
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
                parent_id: None,
                before,
            })?)
        }))
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

    fn scene_creation_location(
        &self,
        kind: SceneObjectType,
        selection: &[ObjectRef],
    ) -> anyhow::Result<(Option<String>, Option<String>)> {
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
                return Ok((Some(object.id.clone()), None));
            }
            owners.push(
                self.model()
                    .organization_owner(&object.id)
                    .map(str::to_owned),
            );
        }
        let Some(owner) = owners.first().cloned() else {
            return Ok((None, None));
        };
        if owners.iter().any(|candidate| *candidate != owner) {
            return Ok((None, None));
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
                    || (handle.kind() == kasane_sdk::ObjectKind::Part)
                        == (kind == SceneObjectType::Part)
            })
            .collect();
        let before = order
            .iter()
            .rposition(|handle| selection.iter().any(|object| object.id == handle.id()))
            .and_then(|index| order.get(index + 1))
            .map(|handle| handle.id().to_owned());
        Ok((owner, before))
    }

    pub fn execute_scene_object_creation(
        &mut self,
        plan: &SceneObjectCreationPlan,
    ) -> anyhow::Result<OperationResult> {
        match &plan.0 {
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

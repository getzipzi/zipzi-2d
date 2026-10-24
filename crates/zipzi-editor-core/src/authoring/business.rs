//! Physics and Pose defaults and placement are resolved by stable identity.
use super::*;
use anyhow::{Result, ensure};
use zipzi_core::document::PhysicsEdit;
use zipzi_sdk::{CapabilityState as C, PoseCollection as PC, PoseEdit};

#[derive(Clone, Debug)]
pub enum BusinessAction {
    Create(ObjectKind, Option<ObjectRef>),
    Copy(ObjectRef),
    Delete(ObjectRef),
    Step(ObjectRef, bool),
    Move(ObjectRef, String, Option<String>),
    MigratePoseGroup(ObjectRef, String),
}
impl BusinessAction {
    pub fn source(&self) -> Option<&ObjectRef> {
        match self {
            Self::Create(_, at) => at.as_ref(),
            Self::Copy(o)
            | Self::Delete(o)
            | Self::Step(o, _)
            | Self::Move(o, ..)
            | Self::MigratePoseGroup(o, _) => Some(o),
        }
    }
}
#[derive(Clone, Debug)]
enum Native {
    Noop,
    Pose(zipzi_sdk::PoseEditPlan),
    Physics(zipzi_sdk::PhysicsEditPlan),
}
impl Native {
    fn apply(&self, edit: &mut zipzi_sdk::EditSession<'_>) -> Result<()> {
        match self {
            Self::Noop => {}
            Self::Pose(p) => edit.apply_pose_plan(p)?,
            Self::Physics(p) => edit.apply_physics_plan(p)?,
        };
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct BusinessPlan {
    native: Native,
    version: zipzi_sdk::Version,
    pub removed: Vec<ObjectRef>,
    pub moved: Vec<ObjectRef>,
    pub fallback: Vec<ObjectRef>,
    pub created_kind: Option<ObjectKind>,
    pub noop: bool,
}
#[derive(Clone, Debug)]
pub struct BusinessDestination {
    pub id: String,
    pub label: String,
    pub nodes: Vec<(String, String)>,
}
fn inventory(model: &zipzi_core::Document) -> Vec<ObjectRef> {
    use ObjectKind as K;
    let mut objects = vec![];
    let mut add = |kind, id: &str| objects.push(ObjectRef::new(kind, id));
    if let Some(p) = model.pose() {
        add(K::Pose, &p.id);
        for g in &p.groups {
            add(K::PoseGroup, &g.id);
            for e in &g.entries {
                add(K::PoseEntry, &e.id);
                for l in e.links.iter().flatten() {
                    add(K::PoseLink, &l.id);
                }
            }
        }
    }
    if let Some(p) = model.physics() {
        add(K::Physics, &p.id);
        for g in &p.data.settings {
            add(K::PhysicsGroup, &g.id);
            for i in &g.inputs {
                add(K::PhysicsInput, &i.id);
            }
            for o in &g.outputs {
                add(K::PhysicsOutput, &o.id);
            }
            for n in &g.vertices {
                add(K::PhysicsNode, &n.id);
            }
        }
    }
    objects
}
impl Document {
    pub fn business_owner(&self, object: &ObjectRef) -> Option<ObjectRef> {
        let index = self.object_index();
        index
            .incoming(object)
            .find(|r| r.kind == crate::RelationKind::Contains)
            .map(|r| r.source.clone())
    }
    fn pose_collection_for(&self, object: &ObjectRef) -> Result<PC> {
        Ok(match object.kind {
            ObjectKind::PoseGroup => PC::Groups,
            ObjectKind::PoseEntry => PC::Entries {
                group_id: self
                    .model()
                    .get_pose_entry(&object.id)
                    .ok_or_else(|| anyhow::anyhow!("Missing Pose entry"))?
                    .0
                    .id
                    .clone(),
            },
            ObjectKind::PoseLink => PC::Links {
                entry_id: self
                    .model()
                    .get_pose_link(&object.id)
                    .ok_or_else(|| anyhow::anyhow!("Missing Pose link"))?
                    .0
                    .id
                    .clone(),
            },
            _ => anyhow::bail!("Expected Pose member"),
        })
    }
    fn pose_order(&self, collection: &PC) -> Result<Vec<String>> {
        Ok(self
            .session
            .pose_collection(collection.clone())?
            .into_iter()
            .map(|h| h.id().to_owned())
            .collect())
    }
    fn pose_creation_collection(&self, kind: ObjectKind, at: Option<&ObjectRef>) -> Result<PC> {
        use ObjectKind as K;
        let owner = match (kind, at) {
            (K::PoseGroup, _) => return Ok(PC::Groups),
            (K::PoseEntry, Some(o)) if o.kind == K::PoseGroup => o.id.clone(),
            (K::PoseEntry, Some(o)) if o.kind == K::PoseEntry => self
                .model()
                .get_pose_entry(&o.id)
                .ok_or_else(|| anyhow::anyhow!("Missing member"))?
                .0
                .id
                .clone(),
            (K::PoseEntry, Some(o)) if o.kind == K::PoseLink => {
                let entry = self
                    .model()
                    .get_pose_link(&o.id)
                    .ok_or_else(|| anyhow::anyhow!("Missing link"))?
                    .0;
                self.model().get_pose_entry(&entry.id).unwrap().0.id.clone()
            }
            (K::PoseLink, Some(o)) if o.kind == K::PoseEntry => o.id.clone(),
            (K::PoseLink, Some(o)) if o.kind == K::PoseLink => self
                .model()
                .get_pose_link(&o.id)
                .ok_or_else(|| anyhow::anyhow!("Missing link"))?
                .0
                .id
                .clone(),
            _ => return Err(AuthoringPlanningError::new("MISSING_POSE_OWNER").into()),
        };
        Ok(if kind == K::PoseEntry {
            PC::Entries { group_id: owner }
        } else {
            PC::Links { entry_id: owner }
        })
    }
    pub fn business_capability(&self, action: &BusinessAction) -> Result<C> {
        use BusinessAction as A;
        use ObjectKind as K;
        match action {
            A::Create(kind, at) => {
                let collection = match kind {
                    K::PhysicsGroup => {
                        AuthoringCollection::Physics(zipzi_sdk::PhysicsCollection::Groups)
                    }
                    K::PoseGroup | K::PoseEntry | K::PoseLink => AuthoringCollection::Pose(
                        self.pose_creation_collection(*kind, at.as_ref())?,
                    ),
                    _ => {
                        return Ok(C::Unsupported {
                            code: "WRONG_OBJECT_KIND",
                        });
                    }
                };
                Ok(self
                    .authoring_collection_capabilities(&collection)?
                    .domain
                    .create)
            }
            _ => {
                let source = action.source().unwrap();
                let kind = match action {
                    A::Copy(_) => AuthoringAction::Duplicate,
                    A::Delete(_) => AuthoringAction::Delete,
                    A::MigratePoseGroup(..) => AuthoringAction::Dissolve,
                    A::Step(..) => AuthoringAction::Reorder,
                    _ => AuthoringAction::Move,
                };
                let state = self.authoring_capabilities(source)?.state(kind);
                if state == C::Available {
                    if let A::Step(o, down) = action {
                        let order = self.business_order(o)?;
                        let index = order
                            .iter()
                            .position(|id| id == &o.id)
                            .ok_or_else(|| anyhow::anyhow!("Missing member"))?;
                        let minimum = usize::from(o.kind == K::PhysicsNode);
                        if (!down && index <= minimum) || (*down && index + 1 == order.len()) {
                            return Ok(C::Unavailable { code: "NO_CHANGE" });
                        }
                    }
                }
                Ok(state)
            }
        }
    }
    fn business_order(&self, object: &ObjectRef) -> Result<Vec<String>> {
        use ObjectKind as K;
        use zipzi_sdk::PhysicsCollection as P;
        if matches!(object.kind, K::PoseGroup | K::PoseEntry | K::PoseLink) {
            return self.pose_order(&self.pose_collection_for(object)?);
        }
        let group = self
            .business_owner(object)
            .map(|o| o.id)
            .unwrap_or_default();
        let collection = match object.kind {
            K::PhysicsGroup => P::Groups,
            K::PhysicsInput => P::Inputs { group_id: group },
            K::PhysicsOutput => P::Outputs { group_id: group },
            K::PhysicsNode => P::Nodes { group_id: group },
            _ => anyhow::bail!("Not an ordered business object"),
        };
        Ok(self
            .session
            .physics_collection(collection)?
            .iter()
            .map(|h| h.id().to_owned())
            .collect())
    }
    pub fn business_destinations(
        &self,
        source: &ObjectRef,
        migrate: bool,
    ) -> Result<Vec<BusinessDestination>> {
        use ObjectKind as K;
        let model = self.model();
        let mut result = vec![];
        match source.kind {
            K::PoseEntry | K::PoseGroup => {
                ensure!(
                    source.kind != K::PoseGroup || migrate,
                    "Groups use execution order"
                );
                for g in &model
                    .pose()
                    .ok_or_else(|| anyhow::anyhow!("Missing Pose"))?
                    .groups
                {
                    if g.id != source.id {
                        result.push(BusinessDestination {
                            id: g.id.clone(),
                            label: format!("{} · {}", g.name, g.id),
                            nodes: vec![],
                        });
                    }
                }
            }
            K::PoseLink => {
                for g in &model.pose().unwrap().groups {
                    for e in &g.entries {
                        result.push(BusinessDestination {
                            id: e.id.clone(),
                            label: format!(
                                "{} / {} · {}",
                                g.name,
                                self.object_index()
                                    .name(&ObjectRef::new(K::PoseEntry, &e.id)),
                                e.id
                            ),
                            nodes: vec![],
                        });
                    }
                }
            }
            K::PhysicsInput | K::PhysicsOutput => {
                for g in &model.physics().unwrap().data.settings {
                    result.push(BusinessDestination {
                        id: g.id.clone(),
                        label: format!("{} · {}", model.physics().unwrap().group_name(&g.id), g.id),
                        nodes: g
                            .vertices
                            .iter()
                            .enumerate()
                            .skip(1)
                            .map(|(i, n)| (n.id.clone(), i.to_string()))
                            .collect(),
                    });
                }
            }
            _ => {}
        }
        Ok(result)
    }
    pub fn plan_business(&self, action: &BusinessAction, name: String) -> Result<BusinessPlan> {
        use BusinessAction as A;
        use ObjectKind as K;
        self.ensure_authoring_idle()?;
        let source = action.source();
        let model = self.model();
        let existing: Vec<&str> = match action {
            A::Create(K::PoseGroup, _) => model
                .pose()
                .into_iter()
                .flat_map(|p| &p.groups)
                .map(|g| g.name.as_str())
                .collect(),
            A::Create(K::PhysicsGroup, _) => model
                .physics()
                .into_iter()
                .flat_map(|p| &p.data.settings)
                .map(|g| model.physics().unwrap().group_name(&g.id))
                .collect(),
            _ => vec![],
        };
        let name = std::iter::once(name.clone())
            .chain((2..).map(|i| format!("{name} {i}")))
            .find(|s| !existing.contains(&s.as_str()))
            .unwrap();

        let mut created_kind = None;
        let next = |order: Vec<String>, id: &str| {
            order
                .iter()
                .position(|s| s == id)
                .and_then(|i| order.get(i + 1))
                .cloned()
        };
        let native = match action {
            A::Create(kind, at) if matches!(kind, K::PoseGroup | K::PoseEntry | K::PoseLink) => {
                created_kind = Some(*kind);
                let collection = self.pose_creation_collection(*kind, at.as_ref())?;
                let before = at
                    .as_ref()
                    .filter(|o| o.kind == *kind)
                    .and_then(|o| next(self.pose_order(&collection).ok()?, &o.id));
                let id = Uuid::new_v4().to_string();
                let intent = match collection {
                    PC::Groups => PoseEdit::CreateGroup {
                        pose_id: model
                            .pose()
                            .map_or_else(|| Uuid::new_v4().to_string(), |p| p.id.clone()),
                        id,
                        name,
                        before,
                    },
                    PC::Entries { group_id } => PoseEdit::CreateEntry {
                        group_id,
                        id,
                        before,
                    },
                    PC::Links { entry_id } => PoseEdit::CreateLink {
                        entry_id,
                        id,
                        before,
                    },
                };
                Native::Pose(self.plan_pose_authoring(intent)?)
            }
            A::Create(K::PhysicsGroup, _) => {
                created_kind = Some(K::PhysicsGroup);
                Native::Physics(self.plan_physics_authoring(self.physics_group_creation(name))?)
            }
            A::Copy(o) | A::Step(o, _)
                if matches!(o.kind, K::PoseGroup | K::PoseEntry | K::PoseLink) =>
            {
                let collection = self.pose_collection_for(o)?;
                let order = self.pose_order(&collection)?;
                let before = if let A::Step(_, down) = action {
                    let i = order
                        .iter()
                        .position(|s| s == &o.id)
                        .ok_or_else(|| anyhow::anyhow!("Missing Pose object"))?;
                    if (!down && i == 0) || (*down && i + 1 == order.len()) {
                        Some(o.id.clone())
                    } else {
                        order.get(if *down { i + 2 } else { i - 1 }).cloned()
                    }
                } else {
                    next(order, &o.id)
                };
                if before.as_deref() == Some(o.id.as_str()) {
                    Native::Noop
                } else {
                    Native::Pose(self.plan_pose_authoring(if matches!(action, A::Copy(_)) {
                        created_kind = Some(o.kind);
                        PoseEdit::Duplicate {
                            ids: vec![o.id.clone()],
                            destination: collection,
                            before,
                        }
                    } else {
                        PoseEdit::Move {
                            ids: vec![o.id.clone()],
                            destination: collection,
                            before,
                        }
                    })?)
                }
            }
            A::Delete(o)
                if matches!(o.kind, K::Pose | K::PoseGroup | K::PoseEntry | K::PoseLink) =>
            {
                Native::Pose(self.plan_pose_authoring(PoseEdit::Delete {
                    ids: vec![o.id.clone()],
                })?)
            }
            A::Move(o, destination, _) if matches!(o.kind, K::PoseEntry | K::PoseLink) => {
                Native::Pose(self.plan_pose_authoring(PoseEdit::Move {
                    ids: vec![o.id.clone()],
                    destination: if o.kind == K::PoseEntry {
                        PC::Entries {
                            group_id: destination.clone(),
                        }
                    } else {
                        PC::Links {
                            entry_id: destination.clone(),
                        }
                    },
                    before: None,
                })?)
            }
            A::MigratePoseGroup(o, destination) => {
                Native::Pose(self.plan_pose_authoring(PoseEdit::DissolveGroup {
                    id: o.id.clone(),
                    destination: destination.clone(),
                    before: None,
                })?)
            }
            A::Copy(o) if o.kind == K::PhysicsNode => {
                created_kind = Some(K::PhysicsNode);
                let before = next(self.business_order(o)?, &o.id);
                Native::Physics(self.plan_physics_authoring(PhysicsEdit::DuplicateNode {
                    id: o.id.clone(),
                    before,
                })?)
            }
            A::Step(o, down) if o.kind == K::PhysicsNode => {
                let order = self.business_order(o)?;
                let at = order
                    .iter()
                    .position(|id| id == &o.id)
                    .ok_or_else(|| anyhow::anyhow!("Missing node"))?;
                if (!down && at <= 1) || (*down && at + 1 == order.len()) {
                    Native::Noop
                } else {
                    let before = order.get(if *down { at + 2 } else { at - 1 }).cloned();
                    Native::Physics(self.plan_physics_authoring(PhysicsEdit::MoveNode {
                        id: o.id.clone(),
                        before,
                    })?)
                }
            }
            A::Copy(o)
                if matches!(o.kind, K::PhysicsGroup | K::PhysicsInput | K::PhysicsOutput) =>
            {
                created_kind = Some(o.kind);
                let owner = self
                    .business_owner(o)
                    .filter(|p| p.kind == K::PhysicsGroup)
                    .map(|p| p.id);
                Native::Physics(self.plan_physics_authoring(PhysicsEdit::Duplicate {
                    ids: vec![o.id.clone()],
                    group_id: owner,
                    before: None,
                })?)
            }
            A::Delete(o) => Native::Physics(self.plan_physics_authoring(PhysicsEdit::Delete {
                ids: vec![o.id.clone()],
                node_replacements: Default::default(),
            })?),
            A::Step(o, down) => {
                let intent = if o.kind == K::PhysicsGroup {
                    self.physics_group_step(&o.id, *down)?
                } else {
                    let group = self
                        .business_owner(o)
                        .ok_or_else(|| anyhow::anyhow!("Missing Physics owner"))?;
                    self.physics_channel_edit(
                        &group.id,
                        &crate::PhysicsChannelAction::Step {
                            object: o.clone(),
                            down: *down,
                        },
                    )?
                };
                match intent {
                    Some(intent) => Native::Physics(self.plan_physics_authoring(intent)?),
                    None => Native::Noop,
                }
            }
            A::Move(o, group, node) => {
                let ids = vec![o.id.clone()];
                let before = None;
                let intent = if o.kind == K::PhysicsInput {
                    PhysicsEdit::MoveInputs {
                        ids,
                        group_id: group.clone(),
                        before,
                    }
                } else {
                    ensure!(o.kind == K::PhysicsOutput, "Expected Physics channel");
                    let rig = model
                        .physics()
                        .unwrap()
                        .data
                        .settings
                        .iter()
                        .find(|g| g.outputs.iter().any(|v| v.id == o.id))
                        .ok_or_else(|| anyhow::anyhow!("Missing output"))?;
                    let output = rig.outputs.iter().find(|v| v.id == o.id).unwrap();
                    let replacements = if rig.id != *group {
                        [(
                            output.id.clone(),
                            node.clone().ok_or_else(|| {
                                AuthoringPlanningError::new("CHOOSE_PHYSICS_NODE")
                            })?,
                        )]
                        .into()
                    } else {
                        Default::default()
                    };
                    PhysicsEdit::MoveOutputs {
                        ids,
                        group_id: group.clone(),
                        before,
                        node_replacements: replacements,
                    }
                };
                Native::Physics(self.plan_physics_authoring(intent)?)
            }
            _ => anyhow::bail!("Unsupported business action"),
        };
        let mut candidate = self
            .session
            .begin_owned_edit("Review Physics or Pose", None)?;
        native.apply(&mut candidate)?;
        let before = inventory(model);
        let after: std::collections::HashSet<_> = inventory(candidate.candidate_document())
            .into_iter()
            .collect();
        let removed = before.into_iter().filter(|o| !after.contains(o)).collect();
        let moved = if let A::MigratePoseGroup(o, _) = action {
            model
                .get_pose_group(&o.id)
                .unwrap()
                .entries
                .iter()
                .map(|e| ObjectRef::new(K::PoseEntry, &e.id))
                .collect()
        } else {
            vec![]
        };
        let fallback = source
            .map(|o| {
                std::iter::once(o.clone())
                    .chain(self.authoring_neighbors(o))
                    .collect()
            })
            .unwrap_or_default();
        Ok(BusinessPlan {
            native,
            version: self.session.version(),
            removed,
            moved,
            fallback,
            created_kind,
            noop: candidate.candidate_document().revision() == model.revision(),
        })
    }
    pub fn execute_business(&mut self, plan: &BusinessPlan) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let mut edit = self
            .session
            .begin_owned_edit("Edit Physics or Pose", Some(plan.version))?;
        plan.native.apply(&mut edit)?;
        let receipt = edit.commit_to(&mut self.session)?;
        if receipt.changed {
            self.pristine_blank = false;
        }
        self.consume_authoring_receipt(&receipt, false);
        Ok(self.authoring_result(receipt))
    }
}
impl crate::Editor {
    pub fn execute_business(
        &mut self,
        document: crate::DocumentId,
        plan: &BusinessPlan,
    ) -> Result<OperationResult> {
        self.ensure_animation_target(document)?;
        let result = self
            .active_session_mut()
            .unwrap()
            .execute_authoring(|doc| doc.execute_business(plan))?;
        if self
            .document()
            .is_some_and(|d| d.physics_definition().is_none())
            && self.animation().is_some_and(|a| a.physics_preview.manual)
        {
            self.set_physics_manual(document, false)?;
        }
        Ok(result)
    }
}

impl crate::CommandId {
    pub fn business_creation_kind(self) -> Option<ObjectKind> {
        use crate::CommandId as I;
        Some(match self {
            I::CreatePhysicsGroup => ObjectKind::PhysicsGroup,
            I::CreatePoseGroup => ObjectKind::PoseGroup,
            I::CreatePoseEntry => ObjectKind::PoseEntry,
            I::CreatePoseLink => ObjectKind::PoseLink,
            _ => return None,
        })
    }
}

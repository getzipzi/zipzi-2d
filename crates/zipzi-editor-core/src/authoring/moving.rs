//! Stable palette destinations adapted to native collection plans. The SDK owns
//! ordering, subtree normalization, locks, cycles and atomic publication.
use super::*;
use anyhow::{Result, anyhow, ensure};
use zipzi_sdk::{
    OrganizationCollection as Collection, OrganizationEdit, ReparentEdit, ReparentPlacement,
    ReparentStrategy, SceneCollection, SceneEdit,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaletteProjection {
    Organization,
    Parameters,
    Deformation,
}

/// Lightweight destination metadata; a host plans only the chosen destination.
/// `None` denotes the projection root and has an empty, host-localized path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteMoveDestination {
    pub object: Option<ObjectRef>,
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PalettePosition {
    Before,
    After,
    Inside,
}

#[derive(Clone, Debug)]
pub struct PaletteMove {
    pub projection: PaletteProjection,
    pub sources: Vec<ObjectRef>,
    /// None is the root of the source object's collection.
    pub target: Option<ObjectRef>,
    pub position: PalettePosition,
}

#[derive(Clone, Debug)]
pub struct PaletteMovePlan(pub(super) MovePlan);
#[derive(Clone, Debug)]
pub(super) enum MovePlan {
    Organization(zipzi_sdk::OrganizationEditPlan),
    Scene(zipzi_sdk::SceneEditPlan),
    Reparent {
        plan: zipzi_sdk::SceneEditPlan,
        preview_revision: u64,
    },
    Steps(super::stepping::PaletteStepsPlan),
}
impl PaletteMovePlan {
    pub(crate) fn apply_to(
        &self,
        edit: &mut zipzi_sdk::EditSession<'_>,
        revision: u64,
    ) -> Result<()> {
        match &self.0 {
            MovePlan::Scene(plan) => Ok(edit.apply_scene_plan(plan)?),
            MovePlan::Reparent {
                plan,
                preview_revision,
            } => {
                ensure!(
                    revision == *preview_revision,
                    "Frozen parameter values changed; reselect the parent"
                );
                Ok(edit.apply_scene_plan(plan)?)
            }
            _ => anyhow::bail!("Expected a deformation parent plan"),
        }
    }
    pub fn is_noop(&self) -> bool {
        match &self.0 {
            MovePlan::Organization(plan) => plan.is_noop(),
            MovePlan::Scene(plan) => plan.is_noop(),
            MovePlan::Reparent { plan, .. } => plan.is_noop(),
            MovePlan::Steps(plan) => plan.is_noop(),
        }
    }
}

impl Document {
    fn palette_deformation_reparent(&self, request: &PaletteMove) -> Result<Option<ReparentEdit>> {
        if request.projection != PaletteProjection::Deformation
            || request.sources.is_empty()
            || !request
                .sources
                .iter()
                .all(|o| matches!(o.kind, ObjectKind::Mesh | ObjectKind::Deformer))
        {
            return Ok(None);
        }
        let (SceneCollection::Deformation { parent_id }, before) =
            super::reparenting::deformation_destination(&self.model(), request)?
        else {
            unreachable!()
        };
        let rotation = request
            .sources
            .iter()
            .any(|o| o.kind == ObjectKind::Deformer);
        let mut context = self.preservation_context();
        if rotation {
            context.parameter_domain.clear();
        }
        Ok(Some(ReparentEdit {
            sources: request.sources.iter().map(|o| o.id.clone()).collect(),
            parent_id: parent_id.unwrap_or_default(),
            strategy: if rotation {
                ReparentStrategy::PreserveEditState
            } else {
                ReparentStrategy::PreserveParameterDomain
            },
            placement: Some(ReparentPlacement { before }),
            tolerance: 0.01,
            relative_tolerance: 1e-6,
            context: Some(context),
        }))
    }

    /// Tree hover checks structural support without converting every keyform.
    /// The complete numeric conversion is planned once when the move is applied.
    /// Deformer subtrees that cannot preserve their shape are rejected here too.
    pub fn palette_deformation_move_support(
        &self,
        request: &PaletteMove,
    ) -> Result<Option<zipzi_sdk::ReparentSupport>> {
        let Some(edit) = self.palette_deformation_reparent(request)? else {
            return Ok(None);
        };
        self.ensure_authoring_idle()?;
        for object in request.sources.iter().chain(request.target.iter()) {
            self.organization_authoring_capabilities(object)?;
        }
        Ok(Some(self.session.reparent_support(&edit)?))
    }

    pub fn palette_move_destinations(
        &self,
        projection: PaletteProjection,
    ) -> Result<Vec<PaletteMoveDestination>> {
        let index = self.object_index();
        let kind = if projection == PaletteProjection::Organization {
            ObjectKind::Part
        } else if projection == PaletteProjection::Deformation {
            ObjectKind::Deformer
        } else {
            ObjectKind::ParameterGroup
        };
        let mut result = Vec::new();
        let mut pending: Vec<(Option<ObjectRef>, String)> = vec![(None, String::new())];
        let mut seen = std::collections::HashSet::new();
        while let Some((object, path)) = pending.pop() {
            let owner = object.as_ref().map(|object| object.id.clone());
            let collection = if projection == PaletteProjection::Organization {
                Collection::PartMembers { part_id: owner }
            } else {
                Collection::ParameterGroups { parent_id: owner }
            };
            result.push(PaletteMoveDestination {
                object: object.clone(),
                path: path.clone(),
            });
            let mut children = Vec::new();
            let handles = if projection == PaletteProjection::Deformation {
                self.session
                    .scene_collection(&SceneCollection::Deformation {
                        parent_id: object.as_ref().map(|o| o.id.clone()),
                    })?
            } else {
                self.session.organization_collection(collection)?
            };
            for handle in handles {
                if (kind == ObjectKind::Part && handle.kind() != zipzi_sdk::ObjectKind::Part)
                    || (kind == ObjectKind::Deformer
                        && handle.kind() != zipzi_sdk::ObjectKind::Transform)
                    || !seen.insert(handle.id().to_owned())
                {
                    continue;
                }
                let object = ObjectRef::new(kind, handle.id());
                let name = index.name(&object);
                let child_path = if path.is_empty() {
                    name.to_owned()
                } else {
                    format!("{path} / {name}")
                };
                children.push((Some(object), child_path));
            }
            // Preorder in native sibling order, without recursion or cloning
            // the scene tree and its non-container objects.
            pending.extend(children.into_iter().rev());
        }
        Ok(result)
    }

    pub fn plan_palette_move(&self, request: PaletteMove) -> Result<PaletteMovePlan> {
        self.ensure_authoring_idle()?;
        let first = request
            .sources
            .first()
            .ok_or_else(|| anyhow!("Select an object first"))?;
        for object in request.sources.iter().chain(request.target.iter()) {
            // Resolve the typed identity rather than trusting the panel row.
            self.organization_authoring_capabilities(object)?;
        }
        let ids: Vec<_> = request
            .sources
            .iter()
            .map(|object| object.id.clone())
            .collect();
        if request.projection == PaletteProjection::Deformation {
            ensure!(
                request
                    .sources
                    .iter()
                    .all(|object| matches!(object.kind, ObjectKind::Mesh | ObjectKind::Deformer)),
                "Select meshes or deformers"
            );
            let (collection, before) =
                super::reparenting::deformation_destination(&self.model(), &request)?;
            let SceneCollection::Deformation { ref parent_id } = collection else {
                unreachable!()
            };
            if request.sources.iter().any(|object| {
                self.model().scene_deformer_parent(&object.id)
                    != Some(parent_id.as_deref().unwrap_or(""))
            }) {
                // Moving a Mesh or a whole Deformer changes its coordinate space.
                // An unsupported subtree must not bypass conversion by falling
                // back to unchanged local coordinates.
                let mut edit = self
                    .palette_deformation_reparent(&request)?
                    .ok_or_else(|| anyhow!("Select meshes or deformers"))?;
                edit.placement = Some(ReparentPlacement { before });
                let frozen = edit.strategy == ReparentStrategy::PreserveEditState;
                let plan = self.plan_scene_authoring(SceneEdit::Reparent { edit })?;
                return Ok(PaletteMovePlan(if frozen {
                    MovePlan::Reparent {
                        plan,
                        preview_revision: self.preview_revision(),
                    }
                } else {
                    MovePlan::Scene(plan)
                }));
            }
            return Ok(PaletteMovePlan(MovePlan::Scene(
                self.plan_scene_authoring(SceneEdit::Move {
                    ids,
                    destination: collection,
                    before,
                })?,
            )));
        }
        let collection = match request.projection {
            PaletteProjection::Parameters => {
                ensure!(
                    request
                        .sources
                        .iter()
                        .all(|object| object.kind == first.kind),
                    "Select members of one parameter collection"
                );
                let owner = match (&request.target, request.position) {
                    (Some(target), PalettePosition::Inside) => {
                        ensure!(
                            target.kind == ObjectKind::ParameterGroup,
                            "Target does not contain parameters or folders"
                        );
                        Some(target.id.clone())
                    }
                    (Some(target), _) => {
                        ensure!(
                            target.kind == first.kind,
                            "Folders and parameters have separate orders"
                        );
                        self.model()
                            .organization_owner(&target.id)
                            .map(str::to_owned)
                    }
                    (None, _) => None,
                };
                match first.kind {
                    ObjectKind::Parameter => Collection::Parameters { group_id: owner },
                    ObjectKind::ParameterGroup => Collection::ParameterGroups { parent_id: owner },
                    _ => anyhow::bail!("Expected a parameter or folder"),
                }
            }
            PaletteProjection::Organization => {
                ensure!(
                    request.sources.iter().all(|object| matches!(
                        object.kind,
                        ObjectKind::Part
                            | ObjectKind::Mesh
                            | ObjectKind::Deformer
                            | ObjectKind::Offscreen
                    )),
                    "Expected organization members"
                );
                let part_id = match (&request.target, request.position) {
                    (Some(target), PalettePosition::Inside) => {
                        ensure!(target.kind == ObjectKind::Part, "Target is not a Part");
                        Some(target.id.clone())
                    }
                    (Some(target), _) => self
                        .model()
                        .organization_owner(&target.id)
                        .map(str::to_owned),
                    (None, _) => None,
                };
                Collection::PartMembers { part_id }
            }
            PaletteProjection::Deformation => unreachable!(),
        };
        let before = if let Some(target) = &request.target {
            if request.position == PalettePosition::Inside {
                None
            } else {
                let mut members: Vec<_> = self
                    .session
                    .organization_collection(collection.clone())?
                    .into_iter()
                    .map(|handle| handle.id().to_owned())
                    .collect();
                if matches!(collection, Collection::PartMembers { part_id: None }) {
                    members.retain(|id| {
                        self.model().get_part(id).is_some() == (target.kind == ObjectKind::Part)
                    });
                }
                insertion(&members, &ids, target, request.position)?
            }
        } else {
            None
        };
        Ok(PaletteMovePlan(MovePlan::Organization(
            self.plan_organization_authoring(OrganizationEdit::Move {
                ids,
                destination: collection,
                before,
            })?,
        )))
    }

    pub fn execute_palette_move(&mut self, plan: &PaletteMovePlan) -> Result<OperationResult> {
        match &plan.0 {
            MovePlan::Organization(plan) => self.execute_organization_authoring(plan),
            MovePlan::Scene(plan) => self.execute_scene_authoring(plan),
            MovePlan::Reparent {
                plan,
                preview_revision,
            } => {
                ensure!(
                    *preview_revision == self.preview_revision(),
                    AuthoringPlanningError::new("STALE_PLAN")
                );
                self.execute_scene_authoring(plan)
            }
            MovePlan::Steps(plan) => self.execute_palette_steps(plan),
        }
    }

    /// Resolve a keyboard/menu step against the full native collection, even if
    /// siblings are currently hidden by a filter or a collapsed ancestor.
    pub fn plan_palette_step(
        &self,
        projection: PaletteProjection,
        object: ObjectRef,
        down: bool,
    ) -> Result<PaletteMovePlan> {
        self.plan_palette_steps(projection, std::slice::from_ref(&object), down)
    }
}

pub(super) fn insertion(
    members: &[String],
    sources: &[String],
    target: &ObjectRef,
    position: PalettePosition,
) -> Result<Option<String>> {
    if position == PalettePosition::Before || sources.contains(&target.id) {
        return Ok(Some(target.id.clone()));
    }
    let at = members
        .iter()
        .position(|id| id == &target.id)
        .ok_or_else(|| anyhow!("Target no longer exists"))?;
    Ok(members[at + 1..]
        .iter()
        .find(|id| !sources.contains(id))
        .cloned())
}

impl crate::Editor {
    pub fn execute_palette_move(
        &mut self,
        document: crate::DocumentId,
        plan: &PaletteMovePlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_target(document, "Palette move")?;
        anyhow::ensure!(
            self.editing_context() == Some(crate::EditingContext::Modeling),
            "Modeling context changed"
        );
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_palette_move(plan))
    }
}

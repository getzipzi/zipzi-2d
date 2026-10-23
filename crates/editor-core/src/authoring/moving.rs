//! Stable palette destinations adapted to native collection plans. The SDK owns
//! ordering, subtree normalization, locks, cycles and atomic publication.
use super::*;
use anyhow::{Result, anyhow, ensure};
use kasane_sdk::{
    OrganizationCollection as Collection, OrganizationEdit, SceneCollection, SceneEdit,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaletteProjection {
    Organization,
    Parameters,
    Deformation,
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
pub struct PaletteMovePlan(MovePlan);
#[derive(Clone, Debug)]
enum MovePlan {
    Organization(kasane_sdk::OrganizationEditPlan),
    Scene(kasane_sdk::SceneEditPlan),
}
impl PaletteMovePlan {
    pub fn is_noop(&self) -> bool {
        match &self.0 {
            MovePlan::Organization(plan) => plan.is_noop(),
            MovePlan::Scene(plan) => plan.is_noop(),
        }
    }
}

impl Document {
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
                request.position != PalettePosition::Inside,
                "Deformation parent changes require a preservation plan"
            );
            let target = request
                .target
                .as_ref()
                .ok_or_else(|| anyhow!("Choose a deformation sibling"))?;
            let parent = self
                .model()
                .scene_deformer_parent(&target.id)
                .ok_or_else(|| anyhow!("Target has no deformation collection"))?;
            let collection = SceneCollection::Deformation {
                parent_id: (!parent.is_empty()).then(|| parent.to_owned()),
            };
            let members = self
                .model()
                .scene_collection(&collection)
                .map_err(|status| anyhow!("{}: {}", status.code, status.message))?;
            let members: Vec<String> = members
                .into_iter()
                .filter(|id| {
                    !parent.is_empty()
                        || self.model().get_transform(id).is_some()
                            == self.model().get_transform(&target.id).is_some()
                })
                .collect();
            let before = insertion(&members, &ids, target, request.position)?;
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
        let members = match projection {
            PaletteProjection::Deformation => {
                let parent = self
                    .model()
                    .scene_deformer_parent(&object.id)
                    .ok_or_else(|| anyhow!("Object has no deformation collection"))?;
                self.model()
                    .scene_collection(&SceneCollection::Deformation {
                        parent_id: (!parent.is_empty()).then(|| parent.to_owned()),
                    })
                    .map_err(|status| anyhow!("{}: {}", status.code, status.message))?
                    .into_iter()
                    .filter(|id| {
                        !parent.is_empty()
                            || self.model().get_transform(id).is_some()
                                == (object.kind == ObjectKind::Deformer)
                    })
                    .collect()
            }
            _ => {
                let owner = self
                    .model()
                    .organization_owner(&object.id)
                    .map(str::to_owned);
                let collection = match projection {
                    PaletteProjection::Organization => Collection::PartMembers {
                        part_id: owner.clone(),
                    },
                    _ if object.kind == ObjectKind::Parameter => Collection::Parameters {
                        group_id: owner.clone(),
                    },
                    _ => Collection::ParameterGroups {
                        parent_id: owner.clone(),
                    },
                };
                self.session
                    .organization_collection(collection)?
                    .into_iter()
                    .filter(|handle| {
                        projection != PaletteProjection::Organization
                            || owner.is_some()
                            || (handle.kind() == kasane_sdk::ObjectKind::Part)
                                == (object.kind == ObjectKind::Part)
                    })
                    .map(|handle| handle.id().to_owned())
                    .collect::<Vec<_>>()
            }
        };
        let index = members
            .iter()
            .position(|id| id == &object.id)
            .ok_or_else(|| anyhow!("Object no longer exists"))?;
        let next = if down {
            index.checked_add(1).filter(|index| *index < members.len())
        } else {
            index.checked_sub(1)
        };
        let target = next.map_or_else(
            || object.clone(),
            |index| ObjectRef::new(object.kind, &members[index]),
        );
        // Organization rows may mix types. Resolve the actual anchor kind.
        let target = self
            .object_index()
            .objects
            .iter()
            .find(|info| info.reference.id == target.id)
            .map(|info| info.reference.clone())
            .ok_or_else(|| anyhow!("Target no longer exists"))?;
        self.plan_palette_move(PaletteMove {
            projection,
            sources: vec![object],
            target: Some(target),
            position: if down {
                PalettePosition::After
            } else {
                PalettePosition::Before
            },
        })
    }
}

fn insertion(
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

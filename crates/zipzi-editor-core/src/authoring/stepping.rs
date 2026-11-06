//! Palette step semantics over complete native collections. A read-only COW
//! candidate validates all blocks; execution composes native edits in one SDK
//! transaction. Domain locks, source closure and order validation stay native.
use super::moving::{MovePlan, PaletteMovePlan, PaletteProjection};
use super::*;
use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};
use zipzi_contract::document::OrganizationCollection as Collection;
use zipzi_contract::document::OrganizationEdit;
use zipzi_contract::document::SceneCollection;
use zipzi_contract::document::SceneEdit;

#[derive(Debug)]
pub struct PaletteMovePlanningError(pub(super) zipzi_core::Status);
impl PaletteMovePlanningError {
    pub fn code(&self) -> &str {
        &self.0.code
    }
}
impl std::fmt::Display for PaletteMovePlanningError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.0.code, self.0.message)
    }
}
impl std::error::Error for PaletteMovePlanningError {}
fn failure(status: zipzi_core::Status) -> anyhow::Error {
    PaletteMovePlanningError(status).into()
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum StepCollection {
    Organization {
        owner: Option<String>,
        root_parts: bool,
    },
    Parameters(Option<String>),
    Folders(Option<String>),
    Deformation {
        parent: Option<String>,
        root_deformers: bool,
    },
}
impl StepCollection {
    fn organization(&self) -> Collection {
        match self {
            Self::Organization { owner, .. } => Collection::PartMembers {
                part_id: owner.clone(),
            },
            Self::Parameters(owner) => Collection::Parameters {
                group_id: owner.clone(),
            },
            Self::Folders(owner) => Collection::ParameterGroups {
                parent_id: owner.clone(),
            },
            Self::Deformation { .. } => unreachable!(),
        }
    }
    fn scene(&self) -> SceneCollection {
        match self {
            Self::Deformation { parent, .. } => SceneCollection::Deformation {
                parent_id: parent.clone(),
            },
            _ => unreachable!(),
        }
    }
    fn members(&self, model: &zipzi_core::state::DocumentState) -> Result<Vec<String>> {
        Ok(match self {
            Self::Deformation {
                parent,
                root_deformers,
            } => model
                .scene_collection(&self.scene())
                .map_err(failure)?
                .into_iter()
                .filter(|id| parent.is_some() || model.transform(id).is_some() == *root_deformers)
                .collect(),
            Self::Organization { owner, root_parts } => model
                .organization_collection(&self.organization())
                .map_err(failure)?
                .into_iter()
                .filter(|id| owner.is_some() || model.part(id).is_some() == *root_parts)
                .collect(),
            _ => model
                .organization_collection(&self.organization())
                .map_err(failure)?,
        })
    }
    fn intent(&self, ids: Vec<String>, before: Option<String>) -> StepEdit {
        if matches!(self, Self::Deformation { .. }) {
            StepEdit::Scene(SceneEdit::Move {
                ids,
                destination: self.scene(),
                before,
            })
        } else {
            StepEdit::Organization(OrganizationEdit::Move {
                ids,
                destination: self.organization(),
                before,
            })
        }
    }
}

#[derive(Clone, Debug)]
enum StepEdit {
    Organization(OrganizationEdit),
    Scene(SceneEdit),
}
impl StepEdit {
    fn apply(&self, candidate: &mut crate::runtime_host::HostedDraft) -> Result<()> {
        candidate.batch(|commands| match self {
            Self::Organization(intent) => commands.edit_organization(intent.clone()).map(|_| ()),
            Self::Scene(intent) => commands
                .edit_scene(intent.clone(), Default::default(), &Default::default())
                .map(|_| ()),
        })?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(super) struct PaletteStepsPlan {
    preview: crate::EditPreview<()>,
}
impl PaletteStepsPlan {
    pub(super) fn version(&self) -> zipzi_sdk::Version {
        self.preview.version()
    }
    pub(super) fn is_noop(&self) -> bool {
        self.preview.is_noop()
    }
}

impl Document {
    pub fn plan_palette_steps(
        &self,
        projection: PaletteProjection,
        objects: &[ObjectRef],
        down: bool,
    ) -> Result<PaletteMovePlan> {
        self.ensure_authoring_idle()?;
        let first = objects
            .first()
            .ok_or_else(|| anyhow!("Select an object first"))?;
        for object in objects {
            self.organization_authoring_capabilities(object)?;
        }
        anyhow::ensure!(
            objects.iter().all(|object| match projection {
                PaletteProjection::Organization => matches!(
                    object.kind,
                    ObjectKind::Part
                        | ObjectKind::Mesh
                        | ObjectKind::Deformer
                        | ObjectKind::Offscreen
                ),
                PaletteProjection::Parameters => matches!(
                    object.kind,
                    ObjectKind::Parameter | ObjectKind::ParameterGroup
                ),
                PaletteProjection::Deformation =>
                    matches!(object.kind, ObjectKind::Mesh | ObjectKind::Deformer),
            }),
            "Objects do not belong to this palette projection"
        );
        let ids: Vec<_> = objects.iter().map(|object| object.id.clone()).collect();
        let source = self.session.capture_authoring();
        let model = source.captured.snapshot.document();
        let roots = if projection == PaletteProjection::Deformation {
            let parent = model
                .scene_deformer_parent(&first.id)
                .ok_or_else(|| anyhow!("Object has no deformation collection"))?;
            if objects
                .iter()
                .any(|object| model.scene_deformer_parent(&object.id) != Some(parent))
            {
                return Err(failure(zipzi_core::Status::error(
                    "PALETTE_DEFORMATION_SIBLINGS",
                    "Choose deformation siblings",
                )));
            }
            // Validate every member, including blocks already at a boundary.
            // This native plan is never applied to the live document.
            model
                .validate_scene_edit(&SceneEdit::Move {
                    ids: ids.clone(),
                    destination: SceneCollection::Deformation {
                        parent_id: (!parent.is_empty()).then(|| parent.to_owned()),
                    },
                    before: None,
                })
                .map_err(failure)?;
            ids
        } else {
            let closure = model.organization_subtree(&ids).map_err(failure)?;
            model.organization_subtree_unlocked(&ids).map_err(failure)?;
            let members: HashSet<_> = closure.iter().map(String::as_str).collect();
            // Native closure already absorbs selected descendants. Its entries
            // without an owner in that closure are the normalized roots.
            closure
                .iter()
                .filter(|id| {
                    model
                        .organization_owner(id)
                        .is_none_or(|owner| !members.contains(owner))
                })
                .cloned()
                .collect()
        };
        let mut groups: Vec<(StepCollection, Vec<String>)> = Vec::new();
        let mut slots = HashMap::new();
        for id in roots {
            let owner = model.organization_owner(&id).map(str::to_owned);
            let collection = match projection {
                PaletteProjection::Organization => StepCollection::Organization {
                    root_parts: owner.is_none() && model.part(&id).is_some(),
                    owner,
                },
                PaletteProjection::Parameters if model.parameter(&id).is_some() => {
                    StepCollection::Parameters(owner)
                }
                PaletteProjection::Parameters if model.parameter_group(&id).is_some() => {
                    StepCollection::Folders(owner)
                }
                PaletteProjection::Parameters => {
                    return Err(anyhow!("Expected a parameter or folder"));
                }
                PaletteProjection::Deformation => {
                    let parent = model.scene_deformer_parent(&id).unwrap();
                    StepCollection::Deformation {
                        parent: (!parent.is_empty()).then(|| parent.to_owned()),
                        root_deformers: parent.is_empty() && model.transform(&id).is_some(),
                    }
                }
            };
            let next = groups.len();
            let slot = *slots.entry(collection.clone()).or_insert_with(|| {
                groups.push((collection, Vec::new()));
                next
            });
            groups[slot].1.push(id);
        }
        let mut candidate = source.begin_draft("Move palette selection")?;
        for (collection, sources) in groups {
            let members = collection.members(&model)?;
            let selected: HashSet<_> = sources.iter().map(String::as_str).collect();
            let mut blocks = Vec::new();
            let mut start = 0;
            while start < members.len() {
                if !selected.contains(members[start].as_str()) {
                    start += 1;
                    continue;
                }
                let mut end = start + 1;
                while end < members.len() && selected.contains(members[end].as_str()) {
                    end += 1;
                }
                blocks.push((start, end));
                start = end;
            }
            if down {
                blocks.reverse();
            }
            for (start, end) in blocks {
                let neighbor = if down {
                    members.get(end)
                } else {
                    start.checked_sub(1).and_then(|i| members.get(i))
                };
                let Some(neighbor) = neighbor else {
                    continue;
                };
                let before = if down {
                    let current = collection.members(&candidate.candidate_document())?;
                    let i = current
                        .iter()
                        .position(|id| id == neighbor)
                        .ok_or_else(|| anyhow!("Step anchor no longer exists"))?;
                    current.get(i + 1).cloned()
                } else {
                    Some(neighbor.clone())
                };
                let edit = collection.intent(members[start..end].to_vec(), before);
                edit.apply(&mut candidate)?;
            }
        }
        Ok(PaletteMovePlan(MovePlan::Steps(PaletteStepsPlan {
            preview: crate::EditPreview::from_candidate(
                candidate,
                source.captured.snapshot.clone(),
                (),
            )?,
        })))
    }

    pub(super) fn execute_palette_steps(
        &mut self,
        plan: &PaletteStepsPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = plan.preview.publish(&mut self.session)?;
        if receipt.kind != zipzi_sdk::PublicationKind::Unchanged {
            self.pristine_blank = None;
            self.consume_authoring_receipt(&receipt, None);
            self.reconcile_parameter_preview()?;
            self.context.reconcile(&self.object_index());
        }
        Ok(self.authoring_result(receipt))
    }
}

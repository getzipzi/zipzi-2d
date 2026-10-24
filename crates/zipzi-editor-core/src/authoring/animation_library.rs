//! Resource defaults and stable palette positions, composed through native edits.
use super::*;
use anyhow::{Result, ensure};
use zipzi_core::document::{ExpressionEdit, MotionContainer, MotionEdit};

#[derive(Clone, Debug)]
enum ResourceEdit {
    Motion(MotionEdit),
    Expression(ExpressionEdit),
}
#[derive(Clone, Debug)]
pub struct AnimationLibraryPlan {
    version: zipzi_sdk::Version,
    edits: Vec<ResourceEdit>,
    noop: bool,
}
impl AnimationLibraryPlan {
    pub fn is_noop(&self) -> bool {
        self.noop
    }
}
impl ResourceEdit {
    fn apply(&self, edit: &mut zipzi_sdk::EditSession<'_>) -> Result<()> {
        match self {
            Self::Motion(intent) => edit.edit_motion(intent.clone())?,
            Self::Expression(intent) => edit.edit_expression(intent.clone())?,
        }
        Ok(())
    }
}
fn insert_after(order: &mut Vec<String>, id: String, after: Option<&str>) {
    if let Some(at) = after.and_then(|after| order.iter().position(|s| s == after)) {
        order.insert(at + 1, id);
    } else {
        order.push(id);
    }
}
fn move_before(
    order: &mut Vec<String>,
    source: &str,
    target: Option<&str>,
    position: PalettePosition,
) -> Result<()> {
    ensure!(order.iter().any(|s| s == source), "Missing resource");
    if target == Some(source) {
        return Ok(());
    }
    order.retain(|s| s != source);
    let at = if let Some(target) = target {
        order
            .iter()
            .position(|s| s == target)
            .ok_or_else(|| anyhow::anyhow!("Invalid resource destination"))?
            + usize::from(position == PalettePosition::After)
    } else {
        order.len()
    };
    order.insert(at, source.into());
    Ok(())
}
impl Document {
    fn plan_library_edits(&self, edits: Vec<ResourceEdit>) -> Result<AnimationLibraryPlan> {
        self.ensure_authoring_idle()?;
        let mut candidate = self
            .session
            .begin_owned_edit("Edit animation library", None)?;
        for intent in &edits {
            intent.apply(&mut candidate)?;
        }
        Ok(AnimationLibraryPlan {
            version: self.session.version(),
            noop: candidate.candidate_document().revision() == self.model().revision(),
            edits,
        })
    }
    pub fn animation_resource_creation_capability(
        &self,
        kind: ObjectKind,
        selection: Option<&ObjectRef>,
    ) -> Result<zipzi_sdk::CapabilityState> {
        use zipzi_sdk::AuthoringCollection as C;
        if let Some(object) = selection {
            ensure!(
                self.object_index().get(object).is_some(),
                "Creation context expired"
            );
        }
        let collection = match kind {
            ObjectKind::Motion => C::Motion(match selection {
                Some(object) if object.kind == ObjectKind::MotionGroup => {
                    MotionContainer::Group(object.id.clone())
                }
                Some(object) if object.kind == ObjectKind::MotionRegistration => {
                    self.motion_registration(&object.id)
                        .ok_or_else(|| anyhow::anyhow!("Missing registration"))?
                        .0
                }
                _ => MotionContainer::Root,
            }),
            ObjectKind::MotionGroup => C::MotionGroups,
            ObjectKind::Expression => C::Expressions,
            ObjectKind::ExpressionEntry => {
                let Some(object) = selection else {
                    return Ok(zipzi_sdk::CapabilityState::Unavailable {
                        code: "MISSING_EXPRESSION",
                    });
                };
                let expression_id = if object.kind == ObjectKind::ExpressionEntry {
                    self.expression_entry(&object.id)
                        .ok_or_else(|| anyhow::anyhow!("Missing expression entry"))?
                        .0
                        .id
                        .clone()
                } else {
                    ensure!(
                        object.kind == ObjectKind::Expression,
                        "Select an expression"
                    );
                    object.id.clone()
                };
                C::ExpressionEntries { expression_id }
            }
            _ => {
                return Ok(zipzi_sdk::CapabilityState::Unsupported {
                    code: "WRONG_OBJECT_KIND",
                });
            }
        };
        Ok(self
            .session
            .authoring_collection_capabilities(&collection)?
            .create)
    }
    pub fn plan_animation_resource_creation(
        &self,
        kind: ObjectKind,
        name: String,
        selection: Option<&ObjectRef>,
    ) -> Result<AnimationLibraryPlan> {
        let model = self.model();
        if let Some(object) = selection {
            ensure!(
                self.object_index().get(object).is_some(),
                "Creation context expired"
            );
        }
        let id = Uuid::new_v4().to_string();
        let unique = |names: Vec<&str>| {
            if !names.contains(&name.as_str()) {
                return name.clone();
            }
            (2..)
                .map(|n| format!("{name} {n}"))
                .find(|n| !names.contains(&n.as_str()))
                .unwrap()
        };
        let mut edits = vec![];
        match kind {
            ObjectKind::Motion => {
                let registration_id = Uuid::new_v4().to_string();
                let (destination, before) = match selection {
                    Some(o) if o.kind == ObjectKind::MotionGroup => {
                        (MotionContainer::Group(o.id.clone()), None)
                    }
                    Some(o) if o.kind == ObjectKind::MotionRegistration => {
                        let (container, _) = model
                            .get_motion_registration(&o.id)
                            .ok_or_else(|| anyhow::anyhow!("Missing registration"))?;
                        let entries = match &container {
                            MotionContainer::Root => model.motion_roots(),
                            MotionContainer::Group(g) => {
                                &model.get_motion_group(g).unwrap().entries
                            }
                        };
                        let before = entries
                            .iter()
                            .position(|e| e.id == o.id)
                            .and_then(|i| entries.get(i + 1))
                            .map(|e| e.id.clone());
                        (container, before)
                    }
                    _ => (MotionContainer::Root, None),
                };
                edits.push(ResourceEdit::Motion(MotionEdit::Create {
                    id: id.clone(),
                    name: unique(
                        model
                            .motion_order()
                            .iter()
                            .filter_map(|id| model.get_motion(id).map(|c| c.name.as_str()))
                            .collect(),
                    ),
                    registration_id: registration_id.clone(),
                    destination: destination.clone(),
                }));
                if before.is_some() {
                    edits.push(ResourceEdit::Motion(MotionEdit::MoveRegistrations {
                        ids: vec![registration_id],
                        destination,
                        before,
                    }));
                }
            }
            ObjectKind::MotionGroup => {
                edits.push(ResourceEdit::Motion(MotionEdit::CreateGroup {
                    id: id.clone(),
                    name: unique(
                        model
                            .motion_groups()
                            .iter()
                            .map(|g| g.name.as_str())
                            .collect(),
                    ),
                }));
                let group = selection.and_then(|o| {
                    if o.kind == ObjectKind::MotionGroup {
                        Some(o.id.clone())
                    } else if o.kind == ObjectKind::MotionRegistration {
                        match model.get_motion_registration(&o.id)?.0 {
                            MotionContainer::Group(id) => Some(id),
                            _ => None,
                        }
                    } else {
                        None
                    }
                });
                let mut order: Vec<_> =
                    model.motion_groups().iter().map(|g| g.id.clone()).collect();
                insert_after(&mut order, id, group.as_deref());
                edits.push(ResourceEdit::Motion(MotionEdit::ReorderGroups {
                    ids: order,
                }));
            }
            ObjectKind::Expression => {
                edits.push(ResourceEdit::Expression(ExpressionEdit::Create {
                    id: id.clone(),
                    name: unique(
                        model
                            .expression_order()
                            .iter()
                            .filter_map(|id| model.get_expression(id).map(|e| e.name.as_str()))
                            .collect(),
                    ),
                }));
                let mut order = model.expression_order().to_vec();
                insert_after(
                    &mut order,
                    id,
                    selection
                        .filter(|o| o.kind == ObjectKind::Expression)
                        .map(|o| o.id.as_str()),
                );
                edits.push(ResourceEdit::Expression(ExpressionEdit::Reorder {
                    ids: order,
                }));
            }
            ObjectKind::ExpressionEntry => {
                let owner =
                    selection.ok_or_else(|| anyhow::anyhow!("Select an expression first"))?;
                let (owner, before) = if owner.kind == ObjectKind::ExpressionEntry {
                    let (expression, _) = model
                        .get_expression_entry(&owner.id)
                        .ok_or_else(|| anyhow::anyhow!("Missing expression entry"))?;
                    let before = expression
                        .entries
                        .iter()
                        .position(|e| e.id == owner.id)
                        .and_then(|i| expression.entries.get(i + 1))
                        .map(|e| e.id.clone());
                    (expression.id.clone(), before)
                } else {
                    ensure!(
                        owner.kind == ObjectKind::Expression,
                        "Select an expression first"
                    );
                    (owner.id.clone(), None)
                };
                edits.push(ResourceEdit::Expression(ExpressionEdit::CreateEntry {
                    owner,
                    id,
                    before,
                }));
            }
            _ => anyhow::bail!("Unsupported animation resource"),
        }
        self.plan_library_edits(edits)
    }
    pub fn animation_resource_destinations(
        &self,
        source: &ObjectRef,
    ) -> Result<Vec<PaletteMoveDestination>> {
        ensure!(
            self.object_index().get(source).is_some(),
            "Resource context expired"
        );
        Ok(match source.kind {
            ObjectKind::MotionRegistration => std::iter::once(PaletteMoveDestination {
                object: None,
                path: String::new(),
            })
            .chain(self.motion_groups().iter().map(|g| PaletteMoveDestination {
                object: Some(ObjectRef::new(ObjectKind::MotionGroup, &g.id)),
                path: g.name.clone(),
            }))
            .collect(),
            ObjectKind::ExpressionEntry => self
                .animation_catalog()
                .expressions
                .iter()
                .map(|e| PaletteMoveDestination {
                    object: Some(ObjectRef::new(ObjectKind::Expression, &e.id)),
                    path: e.name.clone(),
                })
                .collect(),
            _ => anyhow::bail!(AuthoringPlanningError::new("ANIMATION_ORDER_ONLY")),
        })
    }
    pub fn plan_animation_resource_move(
        &self,
        source: &ObjectRef,
        target: Option<&ObjectRef>,
        position: PalettePosition,
    ) -> Result<AnimationLibraryPlan> {
        let model = self.model();
        let index = self.object_index();
        ensure!(
            index.get(source).is_some() && target.is_none_or(|target| index.get(target).is_some()),
            "Resource context expired"
        );
        if target == Some(source) {
            return self.plan_library_edits(vec![]);
        }
        let edits = match source.kind {
            ObjectKind::MotionGroup => {
                ensure!(
                    position != PalettePosition::Inside
                        && target.is_some_and(|o| o.kind == ObjectKind::MotionGroup),
                    "Motion groups cannot be nested"
                );
                let mut order = model.motion_groups().iter().map(|g| g.id.clone()).collect();
                move_before(
                    &mut order,
                    &source.id,
                    target.map(|o| o.id.as_str()),
                    position,
                )?;
                vec![ResourceEdit::Motion(MotionEdit::ReorderGroups {
                    ids: order,
                })]
            }
            ObjectKind::MotionRegistration => {
                let (destination, before) = match target {
                    None => (MotionContainer::Root, None),
                    Some(o) if o.kind == ObjectKind::MotionGroup => {
                        ensure!(
                            position == PalettePosition::Inside,
                            "Drop inside a motion group"
                        );
                        (MotionContainer::Group(o.id.clone()), None)
                    }
                    Some(o) if o.kind == ObjectKind::MotionRegistration => {
                        ensure!(
                            position != PalettePosition::Inside,
                            "Registrations cannot contain resources"
                        );
                        let (owner, _) = model
                            .get_motion_registration(&o.id)
                            .ok_or_else(|| anyhow::anyhow!("Missing target registration"))?;
                        let entries = match &owner {
                            MotionContainer::Root => model.motion_roots(),
                            MotionContainer::Group(g) => {
                                &model.get_motion_group(g).unwrap().entries
                            }
                        };
                        let before = if position == PalettePosition::Before {
                            Some(o.id.clone())
                        } else {
                            entries
                                .iter()
                                .position(|e| e.id == o.id)
                                .and_then(|i| entries.get(i + 1))
                                .map(|e| e.id.clone())
                        };
                        (owner, before)
                    }
                    _ => anyhow::bail!("Invalid motion destination"),
                };
                vec![ResourceEdit::Motion(MotionEdit::MoveRegistrations {
                    ids: vec![source.id.clone()],
                    destination,
                    before,
                })]
            }
            ObjectKind::Expression => {
                ensure!(
                    position != PalettePosition::Inside
                        && target.is_none_or(|o| o.kind == ObjectKind::Expression),
                    "Expressions cannot be nested"
                );
                let mut order = model.expression_order().to_vec();
                move_before(
                    &mut order,
                    &source.id,
                    target.map(|o| o.id.as_str()),
                    position,
                )?;
                vec![ResourceEdit::Expression(ExpressionEdit::Reorder {
                    ids: order,
                })]
            }
            ObjectKind::ExpressionEntry => {
                let target =
                    target.ok_or_else(|| anyhow::anyhow!("Select a destination expression"))?;
                let (destination, before) = if target.kind == ObjectKind::Expression {
                    ensure!(
                        position == PalettePosition::Inside,
                        "Drop inside an expression"
                    );
                    (target.id.clone(), None)
                } else {
                    ensure!(
                        target.kind == ObjectKind::ExpressionEntry
                            && position != PalettePosition::Inside,
                        "Invalid entry destination"
                    );
                    let (owner, _) = model
                        .get_expression_entry(&target.id)
                        .ok_or_else(|| anyhow::anyhow!("Missing target entry"))?;
                    let before = if position == PalettePosition::Before {
                        Some(target.id.clone())
                    } else {
                        owner
                            .entries
                            .iter()
                            .position(|e| e.id == target.id)
                            .and_then(|i| owner.entries.get(i + 1))
                            .map(|e| e.id.clone())
                    };
                    (owner.id.clone(), before)
                };
                vec![ResourceEdit::Expression(ExpressionEdit::MoveEntries {
                    ids: vec![source.id.clone()],
                    destination,
                    before,
                })]
            }
            _ => anyhow::bail!("Unsupported resource movement"),
        };
        self.plan_library_edits(edits)
    }
    pub fn plan_animation_resource_step(
        &self,
        source: &ObjectRef,
        down: bool,
    ) -> Result<AnimationLibraryPlan> {
        let model = self.model();
        let order: Vec<_> = match source.kind {
            ObjectKind::MotionGroup => model
                .motion_groups()
                .iter()
                .map(|g| ObjectRef::new(source.kind, &g.id))
                .collect(),
            ObjectKind::MotionRegistration => {
                let (owner, _) = model
                    .get_motion_registration(&source.id)
                    .ok_or_else(|| anyhow::anyhow!("Missing registration"))?;
                let entries = match owner {
                    MotionContainer::Root => model.motion_roots(),
                    MotionContainer::Group(g) => &model.get_motion_group(&g).unwrap().entries,
                };
                entries
                    .iter()
                    .map(|e| ObjectRef::new(source.kind, &e.id))
                    .collect()
            }
            ObjectKind::Expression => model
                .expression_order()
                .iter()
                .map(|id| ObjectRef::new(source.kind, id))
                .collect(),
            ObjectKind::ExpressionEntry => model
                .get_expression_entry(&source.id)
                .ok_or_else(|| anyhow::anyhow!("Missing entry"))?
                .0
                .entries
                .iter()
                .map(|e| ObjectRef::new(source.kind, &e.id))
                .collect(),
            _ => anyhow::bail!("Unsupported resource movement"),
        };
        let at = order
            .iter()
            .position(|o| o == source)
            .ok_or_else(|| anyhow::anyhow!("Missing resource"))?;
        let target = order
            .get(if down {
                (at + 1).min(order.len() - 1)
            } else {
                at.saturating_sub(1)
            })
            .unwrap();
        self.plan_animation_resource_move(
            source,
            Some(target),
            if down {
                PalettePosition::After
            } else {
                PalettePosition::Before
            },
        )
    }
    pub fn execute_animation_library(
        &mut self,
        plan: &AnimationLibraryPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let mut edit = self
            .session
            .begin_owned_edit("Edit animation library", Some(plan.version))?;
        for intent in &plan.edits {
            intent.apply(&mut edit)?;
        }
        let receipt = edit.commit_to(&mut self.session)?;
        if receipt.changed {
            self.pristine_blank = false;
        }
        self.consume_authoring_receipt(&receipt, false);
        Ok(self.authoring_result(receipt))
    }
}
impl crate::Editor {
    pub fn animation_creation_selection(&self, motion: bool) -> Option<ObjectRef> {
        let object = self.active_inspection()?.object()?;
        if motion && object.kind == ObjectKind::Motion {
            return self
                .animation()?
                .motion
                .as_ref()
                .filter(|m| m.clip_id == object.id)
                .and_then(|m| m.entry.as_ref())
                .map(|id| ObjectRef::new(ObjectKind::MotionRegistration, id));
        }
        if (motion
            && matches!(
                object.kind,
                ObjectKind::MotionGroup | ObjectKind::MotionRegistration
            ))
            || (!motion
                && matches!(
                    object.kind,
                    ObjectKind::Expression | ObjectKind::ExpressionEntry
                ))
        {
            Some(object)
        } else {
            None
        }
    }

    pub fn execute_animation_library(
        &mut self,
        document: crate::DocumentId,
        plan: &AnimationLibraryPlan,
    ) -> Result<OperationResult> {
        self.ensure_animation_target(document)?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|doc| doc.execute_animation_library(plan))
    }
    pub fn execute_expression_authoring(
        &mut self,
        document: crate::DocumentId,
        plan: &zipzi_sdk::ExpressionEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_animation_target(document)?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|doc| doc.execute_expression_authoring(plan))
    }
}

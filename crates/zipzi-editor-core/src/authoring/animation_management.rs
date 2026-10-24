//! Copy identities and deletion effects come from native candidate plans.
use super::*;
use anyhow::{Result, ensure};
use zipzi_core::document::{ExpressionEdit, MotionContainer, MotionEdit, MotionRegistration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionCopyKind {
    Content,
    Group,
}
#[derive(Clone, Debug)]
pub struct MotionCopyPlan {
    native: zipzi_sdk::MotionEditPlan,
    placement: Vec<MotionEdit>,
    pub selection: ObjectRef,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationRemoval {
    Registration,
    Group,
    DissolveGroup,
    Motion,
    Expression,
}
#[derive(Clone, Debug)]
enum RemovalPlan {
    Motion(zipzi_sdk::MotionEditPlan),
    Expression(zipzi_sdk::ExpressionEditPlan),
}
impl RemovalPlan {
    fn apply(&self, edit: &mut zipzi_sdk::EditSession<'_>) -> Result<()> {
        match self {
            Self::Motion(plan) => edit.apply_motion_plan(plan)?,
            Self::Expression(plan) => edit.apply_expression_plan(plan)?,
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct AnimationDeletionPlan {
    native: RemovalPlan,
    version: zipzi_sdk::Version,
    pub source: ObjectRef,
    pub operation: AnimationRemoval,
    pub removed: Vec<ObjectRef>,
    pub migrated_to_root: Vec<ObjectRef>,
    pub preserved: Vec<ObjectRef>,
    pub retained_resources: Vec<ObjectRef>,
    pub blocked_by: Vec<ObjectRef>,
    noop: bool,
}
impl AnimationDeletionPlan {
    pub fn is_noop(&self) -> bool {
        self.noop
    }
    pub fn requires_review(&self) -> bool {
        self.noop
            || self.removed.len() > 1
            || !self.migrated_to_root.is_empty()
            || !self.blocked_by.is_empty()
    }
}
fn unique_name(base: &str, existing: Vec<&str>) -> String {
    std::iter::once(base.to_owned())
        .chain((2..).map(|i| format!("{base} {i}")))
        .find(|s| !existing.contains(&s.as_str()))
        .unwrap()
}
impl Document {
    fn motion_insertion(
        &self,
        at: Option<&ObjectRef>,
    ) -> Result<(MotionContainer, Option<String>)> {
        Ok(match at {
            Some(o) if o.kind == ObjectKind::MotionGroup => {
                ensure!(
                    self.model().get_motion_group(&o.id).is_some(),
                    "Missing motion group"
                );
                (MotionContainer::Group(o.id.clone()), None)
            }
            Some(o) if o.kind == ObjectKind::MotionRegistration => {
                let (owner, _) = self
                    .motion_registration(&o.id)
                    .ok_or_else(|| anyhow::anyhow!("Missing registration"))?;
                let entries = match &owner {
                    MotionContainer::Root => self.model().motion_roots(),
                    MotionContainer::Group(id) => {
                        &self.model().get_motion_group(id).unwrap().entries
                    }
                };
                let before = entries
                    .iter()
                    .position(|e| e.id == o.id)
                    .and_then(|i| entries.get(i + 1))
                    .map(|e| e.id.clone());
                (owner, before)
            }
            None => (MotionContainer::Root, None),
            _ => anyhow::bail!("Expected a motion group or registration"),
        })
    }
    /// New registration defaults inherit Clip fades; existing registrations are untouched.
    pub fn plan_motion_reference(
        &self,
        clip: &str,
        at: Option<&ObjectRef>,
    ) -> Result<MotionCopyPlan> {
        let (destination, before) = self.motion_insertion(at)?;
        let id = Uuid::new_v4().to_string();
        let native = self.plan_motion_authoring(MotionEdit::Register {
            entry: MotionRegistration {
                id: id.clone(),
                clip_id: clip.into(),
                fade_in: None,
                fade_out: None,
                sound: None,
                extensions: Default::default(),
            },
            destination,
            before,
        })?;
        Ok(MotionCopyPlan {
            native,
            placement: vec![],
            selection: ObjectRef::new(ObjectKind::MotionRegistration, id),
        })
    }
    /// Combine native source-copy and destination-create capabilities without planning a copy.
    pub fn motion_copy_capability(
        &self,
        source: &ObjectRef,
        kind: MotionCopyKind,
    ) -> Result<zipzi_sdk::CapabilityState> {
        if !matches!(
            (kind, source.kind),
            (MotionCopyKind::Group, ObjectKind::MotionGroup)
                | (
                    MotionCopyKind::Content,
                    ObjectKind::Motion | ObjectKind::MotionRegistration
                )
        ) {
            return Ok(zipzi_sdk::CapabilityState::Unsupported {
                code: "WRONG_OBJECT_KIND",
            });
        }
        let target =
            if kind == MotionCopyKind::Content && source.kind == ObjectKind::MotionRegistration {
                ObjectRef::new(
                    ObjectKind::Motion,
                    &self
                        .motion_registration(&source.id)
                        .ok_or_else(|| anyhow::anyhow!("Missing registration"))?
                        .1
                        .clip_id,
                )
            } else {
                source.clone()
            };
        let state = self
            .authoring_capabilities(&target)?
            .state(AuthoringAction::Duplicate);
        if state != zipzi_sdk::CapabilityState::Available {
            return Ok(state);
        }
        self.animation_resource_creation_capability(
            if kind == MotionCopyKind::Group {
                ObjectKind::MotionGroup
            } else {
                ObjectKind::Motion
            },
            Some(source),
        )
    }
    pub fn plan_motion_copy(
        &self,
        source: &ObjectRef,
        kind: MotionCopyKind,
        name: String,
    ) -> Result<MotionCopyPlan> {
        let model = self.model();
        let id = Uuid::new_v4().to_string();
        let mut placement = vec![];
        let (intent, selection) = match kind {
            MotionCopyKind::Content => {
                let clip = if source.kind == ObjectKind::MotionRegistration {
                    self.motion_registration(&source.id)
                        .ok_or_else(|| anyhow::anyhow!("Missing registration"))?
                        .1
                        .clip_id
                        .clone()
                } else {
                    ensure!(source.kind == ObjectKind::Motion, "Expected motion content");
                    source.id.clone()
                };
                let at = (source.kind == ObjectKind::MotionRegistration).then_some(source);
                let (destination, before) = self.motion_insertion(at)?;
                let registration_id = Uuid::new_v4().to_string();
                if before.is_some() {
                    placement.push(MotionEdit::MoveRegistrations {
                        ids: vec![registration_id.clone()],
                        destination: destination.clone(),
                        before,
                    });
                }
                let name = unique_name(
                    &name,
                    model
                        .motion_order()
                        .iter()
                        .filter_map(|id| model.get_motion(id).map(|c| c.name.as_str()))
                        .collect(),
                );
                (
                    MotionEdit::DuplicateClip {
                        source: clip,
                        id: id.clone(),
                        name,
                        registration_id,
                        destination,
                    },
                    ObjectRef::new(ObjectKind::Motion, &id),
                )
            }
            MotionCopyKind::Group => {
                ensure!(
                    source.kind == ObjectKind::MotionGroup,
                    "Expected a motion group"
                );
                let mut ids: Vec<_> = model.motion_groups().iter().map(|g| g.id.clone()).collect();
                let at = ids
                    .iter()
                    .position(|g| g == &source.id)
                    .ok_or_else(|| anyhow::anyhow!("Missing motion group"))?;
                ids.insert(at + 1, id.clone());
                placement.push(MotionEdit::ReorderGroups { ids });
                let name = unique_name(
                    &name,
                    model
                        .motion_groups()
                        .iter()
                        .map(|g| g.name.as_str())
                        .collect(),
                );
                (
                    MotionEdit::DuplicateGroup {
                        source: source.id.clone(),
                        id: id.clone(),
                        name,
                    },
                    ObjectRef::new(ObjectKind::MotionGroup, &id),
                )
            }
        };
        let native = self.plan_motion_authoring(intent)?;
        let mut candidate = self
            .session
            .begin_owned_edit("Copy motion", Some(native.version()))?;
        candidate.apply_motion_plan(&native)?;
        for intent in &placement {
            candidate.edit_motion(intent.clone())?;
        }
        Ok(MotionCopyPlan {
            native,
            placement,
            selection,
        })
    }
    pub fn execute_motion_copy(&mut self, plan: &MotionCopyPlan) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let mut edit = self
            .session
            .begin_owned_edit("Copy or register motion", Some(plan.native.version()))?;
        edit.apply_motion_plan(&plan.native)?;
        for intent in &plan.placement {
            edit.edit_motion(intent.clone())?;
        }
        let receipt = edit.commit_to(&mut self.session)?;
        if receipt.changed {
            self.pristine_blank = false;
        }
        // This plan only adds identities and orders the library; existing Clip
        // content and registration overrides are unchanged. Native receipts may
        // mark sibling anchors modified, and copied tracks as evaluation inputs.
        let previous_input = self.animation_input_revision;
        self.consume_authoring_receipt(&receipt, false);
        self.animation_input_revision = previous_input;
        Ok(self.authoring_result(receipt))
    }
    pub fn plan_animation_deletion(
        &self,
        source: &ObjectRef,
        operation: AnimationRemoval,
    ) -> Result<AnimationDeletionPlan> {
        self.ensure_authoring_idle()?;
        let native = match operation {
            AnimationRemoval::Registration => {
                ensure!(
                    source.kind == ObjectKind::MotionRegistration,
                    "Expected registration"
                );
                RemovalPlan::Motion(self.plan_motion_authoring(
                    MotionEdit::DeleteRegistrations {
                        ids: vec![source.id.clone()],
                    },
                )?)
            }
            AnimationRemoval::Group | AnimationRemoval::DissolveGroup => {
                ensure!(
                    source.kind == ObjectKind::MotionGroup,
                    "Expected motion group"
                );
                RemovalPlan::Motion(self.plan_motion_authoring(
                    if operation == AnimationRemoval::Group {
                        MotionEdit::DeleteGroup {
                            id: source.id.clone(),
                        }
                    } else {
                        MotionEdit::DissolveGroup {
                            id: source.id.clone(),
                        }
                    },
                )?)
            }
            AnimationRemoval::Motion => {
                ensure!(source.kind == ObjectKind::Motion, "Expected motion content");
                RemovalPlan::Motion(self.plan_motion_authoring(MotionEdit::DeleteClip {
                    id: source.id.clone(),
                })?)
            }
            AnimationRemoval::Expression => {
                ensure!(source.kind == ObjectKind::Expression, "Expected expression");
                RemovalPlan::Expression(self.plan_expression_authoring(ExpressionEdit::Delete {
                    id: source.id.clone(),
                })?)
            }
        };
        let mut candidate = self
            .session
            .begin_owned_edit("Review animation deletion", None)?;
        native.apply(&mut candidate)?;
        let after = candidate.candidate_document();
        let before = self.model();
        let mut removed = vec![];
        let mut migrated_to_root = vec![];
        let mut preserved = vec![];
        let after_groups: std::collections::HashSet<_> = after
            .motion_groups()
            .iter()
            .map(|g| g.id.as_str())
            .collect();
        let after_registrations: std::collections::HashSet<_> = after
            .motion_registrations()
            .map(|e| e.id.as_str())
            .collect();
        let after_roots: std::collections::HashSet<_> =
            after.motion_roots().iter().map(|e| e.id.as_str()).collect();
        for group in before.motion_groups() {
            if !after_groups.contains(group.id.as_str()) {
                removed.push(ObjectRef::new(ObjectKind::MotionGroup, &group.id));
            }
        }
        let entries = before
            .motion_groups()
            .iter()
            .flat_map(|g| {
                g.entries
                    .iter()
                    .map(move |entry| (Some(g.id.as_str()), entry))
            })
            .chain(before.motion_roots().iter().map(|entry| (None, entry)));
        for (owner, entry) in entries {
            let object = ObjectRef::new(ObjectKind::MotionRegistration, &entry.id);
            let survives = after_registrations.contains(entry.id.as_str());
            if !survives {
                removed.push(object);
            } else if owner.is_some() && after_roots.contains(entry.id.as_str()) {
                migrated_to_root.push(object);
            }
            let affected = entry.id == source.id || owner == Some(source.id.as_str());
            if affected {
                if after.get_motion(&entry.clip_id).is_some() {
                    preserved.push(ObjectRef::new(ObjectKind::Motion, &entry.clip_id));
                }
                if survives {
                    preserved.push(ObjectRef::new(ObjectKind::MotionRegistration, &entry.id));
                }
            }
        }
        for clip in before
            .motion_order()
            .iter()
            .filter_map(|id| before.get_motion(id))
        {
            if after.get_motion(&clip.id).is_none() {
                removed.push(ObjectRef::new(ObjectKind::Motion, &clip.id));
                for track in &clip.tracks {
                    removed.push(ObjectRef::new(ObjectKind::MotionTrack, &track.id));
                    removed.extend(
                        track
                            .keys()
                            .map(|key| ObjectRef::new(ObjectKind::MotionKey, &key.id)),
                    );
                }
                removed.extend(
                    clip.events
                        .iter()
                        .map(|event| ObjectRef::new(ObjectKind::MotionEvent, &event.id)),
                );
            }
        }
        for expression in before
            .expression_order()
            .iter()
            .filter_map(|id| before.get_expression(id))
        {
            if after.get_expression(&expression.id).is_none() {
                removed.push(ObjectRef::new(ObjectKind::Expression, &expression.id));
                removed.extend(
                    expression
                        .entries
                        .iter()
                        .map(|e| ObjectRef::new(ObjectKind::ExpressionEntry, &e.id)),
                );
            }
        }
        let index = self.object_index();
        let deleted: std::collections::HashSet<_> = removed.iter().cloned().collect();
        let mut blocked_by = vec![];
        let mut retained_resources = vec![];
        for object in &removed {
            blocked_by.extend(
                index
                    .incoming(object)
                    .filter(|r| {
                        !deleted.contains(&r.source) && r.kind != crate::RelationKind::Contains
                    })
                    .map(|r| r.source.clone()),
            );
            retained_resources.extend(
                index
                    .outgoing(object)
                    .filter(|r| {
                        !deleted.contains(&r.target) && r.kind != crate::RelationKind::Contains
                    })
                    .map(|r| r.target.clone()),
            );
        }
        for objects in [&mut preserved, &mut blocked_by, &mut retained_resources] {
            let mut seen = std::collections::HashSet::new();
            objects.retain(|o| seen.insert(o.clone()));
        }
        let preserved_set: std::collections::HashSet<_> = preserved.iter().collect();
        retained_resources.retain(|o| !preserved_set.contains(o));
        Ok(AnimationDeletionPlan {
            native,
            version: self.session.version(),
            source: source.clone(),
            operation,
            removed,
            migrated_to_root,
            preserved,
            retained_resources,
            blocked_by,
            noop: after.revision() == before.revision(),
        })
    }
    pub fn execute_animation_deletion(
        &mut self,
        plan: &AnimationDeletionPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        ensure!(
            plan.blocked_by.is_empty(),
            AuthoringPlanningError::new("OBJECT_REFERENCED")
        );
        let mut edit = self
            .session
            .begin_owned_edit("Delete animation resource", Some(plan.version))?;
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
    pub fn execute_motion_copy(
        &mut self,
        document: crate::DocumentId,
        plan: &MotionCopyPlan,
    ) -> Result<OperationResult> {
        self.ensure_animation_target(document)?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|doc| doc.execute_motion_copy(plan))
    }
    pub fn execute_animation_deletion(
        &mut self,
        document: crate::DocumentId,
        plan: &AnimationDeletionPlan,
    ) -> Result<OperationResult> {
        self.ensure_animation_target(document)?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|doc| doc.execute_animation_deletion(plan))
    }
}

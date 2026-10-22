//! Composable publication summaries. Expression deltas are derived from borrowed
//! domain storage, so direct edits, imports, candidates and history agree.
use crate::types::{ObjectKey, ObjectKind};
use kasane_core::document::ExpressionTarget;
use kasane_core::{ChangeKind, Document};
use serde::Serialize;
use std::collections::{BTreeSet, HashSet};
mod binding;
mod motion;
mod physics;
mod pose;
pub use binding::BindingReferenceChange;
pub use motion::{MotionReferenceChange, MotionReferenceValue};
pub use physics::{PhysicsReferenceChange, PhysicsReferenceValue};
pub use pose::PoseReferenceChange;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeCategory {
    Labels,
    Order,
    References,
    Content,
    Topology,
    EvaluationInputs,
    EditorState,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntityRef {
    pub document_id: String,
    pub kind: ObjectKind,
    pub id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CollectionRef {
    DeformationChildren {
        document_id: String,
        parent_id: String,
    },
    BindingGrid {
        document_id: String,
        binding_id: String,
    },
    Pose {
        document_id: String,
        owner: Option<String>,
        role: String,
    },
    Physics {
        document_id: String,
        owner: Option<String>,
        role: String,
    },
    Motion {
        document_id: String,
        owner: Option<String>,
        role: String,
    },
    Expressions {
        document_id: String,
    },
    ExpressionEntries {
        document_id: String,
        expression_id: String,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReferenceChange {
    pub entry: EntityRef,
    pub before: Option<ExpressionTarget>,
    pub after: Option<ExpressionTarget>,
}
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ChangeSet {
    pub created: Vec<EntityRef>,
    pub deleted: Vec<EntityRef>,
    pub modified: Vec<EntityRef>,
    pub collections: Vec<CollectionRef>,
    pub references: Vec<ReferenceChange>,
    pub motion_references: Vec<MotionReferenceChange>,
    pub pose_references: Vec<PoseReferenceChange>,
    pub physics_references: Vec<PhysicsReferenceChange>,
    pub binding_references: Vec<BindingReferenceChange>,
    pub resource_content: Vec<EntityRef>,
    pub categories: BTreeSet<ChangeCategory>,
    pub full_invalidation: bool,
}
impl ChangeSet {
    pub(crate) fn inverted(&self) -> Self {
        let mut inverse = self.clone();
        std::mem::swap(&mut inverse.created, &mut inverse.deleted);
        for edge in &mut inverse.references {
            std::mem::swap(&mut edge.before, &mut edge.after);
        }
        for edge in &mut inverse.motion_references {
            std::mem::swap(&mut edge.before, &mut edge.after);
        }
        for edge in &mut inverse.pose_references {
            std::mem::swap(&mut edge.before, &mut edge.after);
        }
        for edge in &mut inverse.physics_references {
            std::mem::swap(&mut edge.before, &mut edge.after);
        }
        for edge in &mut inverse.binding_references {
            std::mem::swap(&mut edge.before, &mut edge.after);
        }
        inverse
    }
    pub(crate) fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + (self.created.capacity() + self.deleted.capacity() + self.modified.capacity())
                * std::mem::size_of::<EntityRef>()
            + self
                .created
                .iter()
                .chain(&self.deleted)
                .chain(&self.modified)
                .map(|e| e.id.capacity() + e.document_id.capacity())
                .sum::<usize>()
            + self.collections.capacity() * std::mem::size_of::<CollectionRef>()
            + self
                .collections
                .iter()
                .map(|c| match c {
                    CollectionRef::DeformationChildren {
                        document_id,
                        parent_id,
                    } => document_id.capacity() + parent_id.capacity(),
                    CollectionRef::BindingGrid {
                        document_id,
                        binding_id,
                    } => document_id.capacity() + binding_id.capacity(),
                    CollectionRef::Pose {
                        document_id,
                        owner,
                        role,
                    } => {
                        document_id.capacity()
                            + owner.as_ref().map_or(0, String::capacity)
                            + role.capacity()
                    }
                    CollectionRef::Physics {
                        document_id,
                        owner,
                        role,
                    } => {
                        document_id.capacity()
                            + owner.as_ref().map_or(0, String::capacity)
                            + role.capacity()
                    }
                    CollectionRef::Motion {
                        document_id,
                        owner,
                        role,
                    } => {
                        document_id.capacity()
                            + owner.as_ref().map_or(0, |s| s.capacity())
                            + role.capacity()
                    }
                    CollectionRef::Expressions { document_id } => document_id.capacity(),
                    CollectionRef::ExpressionEntries {
                        document_id,
                        expression_id,
                    } => document_id.capacity() + expression_id.capacity(),
                })
                .sum::<usize>()
            + self.references.capacity() * std::mem::size_of::<ReferenceChange>()
            + self
                .references
                .iter()
                .map(|r| {
                    r.entry.id.capacity()
                        + r.entry.document_id.capacity()
                        + target_bytes(&r.before)
                        + target_bytes(&r.after)
                })
                .sum::<usize>()
            + self
                .resource_content
                .iter()
                .map(|e| {
                    std::mem::size_of::<EntityRef>() + e.id.capacity() + e.document_id.capacity()
                })
                .sum::<usize>()
            + self
                .motion_references
                .iter()
                .map(|r| {
                    std::mem::size_of::<MotionReferenceChange>()
                        + r.entity.id.capacity()
                        + r.entity.document_id.capacity()
                        + r.field.capacity()
                        + motion_target_bytes(&r.before)
                        + motion_target_bytes(&r.after)
                })
                .sum::<usize>()
            + self
                .physics_references
                .iter()
                .map(|r| {
                    std::mem::size_of::<PhysicsReferenceChange>()
                        + r.entity.id.capacity()
                        + r.entity.document_id.capacity()
                        + r.field.capacity()
                        + physics_target_bytes(&r.before)
                        + physics_target_bytes(&r.after)
                })
                .sum::<usize>()
            + self
                .pose_references
                .iter()
                .map(|r| {
                    std::mem::size_of::<PoseReferenceChange>()
                        + r.entity.id.capacity()
                        + r.entity.document_id.capacity()
                        + pose::reference_bytes(&r.before)
                        + pose::reference_bytes(&r.after)
                })
                .sum::<usize>()
            + self.categories.len() * (std::mem::size_of::<ChangeCategory>() + 64)
            + self
                .binding_references
                .iter()
                .map(|r| {
                    std::mem::size_of::<BindingReferenceChange>()
                        + r.entity.id.capacity()
                        + r.entity.document_id.capacity()
                        + r.field.capacity()
                        + r.before.as_ref().map_or(0, String::capacity)
                        + r.after.as_ref().map_or(0, String::capacity)
                })
                .sum::<usize>()
    }
}

pub(crate) fn summarize(
    before: &Document,
    after: &Document,
    before_keys: &HashSet<ObjectKey>,
    after_keys: &HashSet<ObjectKey>,
    object_ids: &[String],
    kinds: &BTreeSet<ChangeCategory>,
) -> ChangeSet {
    let mut out = ChangeSet::default();
    let entity = |kind, id: &str| EntityRef {
        document_id: after.id().into(),
        kind,
        id: id.into(),
    };
    out.created = after_keys
        .difference(before_keys)
        .map(|key| entity(key.kind, &key.id))
        .collect();
    out.deleted = before_keys
        .difference(after_keys)
        .map(|key| entity(key.kind, &key.id))
        .collect();
    // C1 only scans expression storage when an expression or owned entry was touched.
    let expression_edit = object_ids.iter().any(|id| {
        before.get_expression(id).is_some()
            || after.get_expression(id).is_some()
            || before.get_expression_entry(id).is_some()
            || after.get_expression_entry(id).is_some()
    });
    out.categories.extend(kinds.iter().copied());
    if expression_edit {
        if before.expression_order() != after.expression_order() {
            out.collections.push(CollectionRef::Expressions {
                document_id: after.id().into(),
            });
            out.categories.insert(ChangeCategory::Order);
        }
        let owners: BTreeSet<_> = before
            .expression_order()
            .iter()
            .chain(after.expression_order())
            .collect();
        for id in owners {
            let old = before.get_expression(id);
            let new = after.get_expression(id);
            if old == new {
                continue;
            }
            if old.is_some() && new.is_some() {
                out.modified.push(entity(ObjectKind::Expression, id));
            }
            if old.map(|e| &e.name) != new.map(|e| &e.name) {
                out.categories.insert(ChangeCategory::Labels);
            }
            if old.map(|e| {
                (
                    &e.file_type,
                    &e.extensions,
                    &e.opaque_source_ids,
                    &e.opaque_source_content_hash,
                )
            }) != new.map(|e| {
                (
                    &e.file_type,
                    &e.extensions,
                    &e.opaque_source_ids,
                    &e.opaque_source_content_hash,
                )
            }) {
                out.categories.insert(ChangeCategory::Content);
            }
            let old_entries: Vec<_> = old
                .into_iter()
                .flat_map(|e| e.entries.iter().map(|e| &e.id))
                .collect();
            let new_entries: Vec<_> = new
                .into_iter()
                .flat_map(|e| e.entries.iter().map(|e| &e.id))
                .collect();
            if old_entries != new_entries {
                out.collections.push(CollectionRef::ExpressionEntries {
                    document_id: after.id().into(),
                    expression_id: id.clone(),
                });
                out.categories.insert(ChangeCategory::Order);
            }
            if old.map(|e| (e.fade_in, e.fade_out, &e.entries))
                != new.map(|e| (e.fade_in, e.fade_out, &e.entries))
            {
                out.categories.insert(ChangeCategory::EvaluationInputs);
            }
        }
        let entry_ids: BTreeSet<_> = before
            .expression_order()
            .iter()
            .filter_map(|id| before.get_expression(id))
            .chain(
                after
                    .expression_order()
                    .iter()
                    .filter_map(|id| after.get_expression(id)),
            )
            .flat_map(|a| a.entries.iter().map(|e| &e.id))
            .collect();
        for id in entry_ids {
            let old = before.get_expression_entry(id);
            let new = after.get_expression_entry(id);
            if old.map(|(a, e)| (&a.id, e)) == new.map(|(a, e)| (&a.id, e)) {
                continue;
            }
            if old.is_some() && new.is_some() {
                out.modified.push(entity(ObjectKind::ExpressionEntry, id));
            }
            let a = old.map(|(_, e)| e.target.clone());
            let b = new.map(|(_, e)| e.target.clone());
            if a != b {
                out.categories.insert(ChangeCategory::References);
                out.references.push(ReferenceChange {
                    entry: entity(ObjectKind::ExpressionEntry, id),
                    before: a,
                    after: b,
                });
            }
        }
    }
    motion::summarize_motion(before, after, &mut out);
    physics::summarize_physics(before, after, &mut out);
    pose::summarize_pose(before, after, &mut out);
    binding::summarize_bindings(before, after, object_ids, &mut out);
    binding::summarize_binding_children(before, after, object_ids, &mut out);
    // Persistent authoring flags affect capabilities even when domain content
    // is unchanged. Derive them from both states so undo uses the same summary.
    if before.editor_state() != after.editor_state() {
        for key in after_keys.intersection(before_keys) {
            if before.editor_state().get(&key.id) != after.editor_state().get(&key.id) {
                out.modified.push(entity(key.kind, &key.id));
                out.categories.insert(ChangeCategory::EditorState);
                // Entry availability depends on its owner, and an owner's subtree
                // operations depend on entry locks. Report those dependants too.
                if key.kind == ObjectKind::Expression {
                    if let Some(owner) = after.get_expression(&key.id) {
                        out.modified.extend(
                            owner
                                .entries
                                .iter()
                                .map(|entry| entity(ObjectKind::ExpressionEntry, &entry.id)),
                        );
                    }
                } else if key.kind == ObjectKind::ExpressionEntry {
                    if let Some((owner, _)) = after.get_expression_entry(&key.id) {
                        out.modified.push(entity(ObjectKind::Expression, &owner.id));
                    }
                }
            }
        }
    }
    for id in object_ids {
        for key in after_keys.intersection(before_keys).filter(|key| {
            key.id == *id
                && !matches!(
                    key.kind,
                    ObjectKind::Expression
                        | ObjectKind::ExpressionEntry
                        | ObjectKind::Pose
                        | ObjectKind::PoseGroup
                        | ObjectKind::PoseEntry
                        | ObjectKind::PoseLink
                        | ObjectKind::Physics
                        | ObjectKind::PhysicsGroup
                        | ObjectKind::PhysicsInput
                        | ObjectKind::PhysicsOutput
                        | ObjectKind::PhysicsNode
                )
        }) {
            out.modified.push(entity(key.kind, id));
        }
    }
    // Deterministic order is useful to event consumers and persisted diagnostics.
    for entities in [&mut out.created, &mut out.deleted, &mut out.modified] {
        entities.sort_by(|a, b| a.id.cmp(&b.id));
        entities.dedup();
    }
    out
}
pub(crate) fn categories(kind: ChangeKind) -> BTreeSet<ChangeCategory> {
    use ChangeCategory::*;
    match kind {
        ChangeKind::None => BTreeSet::new(),
        ChangeKind::Metadata => BTreeSet::new(), // Derived from actual expression fields above.
        ChangeKind::Structure => BTreeSet::from([References, Topology, EvaluationInputs]),
        ChangeKind::Resources => BTreeSet::from([Content, EvaluationInputs]),
        ChangeKind::Positions | ChangeKind::Appearance => BTreeSet::from([EvaluationInputs]),
    }
}

fn target_bytes(target: &Option<ExpressionTarget>) -> usize {
    match target {
        Some(ExpressionTarget::Resolved { parameter_id }) => parameter_id.capacity(),
        Some(ExpressionTarget::Unresolved { runtime_id }) => runtime_id.capacity(),
        _ => 0,
    }
}

fn motion_target_bytes(value: &Option<MotionReferenceValue>) -> usize {
    match value {
        Some(MotionReferenceValue::Clip { id }) => id.capacity(),
        Some(MotionReferenceValue::Attachment { reference }) => match reference {
            kasane_core::document::AttachmentRef::Resolved { attachment_id } => {
                attachment_id.capacity()
            }
            kasane_core::document::AttachmentRef::Unresolved { path } => path.capacity(),
        },
        Some(MotionReferenceValue::Track { target }) => match target {
            kasane_core::document::MotionTrackTarget::Unspecified => 0,
            kasane_core::document::MotionTrackTarget::Model { runtime_id } => runtime_id.capacity(),
            kasane_core::document::MotionTrackTarget::Parameter { parameter_id } => {
                parameter_id.capacity()
            }
            kasane_core::document::MotionTrackTarget::PartOpacity { part_id } => part_id.capacity(),
            kasane_core::document::MotionTrackTarget::Unresolved {
                category,
                runtime_id,
            } => category.capacity() + runtime_id.capacity(),
        },
        _ => 0,
    }
}

fn physics_target_bytes(value: &Option<PhysicsReferenceValue>) -> usize {
    match value {
        Some(PhysicsReferenceValue::Node { id }) => id.capacity(),
        Some(PhysicsReferenceValue::Parameter { target }) => match target {
            kasane_core::document::PhysicsTarget::Resolved { parameter_id } => {
                parameter_id.capacity()
            }
            kasane_core::document::PhysicsTarget::Unresolved { runtime_id } => {
                runtime_id.capacity()
            }
            _ => 0,
        },
        None => 0,
    }
}

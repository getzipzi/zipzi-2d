//! Composable publication summaries. Expression deltas are derived from borrowed
//! domain storage, so direct edits, imports, candidates and history agree.
use crate::types::{ObjectKey, ObjectKind};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use zipzi_core::document::ExpressionTarget;
use zipzi_core::{ChangeKind, Document};
mod binding;
mod content;
mod document;
mod references;
mod resources;
use references::ReferenceFieldChangeSize;
pub use references::{ReferenceFieldChange, ReferenceSelectionChange};
mod mesh;
mod motion;
pub(crate) use mesh::MeshPublication;
mod organization;
mod scene;
use content::SceneContentReferenceChangeSize;
pub use content::{SceneContentChange, SceneContentReferenceChange, SceneContentReferenceValue};
pub use scene::{SceneFieldChange, SceneReferenceChange};
mod physics;
mod pose;
pub use binding::BindingReferenceChange;
pub use motion::{MotionReferenceChange, MotionReferenceValue};
pub use organization::OrganizationReferenceChange;
pub use physics::{PhysicsReferenceChange, PhysicsReferenceValue};
pub use pose::PoseReferenceChange;

pub use zipzi_contract::changes::{
    ChangeCategory, ChangeSet, CollectionRef, EntityRef, ReferenceChange,
};

trait CollectionRefExt {
    fn extra_bytes(&self) -> usize;
}
impl CollectionRefExt for CollectionRef {
    fn extra_bytes(&self) -> usize {
        match self {
            CollectionRef::Resources { document_id } => document_id.capacity(),
            CollectionRef::References {
                document_id,
                collection,
            } => {
                document_id.capacity()
                    + match collection {
                        zipzi_core::document::ReferenceCollection::ModelParameterMembers {
                            group_id,
                        } => group_id.capacity(),
                        zipzi_core::document::ReferenceCollection::CdiCombinedMembers {
                            set_id,
                        } => set_id.capacity(),
                        _ => 0,
                    }
            }
            CollectionRef::SceneContent {
                document_id,
                owner_id,
                ..
            } => document_id.capacity() + owner_id.capacity(),
            CollectionRef::Scene {
                document_id,
                collection,
            } => {
                document_id.capacity()
                    + match collection {
                        zipzi_core::document::SceneCollection::Organization { part_id } => {
                            part_id.as_ref().map_or(0, String::capacity)
                        }
                        zipzi_core::document::SceneCollection::Deformation { parent_id } => {
                            parent_id.as_ref().map_or(0, String::capacity)
                        }
                        zipzi_core::document::SceneCollection::Drawing { owner } => {
                            owner.capacity()
                        }
                        _ => 0,
                    }
            }
            CollectionRef::DeformationChildren {
                document_id,
                parent_id,
            } => document_id.capacity() + parent_id.capacity(),
            CollectionRef::BindingGrid {
                document_id,
                binding_id,
            } => document_id.capacity() + binding_id.capacity(),
            CollectionRef::Organization {
                document_id,
                collection,
            } => {
                document_id.capacity()
                    + match collection {
                        zipzi_core::document::OrganizationCollection::Parameters { group_id } => {
                            group_id.as_ref().map_or(0, String::capacity)
                        }
                        zipzi_core::document::OrganizationCollection::ParameterGroups {
                            parent_id,
                        } => parent_id.as_ref().map_or(0, String::capacity),
                        zipzi_core::document::OrganizationCollection::PartMembers { part_id } => {
                            part_id.as_ref().map_or(0, String::capacity)
                        }
                    }
            }
            CollectionRef::Pose {
                document_id,
                owner,
                role,
            }
            | CollectionRef::Physics {
                document_id,
                owner,
                role,
            }
            | CollectionRef::Motion {
                document_id,
                owner,
                role,
            } => {
                document_id.capacity()
                    + owner.as_ref().map_or(0, String::capacity)
                    + role.capacity()
            }
            CollectionRef::Expressions { document_id } => document_id.capacity(),
            CollectionRef::ExpressionEntries {
                document_id,
                expression_id,
            } => document_id.capacity() + expression_id.capacity(),
        }
    }
}

trait EntityRefExt: Sized {
    fn new(document_id: &str, kind: ObjectKind, id: &str) -> Self;
    fn extra_bytes(&self) -> usize;
}
impl EntityRefExt for EntityRef {
    fn new(document_id: &str, kind: ObjectKind, id: &str) -> Self {
        Self {
            document_id: document_id.into(),
            kind,
            id: id.into(),
        }
    }

    fn extra_bytes(&self) -> usize {
        self.id.capacity() + self.document_id.capacity()
    }
}

// Include spare vector slots as well as allocations owned by occupied elements.
fn vec_bytes<T>(items: &Vec<T>, extra: impl Fn(&T) -> usize) -> usize {
    items.capacity() * std::mem::size_of::<T>() + items.iter().map(extra).sum::<usize>()
}

// Preserve sorted traversal and leave field comparison and classification to each domain.
fn paired_records<'a, K: Ord, V>(
    before: &'a BTreeMap<K, V>,
    after: &'a BTreeMap<K, V>,
) -> impl Iterator<Item = (&'a K, Option<&'a V>, Option<&'a V>)> {
    before
        .keys()
        .chain(after.keys())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|key| (key, before.get(key), after.get(key)))
}

pub(crate) trait ChangeSetExt: Sized {
    fn affected_entities(&self) -> impl Iterator<Item = &EntityRef>;
    fn retain_existing_modified(&mut self);
    fn inverted(&self) -> Self;
    fn estimated_bytes(&self) -> usize;
}
impl ChangeSetExt for ChangeSet {
    /// Publication order is preserved; callers choose whether to deduplicate IDs.
    fn affected_entities(&self) -> impl Iterator<Item = &EntityRef> {
        self.created
            .iter()
            .chain(&self.deleted)
            .chain(&self.modified)
    }

    fn retain_existing_modified(&mut self) {
        self.modified
            .retain(|e| !self.created.contains(e) && !self.deleted.contains(e));
    }

    fn inverted(&self) -> Self {
        let mut inverse = self.clone();
        std::mem::swap(&mut inverse.created, &mut inverse.deleted);
        macro_rules! invert_edges {
            ($($field:ident),+ $(,)?) => {$(
                for edge in &mut inverse.$field {
                    std::mem::swap(&mut edge.before, &mut edge.after);
                }
            )+};
        }
        invert_edges!(
            references,
            motion_references,
            reference_fields,
            reference_selections,
            content_references,
            scene_references,
            organization_references,
            pose_references,
            physics_references,
            binding_references,
        );
        for edge in &mut inverse.reference_fields {
            std::mem::swap(&mut edge.before_owner, &mut edge.after_owner);
        }
        inverse
    }
    fn estimated_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.document_fields.capacity()
                * std::mem::size_of::<zipzi_core::document::DocumentField>()
            + vec_bytes(&self.reference_fields, ReferenceFieldChange::extra_bytes)
            + vec_bytes(&self.reference_selections, |c| {
                c.entity.extra_bytes()
                    + vec_bytes(&c.before, String::capacity)
                    + vec_bytes(&c.after, String::capacity)
            })
            + vec_bytes(
                &self.content_references,
                SceneContentReferenceChange::extra_bytes,
            )
            + vec_bytes(&self.content_changes, |c| c.entity.extra_bytes())
            + vec_bytes(&self.created, EntityRef::extra_bytes)
            + vec_bytes(&self.deleted, EntityRef::extra_bytes)
            + vec_bytes(&self.modified, EntityRef::extra_bytes)
            + vec_bytes(self.pose_field_changes(), String::capacity)
            + vec_bytes(&self.collections, CollectionRef::extra_bytes)
            + vec_bytes(&self.references, |r| {
                r.entry.extra_bytes() + target_bytes(&r.before) + target_bytes(&r.after)
            })
            + self
                .resource_content
                .iter()
                .map(|e| std::mem::size_of::<EntityRef>() + e.extra_bytes())
                .sum::<usize>()
            + self
                .motion_references
                .iter()
                .map(|r| {
                    std::mem::size_of::<MotionReferenceChange>()
                        + r.entity.extra_bytes()
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
                        + r.entity.extra_bytes()
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
                        + r.entity.extra_bytes()
                        + pose::reference_bytes(&r.before)
                        + pose::reference_bytes(&r.after)
                })
                .sum::<usize>()
            + self
                .organization_references
                .iter()
                .map(|r| {
                    std::mem::size_of::<OrganizationReferenceChange>()
                        + r.entity.extra_bytes()
                        + r.before.as_ref().map_or(0, String::capacity)
                        + r.after.as_ref().map_or(0, String::capacity)
                })
                .sum::<usize>()
            + self
                .scene_fields
                .iter()
                .map(|r| std::mem::size_of::<SceneFieldChange>() + r.entity.extra_bytes())
                .sum::<usize>()
            + self
                .scene_references
                .iter()
                .map(|r| {
                    std::mem::size_of::<SceneReferenceChange>()
                        + r.entity.extra_bytes()
                        + vec_bytes(&r.before, zipzi_core::SceneReference::capacity)
                        + vec_bytes(&r.after, zipzi_core::SceneReference::capacity)
                })
                .sum::<usize>()
            + self.categories.len() * (std::mem::size_of::<ChangeCategory>() + 64)
            + self
                .binding_references
                .iter()
                .map(|r| {
                    std::mem::size_of::<BindingReferenceChange>()
                        + r.entity.extra_bytes()
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
    summarize_impl(
        before,
        after,
        before_keys,
        after_keys,
        object_ids,
        kinds,
        true,
    )
}

fn summarize_impl(
    before: &Document,
    after: &Document,
    before_keys: &HashSet<ObjectKey>,
    after_keys: &HashSet<ObjectKey>,
    object_ids: &[String],
    kinds: &BTreeSet<ChangeCategory>,
    prune_domains: bool,
) -> ChangeSet {
    #[cfg(feature = "bench-instrumentation")]
    let _span = zipzi_core::benchmark::span("sdk.summarize");
    #[cfg(feature = "bench-instrumentation")]
    let _full_span = zipzi_core::benchmark::span("sdk.summarize.fallback");
    use zipzi_core::document::DocumentDomain as D;
    let unchanged = |domain| prune_domains && before.same_domain(after, domain);
    let mut out = ChangeSet::default();
    let entity = |kind, id: &str| EntityRef::new(after.id(), kind, id);
    if !std::ptr::eq(before_keys, after_keys) {
        out.created = after_keys
            .difference(before_keys)
            .map(|key| entity(key.kind, &key.id))
            .collect();
        out.deleted = before_keys
            .difference(after_keys)
            .map(|key| entity(key.kind, &key.id))
            .collect();
    }
    // C1 only scans expression storage when an expression or owned entry was touched.
    let expression_edit = !unchanged(D::Expressions)
        && object_ids.iter().any(|id| {
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
    if !unchanged(D::Motions) {
        motion::summarize_motion(before, after, &mut out);
    }
    physics::summarize_physics(before, after, &mut out);
    pose::summarize_pose(before, after, &mut out);
    if !unchanged(D::Organization) {
        organization::summarize_organization(before, after, &mut out);
    }
    let scene_changed = !unchanged(D::Scene);
    let bindings_changed = !unchanged(D::Bindings);
    // Scene summarization also normalizes the declared Topology category for
    // touched scene objects, including erase/recreate with identical content.
    if scene_changed || kinds.contains(&ChangeCategory::Topology) {
        scene::summarize_scene(before, after, object_ids, &mut out);
    }
    if scene_changed || bindings_changed {
        binding::summarize_bindings(before, after, object_ids, &mut out);
        binding::summarize_binding_children(before, after, object_ids, &mut out);
    }
    let references_changed = !unchanged(D::References);
    if references_changed {
        references::summarize_references(before, after, &mut out);
    }
    if scene_changed || bindings_changed {
        content::summarize_content(before, after, object_ids, &mut out);
    }
    if !unchanged(D::Assets) {
        resources::summarize_resources(before, after, &mut out);
    }
    if references_changed || !unchanged(D::Document) {
        document::summarize_document(before, after, &mut out);
    }
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
                    ObjectKind::Parameter
                        | ObjectKind::Part
                        | ObjectKind::ModelParameterGroup
                        | ObjectKind::ModelParameterMember
                        | ObjectKind::ModelHitArea
                        | ObjectKind::CdiCombinedSet
                        | ObjectKind::CdiCombinedMember
                        | ObjectKind::CdiParameterGroup
                        | ObjectKind::DrawOrderGroup
                        | ObjectKind::Expression
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
    // Meshing is authoring intent, independent of the persisted triangles used by evaluation.
    // Keep this refinement in the full summary so receipt and shadow comparisons agree; the
    // MeshPublication fast path already requires meshing to be unchanged.
    let meshing_changed = out.content_changes.iter().any(|c| {
        c.domain == zipzi_core::document::SceneContentDomain::MeshTopology
            && before.get_mesh(&c.entity.id).map(|m| &m.meshing)
                != after.get_mesh(&c.entity.id).map(|m| &m.meshing)
    });
    if meshing_changed && before.same_evaluation_inputs(after) {
        out.categories.remove(&ChangeCategory::EvaluationInputs);
    }
    finish_summary(&mut out);
    out
}

fn finish_summary(out: &mut ChangeSet) {
    out.retain_existing_modified();
    // Deterministic order is useful to event consumers and persisted diagnostics.
    for entities in [&mut out.created, &mut out.deleted, &mut out.modified] {
        // A Part and its DrawOrderGroup intentionally share an ID. Break ties
        // by kind so independent key-set enumeration has identical output.
        entities.sort_by(|a, b| {
            a.id.cmp(&b.id)
                .then_with(|| (a.kind as usize).cmp(&(b.kind as usize)))
        });
        entities.dedup();
    }
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
            zipzi_core::document::AttachmentRef::Resolved { attachment_id } => {
                attachment_id.capacity()
            }
            zipzi_core::document::AttachmentRef::Unresolved { path } => path.capacity(),
        },
        Some(MotionReferenceValue::Track { target }) => match target {
            zipzi_core::document::MotionTrackTarget::Unspecified => 0,
            zipzi_core::document::MotionTrackTarget::Model { runtime_id } => runtime_id.capacity(),
            zipzi_core::document::MotionTrackTarget::Parameter { parameter_id } => {
                parameter_id.capacity()
            }
            zipzi_core::document::MotionTrackTarget::PartOpacity { part_id } => part_id.capacity(),
            zipzi_core::document::MotionTrackTarget::Unresolved {
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
            zipzi_core::document::PhysicsTarget::Resolved { parameter_id } => {
                parameter_id.capacity()
            }
            zipzi_core::document::PhysicsTarget::Unresolved { runtime_id } => runtime_id.capacity(),
            _ => 0,
        },
        None => 0,
    }
}

#[cfg(test)]
mod binding_payload_tests;
#[cfg(test)]
mod mesh_tests;
#[cfg(test)]
mod tracking_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_id_entity_kinds_have_deterministic_summary_order() {
        let before = Document::new();
        let after = Document::new();
        let before_keys = HashSet::new();
        let mut expected = None;
        for reverse in [false, true] {
            let mut keys = [
                ObjectKey {
                    kind: ObjectKind::Part,
                    id: "shared".into(),
                },
                ObjectKey {
                    kind: ObjectKind::DrawOrderGroup,
                    id: "shared".into(),
                },
            ];
            if reverse {
                keys.reverse();
            }
            let summary = summarize_impl(
                &before,
                &after,
                &before_keys,
                &keys.into_iter().collect(),
                &[],
                &BTreeSet::new(),
                false,
            );
            let json = serde_json::to_value(summary).unwrap();
            if let Some(expected) = &expected {
                assert_eq!(&json, expected);
            }
            expected = Some(json);
        }
    }

    #[test]
    fn summary_estimate_charges_spare_slots_and_occupied_entity_payloads() {
        let mut summary = ChangeSet::default();
        let baseline = summary.estimated_bytes();
        summary.created.reserve_exact(8);
        summary.reference_fields.reserve_exact(3);
        let slots = summary.created.capacity() * std::mem::size_of::<EntityRef>()
            + summary.reference_fields.capacity() * std::mem::size_of::<ReferenceFieldChange>();
        assert_eq!(summary.estimated_bytes(), baseline + slots);

        let entity = EntityRef {
            document_id: String::with_capacity(80),
            kind: ObjectKind::Mesh,
            id: String::with_capacity(120),
        };
        let payload = entity.document_id.capacity() + entity.id.capacity();
        summary.created.push(entity);
        assert_eq!(summary.estimated_bytes(), baseline + slots + payload);
    }
    #[test]
    fn pruned_summaries_match_full_summaries_across_domains_and_history() {
        use crate::{AuthoringSession, EditSession, SdkError};
        use zipzi_core::{Mesh, ModelSetup, Parameter, Part};
        fn id(n: u32) -> String {
            format!("00000000-0000-4000-8000-{n:012x}")
        }
        fn check(
            sdk: &mut AuthoringSession,
            edit: impl FnOnce(&mut EditSession<'_>) -> Result<(), SdkError>,
        ) {
            let before = sdk.document().clone();
            let before_keys = crate::session::keys_in(&before);
            let mut transaction = sdk.begin_owned_edit("comparison", None).unwrap();
            edit(&mut transaction).unwrap();
            let after = transaction.candidate_document();
            let after_keys = crate::session::keys_in(after);
            let full = summarize_impl(
                &before,
                after,
                &before_keys,
                &after_keys,
                &transaction.object_ids,
                &transaction.categories,
                false,
            );
            let receipt = transaction.commit_to(sdk).unwrap();
            assert!(receipt.changed);
            assert_eq!(
                serde_json::to_value(&receipt.changes).unwrap(),
                serde_json::to_value(&full).unwrap()
            );
            let persisted = serde_json::to_value(&receipt.changes).unwrap();
            let undo = sdk.undo().unwrap();
            assert_eq!(
                serde_json::to_value(&undo.changes).unwrap(),
                serde_json::to_value(full.inverted()).unwrap()
            );
            let redo = sdk.redo().unwrap();
            assert_eq!(serde_json::to_value(&redo.changes).unwrap(), persisted);
        }
        let mut sdk = AuthoringSession::new(&id(1), ModelSetup::default()).unwrap();
        check(&mut sdk, |e| {
            e.create_mesh(Mesh {
                id: id(2),
                name: "Mesh".into(),
                runtime_id: "Mesh".into(),
                ..Default::default()
            })
        });
        check(&mut sdk, |e| e.rename_mesh(&id(2), "Renamed"));
        check(&mut sdk, |e| e.set_mesh_opacity(&id(2), 0.4));
        check(&mut sdk, |e| {
            e.create_mesh(Mesh {
                id: id(6),
                runtime_id: "Mask".into(),
                ..Default::default()
            })
        });
        let mut masked = sdk.mesh(&id(2)).unwrap();
        masked.masks = vec![id(6)];
        check(&mut sdk, |e| e.replace_mesh(masked));
        let d = sdk.document();
        let keys = crate::session::keys_in(d);
        let ids = vec![id(2)];
        let kinds = categories(ChangeKind::Structure);
        assert_eq!(
            serde_json::to_value(summarize(d, d, &keys, &keys, &ids, &kinds)).unwrap(),
            serde_json::to_value(summarize_impl(d, d, &keys, &keys, &ids, &kinds, false)).unwrap()
        );
        check(&mut sdk, |e| {
            e.create_part(Part {
                id: id(3),
                ..Default::default()
            })
        });
        check(&mut sdk, |e| {
            e.edit_scene(zipzi_core::document::SceneEdit::Move {
                ids: vec![id(2)],
                destination: zipzi_core::document::SceneCollection::Organization {
                    part_id: Some(id(3)),
                },
                before: None,
            })
        });
        check(&mut sdk, |e| {
            e.create_parameter(Parameter {
                id: id(4),
                name: "Parameter".into(),
                minimum: 0.,
                maximum: 1.,
                ..Default::default()
            })
        });
        let mut p = sdk.parameter(&id(4)).unwrap();
        p.default_value = 0.4;
        check(&mut sdk, |e| e.replace_parameter(p));
        check(&mut sdk, |e| {
            e.create_binding(zipzi_core::MeshBinding {
                id: id(7),
                mesh_id: id(2),
                axes: vec![zipzi_core::BindingAxis {
                    parameter_id: id(4),
                    keys: vec![0., 1.],
                    ..Default::default()
                }],
                keyforms: vec![0., 1.]
                    .into_iter()
                    .map(|key| zipzi_core::MeshKeyform {
                        keys: vec![key],
                        ..Default::default()
                    })
                    .collect(),
            })
        });
        let mut binding = sdk.document().get_binding(&id(7)).unwrap().clone();
        binding.keyforms[0].appearance.opacity = 0.25;
        check(&mut sdk, |e| e.replace_binding(binding));
        let clip: zipzi_core::document::MotionClip=serde_json::from_value(serde_json::json!({
            "id":id(5),"name":"Motion","duration":1.,"fps":30.,"looping":false,"restricted_beziers":true,
            "fade_in":null,"fade_out":null,"track_order":[],"tracks":[],"events":[],
            "extensions":{},"meta_extensions":{},"opaque_source_ids":null
        })).unwrap();
        check(&mut sdk, |e| e.create_motion(clip));
        check(&mut sdk, |e| {
            e.patch_property(&zipzi_core::document::PropertyPatch::Motion {
                id: id(5),
                duration: 1.,
                fps: 30.,
                fade_in: Some(0.3),
                fade_out: None,
            })
        });
        let mut state = sdk.document().editor_state().clone();
        state.objects.insert(
            id(2),
            zipzi_core::document::ObjectEditorState {
                locked: true,
                hidden_in_editor: false,
            },
        );
        check(&mut sdk, |e| e.replace_editor_state(state));
    }
}

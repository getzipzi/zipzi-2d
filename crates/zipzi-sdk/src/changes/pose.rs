//! Borrowed records avoid repeated hierarchy searches and row cloning.
use super::*;
use std::collections::BTreeMap;
use zipzi_core::document::{PoseAsset, PoseEntry, PosePartRef};
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PoseReferenceChange {
    pub entity: EntityRef,
    pub before: Option<PosePartRef>,
    pub after: Option<PosePartRef>,
}
#[derive(PartialEq)]
enum Field<'a> {
    Fade(Option<f32>),
    Name(&'a str),
    Part(&'a PosePartRef),
}
struct Record<'a> {
    kind: ObjectKind,
    owner: Option<&'a str>,
    field: Field<'a>,
    children: Vec<&'a str>,
    role: &'static str,
    entry: Option<&'a PoseEntry>,
    links_present: bool,
}
fn records(p: Option<&PoseAsset>) -> BTreeMap<&str, Record<'_>> {
    let mut out = BTreeMap::new();
    if let Some(p) = p {
        out.insert(
            p.id.as_str(),
            Record {
                kind: ObjectKind::Pose,
                owner: None,
                field: Field::Fade(p.fade_in),
                children: p.groups.iter().map(|g| g.id.as_str()).collect(),
                role: "groups",
                entry: None,
                links_present: false,
            },
        );
        for g in &p.groups {
            out.insert(
                g.id.as_str(),
                Record {
                    kind: ObjectKind::PoseGroup,
                    owner: Some(&p.id),
                    field: Field::Name(&g.name),
                    children: g.entries.iter().map(|e| e.id.as_str()).collect(),
                    role: "entries",
                    entry: None,
                    links_present: false,
                },
            );
            for e in &g.entries {
                out.insert(
                    e.id.as_str(),
                    Record {
                        kind: ObjectKind::PoseEntry,
                        owner: Some(&g.id),
                        field: Field::Part(&e.part),
                        children: e.links.iter().flatten().map(|l| l.id.as_str()).collect(),
                        role: "links",
                        entry: Some(e),
                        links_present: e.links.is_some(),
                    },
                );
                for l in e.links.iter().flatten() {
                    out.insert(
                        l.id.as_str(),
                        Record {
                            kind: ObjectKind::PoseLink,
                            owner: Some(&e.id),
                            field: Field::Part(&l.target),
                            children: vec![],
                            role: "",
                            entry: None,
                            links_present: false,
                        },
                    );
                }
            }
        }
    }
    out
}
pub(super) fn summarize_pose(before: &Document, after: &Document, out: &mut ChangeSet) {
    let a = before.pose();
    let b = after.pose();
    if a.zip(b).is_some_and(|(a, b)| std::ptr::eq(a, b)) || a == b {
        return;
    }
    let entity = |kind, id: &str| EntityRef::new(after.id(), kind, id);
    let left = records(a);
    let right = records(b);
    for (id, old, new) in paired_records(&left, &right) {
        let current = new.or(old).unwrap();
        let old_children = old.map(|r| r.children.as_slice()).unwrap_or_default();
        let new_children = new.map(|r| r.children.as_slice()).unwrap_or_default();
        if old_children != new_children && !current.role.is_empty() {
            out.collections.push(CollectionRef::Pose {
                document_id: after.id().into(),
                owner: Some((*id).into()),
                role: current.role.into(),
            });
            out.categories
                .extend([ChangeCategory::Order, ChangeCategory::EvaluationInputs]);
        }
        let fields_changed = old.map(|r| &r.field) != new.map(|r| &r.field);
        let moved = old.map(|r| r.owner) != new.map(|r| r.owner);
        let content = old.map(|r| (r.entry.map(|e| &e.extensions), r.links_present))
            != new.map(|r| (r.entry.map(|e| &e.extensions), r.links_present));
        if fields_changed || moved || content {
            if new.is_some() {
                out.modified.push(entity(current.kind, id));
            }
            if fields_changed {
                if current.kind == ObjectKind::PoseGroup && old.is_some() && new.is_some() {
                    out.categories.insert(ChangeCategory::Labels);
                } else {
                    out.categories.insert(ChangeCategory::EvaluationInputs);
                }
            }
            if moved {
                out.categories.insert(ChangeCategory::References);
            }
            if content {
                out.categories.insert(ChangeCategory::Content);
                if old.and_then(|r| r.entry.map(|e| &e.extensions))
                    != new.and_then(|r| r.entry.map(|e| &e.extensions))
                {
                    out.categories.insert(ChangeCategory::EvaluationInputs);
                }
            }
        }
        let reference = |r: Option<&Record>| match r {
            Some(Record {
                field: Field::Part(p),
                ..
            }) => Some((*p).clone()),
            _ => None,
        };
        let old_ref = reference(old);
        let new_ref = reference(new);
        if old_ref != new_ref {
            out.pose_references.push(PoseReferenceChange {
                entity: entity(current.kind, id),
                before: old_ref,
                after: new_ref,
            });
            out.categories.insert(ChangeCategory::References);
        }
    }
    if a.map(|p| {
        (
            &p.file_type,
            &p.extensions,
            &p.opaque_source_ids,
            &p.opaque_source_content_hash,
        )
    }) != b.map(|p| {
        (
            &p.file_type,
            &p.extensions,
            &p.opaque_source_ids,
            &p.opaque_source_content_hash,
        )
    }) {
        out.categories
            .extend([ChangeCategory::Content, ChangeCategory::EvaluationInputs]);
    }
    // Root payload describes the singleton even when only owned content changed.
    if let Some(p) = b {
        out.modified.push(entity(ObjectKind::Pose, &p.id));
    }
    out.retain_existing_modified();
}
pub(super) fn reference_bytes(target: &Option<PosePartRef>) -> usize {
    match target {
        Some(PosePartRef::Resolved { part_id }) => part_id.capacity(),
        Some(PosePartRef::Unresolved { runtime_id }) => runtime_id.capacity(),
        _ => 0,
    }
}

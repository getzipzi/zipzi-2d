use super::*;
use kasane_core::document::{AttachmentRef, MotionTrackTarget};
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MotionReferenceValue {
    Clip { id: String },
    Track { target: MotionTrackTarget },
    Attachment { reference: AttachmentRef },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MotionReferenceChange {
    pub entity: EntityRef,
    pub field: String,
    pub before: Option<MotionReferenceValue>,
    pub after: Option<MotionReferenceValue>,
}
pub(super) fn summarize_motion(before: &Document, after: &Document, out: &mut ChangeSet) {
    let entity = |kind, id: &str| EntityRef::new(after.id(), kind, id);
    let collection = |owner: Option<String>, role: &str| CollectionRef::Motion {
        document_id: after.id().into(),
        owner,
        role: role.into(),
    };
    let mut modified = |kind, id: &str| out.modified.push(entity(kind, id));
    if before
        .motion_groups()
        .iter()
        .map(|g| &g.id)
        .collect::<Vec<_>>()
        != after
            .motion_groups()
            .iter()
            .map(|g| &g.id)
            .collect::<Vec<_>>()
    {
        out.collections.push(collection(None, "groups"));
        out.categories.insert(ChangeCategory::Order);
    }
    let groups: BTreeSet<_> = before
        .motion_groups()
        .iter()
        .chain(after.motion_groups())
        .map(|g| &g.id)
        .collect();
    for id in groups {
        let a = before.get_motion_group(id);
        let b = after.get_motion_group(id);
        if a == b {
            continue;
        }
        if a.is_some() && b.is_some() {
            modified(ObjectKind::MotionGroup, id);
        }
        if a.map(|g| &g.name) != b.map(|g| &g.name) {
            out.categories.insert(ChangeCategory::Labels);
        }
        if a.map(|g| g.entries.iter().map(|e| &e.id).collect::<Vec<_>>())
            != b.map(|g| g.entries.iter().map(|e| &e.id).collect::<Vec<_>>())
        {
            out.collections
                .push(collection(Some(id.clone()), "registrations"));
            out.categories.insert(ChangeCategory::Order);
        }
    }
    if before
        .motion_roots()
        .iter()
        .map(|e| &e.id)
        .collect::<Vec<_>>()
        != after
            .motion_roots()
            .iter()
            .map(|e| &e.id)
            .collect::<Vec<_>>()
    {
        out.collections.push(collection(None, "registrations"));
        out.categories.insert(ChangeCategory::Order);
    }
    let entries: BTreeSet<_> = before
        .motion_registrations()
        .chain(after.motion_registrations())
        .map(|e| &e.id)
        .collect();
    for id in entries {
        let a = before.get_motion_registration(id);
        let b = after.get_motion_registration(id);
        if a == b {
            continue;
        }
        if a.is_some() && b.is_some() {
            modified(ObjectKind::MotionRegistration, id);
        }
        // Location changes only affect the library, never evaluation.
        let old = a.as_ref().map(|(_, e)| *e);
        let new = b.as_ref().map(|(_, e)| *e);
        if old.zip(new).is_some_and(|(old, new)| {
            (&old.clip_id, old.fade_in, old.fade_out) != (&new.clip_id, new.fade_in, new.fade_out)
        }) {
            out.categories.insert(ChangeCategory::EvaluationInputs);
        }
        if old.map(|e| &e.extensions) != new.map(|e| &e.extensions) {
            out.categories.insert(ChangeCategory::Content);
        }
        for (field, a, b) in [
            (
                "clip",
                old.map(|e| MotionReferenceValue::Clip {
                    id: e.clip_id.clone(),
                }),
                new.map(|e| MotionReferenceValue::Clip {
                    id: e.clip_id.clone(),
                }),
            ),
            (
                "sound",
                old.and_then(|e| e.sound.clone())
                    .map(|reference| MotionReferenceValue::Attachment { reference }),
                new.and_then(|e| e.sound.clone())
                    .map(|reference| MotionReferenceValue::Attachment { reference }),
            ),
        ] {
            if a != b {
                out.categories.insert(ChangeCategory::References);
                out.motion_references.push(MotionReferenceChange {
                    entity: entity(ObjectKind::MotionRegistration, id),
                    field: field.into(),
                    before: a,
                    after: b,
                });
            }
        }
    }
    if before.motion_order() != after.motion_order() {
        out.collections.push(collection(None, "clips"));
        out.categories.insert(ChangeCategory::Order);
    }
    let clips: BTreeSet<_> = before
        .motion_order()
        .iter()
        .chain(after.motion_order())
        .collect();
    for id in clips {
        let a = before.get_motion(id);
        let b = after.get_motion(id);
        if a == b {
            continue;
        }
        if a.is_some() && b.is_some() {
            modified(ObjectKind::Motion, id);
        }
        if a.map(|c| &c.name) != b.map(|c| &c.name) {
            out.categories.insert(ChangeCategory::Labels);
        }
        if a.map(|c| &c.track_order) != b.map(|c| &c.track_order) {
            out.collections
                .push(collection(Some(id.clone()), "track_display_order"));
            out.categories.insert(ChangeCategory::Order);
        }
        if a.map(|c| {
            (
                c.duration,
                c.fps,
                c.looping,
                c.restricted_beziers,
                c.fade_in,
                c.fade_out,
                &c.tracks,
                &c.events,
            )
        }) != b.map(|c| {
            (
                c.duration,
                c.fps,
                c.looping,
                c.restricted_beziers,
                c.fade_in,
                c.fade_out,
                &c.tracks,
                &c.events,
            )
        }) {
            out.categories.insert(ChangeCategory::EvaluationInputs);
        }
        if a.map(|c| {
            (
                &c.extensions,
                &c.meta_extensions,
                &c.opaque_source_ids,
                &c.opaque_source_content_hash,
            )
        }) != b.map(|c| {
            (
                &c.extensions,
                &c.meta_extensions,
                &c.opaque_source_ids,
                &c.opaque_source_content_hash,
            )
        }) {
            out.categories.insert(ChangeCategory::Content);
        }
        if a.map(|c| c.tracks.iter().map(|t| &t.id).collect::<Vec<_>>())
            != b.map(|c| c.tracks.iter().map(|t| &t.id).collect::<Vec<_>>())
        {
            out.collections.push(collection(Some(id.clone()), "tracks"));
            out.categories.insert(ChangeCategory::Order);
        }
        if a.map(|c| c.events.iter().map(|e| &e.id).collect::<Vec<_>>())
            != b.map(|c| c.events.iter().map(|e| &e.id).collect::<Vec<_>>())
        {
            out.collections.push(collection(Some(id.clone()), "events"));
            out.categories.insert(ChangeCategory::Order);
        }
        let tracks: BTreeSet<_> = a
            .into_iter()
            .chain(b)
            .flat_map(|c| c.tracks.iter().map(|t| &t.id))
            .collect();
        for tid in tracks {
            let old = before.get_motion_track(tid);
            let new = after.get_motion_track(tid);
            if old.map(|(c, t)| (&c.id, t)) == new.map(|(c, t)| (&c.id, t)) {
                continue;
            }
            if old.is_some() && new.is_some() {
                modified(ObjectKind::MotionTrack, tid);
            }
            let x = old.map(|(_, t)| MotionReferenceValue::Track {
                target: t.target.clone(),
            });
            let y = new.map(|(_, t)| MotionReferenceValue::Track {
                target: t.target.clone(),
            });
            if x != y {
                out.categories.insert(ChangeCategory::References);
                out.motion_references.push(MotionReferenceChange {
                    entity: entity(ObjectKind::MotionTrack, tid),
                    field: "target".into(),
                    before: x,
                    after: y,
                });
            }
            if old.map(|(_, t)| t.keys().map(|k| &k.id).collect::<Vec<_>>())
                != new.map(|(_, t)| t.keys().map(|k| &k.id).collect::<Vec<_>>())
            {
                out.collections.push(collection(Some(tid.clone()), "keys"));
                out.categories.insert(ChangeCategory::Order);
            }
            let keys: BTreeSet<_> = old
                .into_iter()
                .chain(new)
                .flat_map(|(_, t)| t.keys().map(|k| &k.id))
                .collect();
            for kid in keys {
                let x = before.get_motion_key(kid);
                let y = after.get_motion_key(kid);
                if x.is_some()
                    && y.is_some()
                    && x.map(|(c, t, k)| (&c.id, &t.id, k)) != y.map(|(c, t, k)| (&c.id, &t.id, k))
                {
                    modified(ObjectKind::MotionKey, kid);
                }
            }
        }
        let events: BTreeSet<_> = a
            .into_iter()
            .chain(b)
            .flat_map(|c| c.events.iter().map(|e| &e.id))
            .collect();
        for eid in events {
            let x = before.get_motion_event(eid);
            let y = after.get_motion_event(eid);
            if x.is_some()
                && y.is_some()
                && x.map(|(c, e)| (&c.id, e)) != y.map(|(c, e)| (&c.id, e))
            {
                modified(ObjectKind::MotionEvent, eid);
            }
        }
    }
    if before
        .package_attachments()
        .iter()
        .map(|a| &a.id)
        .collect::<Vec<_>>()
        != after
            .package_attachments()
            .iter()
            .map(|a| &a.id)
            .collect::<Vec<_>>()
    {
        out.collections.push(collection(None, "attachments"));
        out.categories.insert(ChangeCategory::Order);
    }
    let ids: BTreeSet<_> = before
        .package_attachments()
        .iter()
        .chain(after.package_attachments())
        .map(|a| &a.id)
        .collect();
    for id in ids {
        let a = before.get_attachment(id);
        let b = after.get_attachment(id);
        if a == b {
            continue;
        }
        if a.is_some() && b.is_some() {
            modified(ObjectKind::Attachment, id);
        }
        if a.map(|a| &a.path) != b.map(|a| &a.path) {
            out.categories.insert(ChangeCategory::Labels);
        }
        if a.map(|a| &a.bytes) != b.map(|a| &a.bytes) {
            out.categories.insert(ChangeCategory::Content);
            out.resource_content
                .push(entity(ObjectKind::Attachment, id));
        }
    }
    let x = before
        .model3_settings()
        .user_data
        .clone()
        .map(|reference| MotionReferenceValue::Attachment { reference });
    let y = after
        .model3_settings()
        .user_data
        .clone()
        .map(|reference| MotionReferenceValue::Attachment { reference });
    if x != y {
        out.categories.insert(ChangeCategory::References);
        out.motion_references.push(MotionReferenceChange {
            entity: entity(ObjectKind::Attachment, after.id()),
            field: "user_data".into(),
            before: x,
            after: y,
        });
    }
}

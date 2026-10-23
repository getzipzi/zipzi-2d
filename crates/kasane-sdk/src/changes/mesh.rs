//! Publication for proven Mesh scalar fields and MeshBinding keyform payloads.
//! No organization collections, sibling bindings or global key sets are read.
use super::*;
use kasane_core::document::{SceneField, TrackedRecord};

pub(crate) struct MeshPublication<'a> {
    before: &'a Document,
    after: &'a Document,
    records: &'a BTreeSet<TrackedRecord>,
    object_ids: &'a [String],
    notifications: BTreeMap<&'a str, ObjectKind>,
}

impl<'a> MeshPublication<'a> {
    pub(crate) fn new(
        before: &'a Document,
        after: &'a Document,
        object_ids: &'a [String],
        erased_keys: &HashSet<ObjectKey>,
    ) -> Option<Self> {
        let result = Self::prove(before, after, object_ids, erased_keys);
        #[cfg(feature = "bench-instrumentation")]
        match &result {
            Ok(publication) => {
                kasane_core::benchmark::count("sdk.publication.local", 1);
                kasane_core::benchmark::count(
                    "sdk.publication.touched_records",
                    publication.records.len(),
                );
            }
            Err(reason) => {
                kasane_core::benchmark::count("sdk.publication.fallback", 1);
                kasane_core::benchmark::count(reason, 1);
            }
        }
        result.ok()
    }

    fn prove(
        before: &'a Document,
        after: &'a Document,
        object_ids: &'a [String],
        erased_keys: &HashSet<ObjectKey>,
    ) -> Result<Self, &'static str> {
        if !erased_keys.is_empty() {
            return Err("sdk.publication.fallback.retired_identities");
        }
        let records = after
            .tracked_records_since(before)
            .ok_or("sdk.publication.fallback.unknown_coverage")?;
        let mut notifications = BTreeMap::new();
        for id in object_ids {
            let id = id.as_str();
            if notifications.contains_key(id) {
                continue;
            }
            #[cfg(feature = "bench-instrumentation")]
            kasane_core::benchmark::count("sdk.mesh_summary.notification_reads", 2);
            let meshes = (before.get_mesh(id), after.get_mesh(id));
            if meshes.0.is_some() && meshes.1.is_some() {
                notifications.insert(id, ObjectKind::Mesh);
                continue;
            }
            #[cfg(feature = "bench-instrumentation")]
            kasane_core::benchmark::count("sdk.binding_summary.notification_reads", 2);
            let bindings = (before.get_binding(id), after.get_binding(id));
            if bindings.0.is_none() || bindings.1.is_none() {
                return Err("sdk.publication.fallback.notification_scope");
            }
            notifications.insert(id, ObjectKind::MeshBinding);
        }
        for record in records {
            match record {
                // Core checked both snapshots and the final allowed field scope.
                // New record kinds must explicitly establish lifecycle safety.
                TrackedRecord::Mesh(id) | TrackedRecord::MeshBinding(id) => {
                    if !notifications.contains_key(id.as_str()) {
                        return Err("sdk.publication.fallback.notification_scope");
                    }
                }
            }
        }
        Ok(Self {
            before,
            after,
            records,
            object_ids,
            notifications,
        })
    }

    pub(crate) fn summarize(&self, kinds: &BTreeSet<ChangeCategory>) -> ChangeSet {
        #[cfg(feature = "bench-instrumentation")]
        let _span = kasane_core::benchmark::span("sdk.summarize");
        let mut out = ChangeSet::default();
        out.categories.extend(kinds.iter().copied());
        let mut derived_meshes = BTreeSet::new();
        for record in self.records {
            match record {
                TrackedRecord::Mesh(id) => {
                    #[cfg(feature = "bench-instrumentation")]
                    kasane_core::benchmark::count("sdk.mesh_summary.record_reads", 2);
                    let a = self.before.get_mesh(id).expect("proven existing Mesh");
                    let b = self.after.get_mesh(id).expect("proven existing Mesh");
                    for (field, changed) in [
                        (SceneField::Label, a.name != b.name),
                        (SceneField::Enabled, a.enabled != b.enabled),
                        (SceneField::Appearance, a.appearance != b.appearance),
                    ] {
                        if changed {
                            scene::record_field_change(
                                &mut out,
                                EntityRef::new(self.after.id(), ObjectKind::Mesh, id),
                                field,
                            );
                        }
                    }
                }
                TrackedRecord::MeshBinding(id) => {
                    #[cfg(feature = "bench-instrumentation")]
                    kasane_core::benchmark::count("sdk.binding_summary.record_reads", 2);
                    let a = self
                        .before
                        .get_binding(id)
                        .expect("proven existing MeshBinding");
                    let b = self
                        .after
                        .get_binding(id)
                        .expect("proven existing MeshBinding");
                    // Axes, coordinates, ownership and grid shape were checked
                    // in Core. Compare final payload once; do not re-read peers
                    // or reconstruct binding lookups to find the derived target.
                    if a.keyforms != b.keyforms {
                        derived_meshes.insert(b.mesh_id.as_str());
                    }
                }
            }
        }
        // Mesh-only notification scopes get Scene's Topology normalization.
        // A notified Binding retains the declaration even for a net-noop payload.
        if !self.object_ids.is_empty()
            && self
                .notifications
                .values()
                .all(|kind| *kind == ObjectKind::Mesh)
        {
            out.categories.remove(&ChangeCategory::Topology);
        }
        for id in derived_meshes {
            #[cfg(feature = "bench-instrumentation")]
            kasane_core::benchmark::count("sdk.binding_summary.derived_meshes", 1);
            content::record_content_change(
                &mut out,
                self.after.id(),
                id,
                ObjectKind::Mesh,
                kasane_core::document::SceneContentDomain::MeshTopology,
                false,
            );
        }
        // Preserve notifications for a touched Mesh restored to its baseline
        // when another Mesh still changes. Touched records do not replace IDs.
        out.modified.extend(
            self.object_ids
                .iter()
                .map(|id| EntityRef::new(self.after.id(), self.notifications[id.as_str()], id)),
        );
        finish_summary(&mut out);
        out
    }
}

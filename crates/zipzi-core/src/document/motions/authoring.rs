//! Motion library relationships are domain data, independent of playback state.
use super::*;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum MotionContainer {
    Root,
    Group(
        /// Destination motion-group identity; Root selects ungrouped registrations.
        String,
    ),
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum MotionEdit {
    Create {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
        /// Stable ID allocated for the new motion library registration.
        registration_id: String,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: MotionContainer,
    },
    CreateGroup {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
    },
    RenameGroup {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
    },
    ReorderGroups {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    Register {
        /// Complete replacement motion registration; its owner is selected separately.
        entry: MotionRegistration,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: MotionContainer,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    PatchRegistration {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Exact current field values required by the optimistic edit; stale values reject it.
        expected: MotionRegistration,
        /// Complete replacement motion registration; its owner is selected separately.
        entry: MotionRegistration,
    },
    MoveRegistrations {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: MotionContainer,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    DuplicateRegistrations {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: MotionContainer,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
    },
    DeleteRegistrations {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    DissolveGroup {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
    },
    DeleteGroup {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
    },
    DuplicateGroup {
        /// Typed parameter reference or stable source object ID, as declared by the enclosing operation.
        source: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
    },
    DuplicateClip {
        /// Typed parameter reference or stable source object ID, as declared by the enclosing operation.
        source: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Human-readable display or registration name; distinct from persistent object identity.
        name: String,
        /// Stable ID allocated for the new motion library registration.
        registration_id: String,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: MotionContainer,
    },
    DeleteClip {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
    },
    ReorderTracks {
        /// Stable ID of the motion clip owning the affected track or event.
        clip: String,
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    CreateTrack {
        /// Stable ID of the motion clip owning the affected track or event.
        clip: String,
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Stable identity allocated for the initial key of a new track.
        key_id: String,
        /// Absolute time in seconds within the owning motion clip.
        time: f32,
    },
    SetTrackTarget {
        /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
        id: String,
        /// Typed target reference; unresolved imported runtime labels remain explicit.
        target: MotionTrackTarget,
    },
    MoveTracks {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: String,
        /// Optional stable sibling ID before which to insert; None appends.
        before: Option<String>,
        /// True allocates copied identities; false transfers the existing identities.
        copy: bool,
    },
    DeleteTracks {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    TransferKeys {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Stable receiving track ID for transferred keys.
        destination_track: String,
        /// Offset in seconds added when moving or copying keys/events.
        time_offset: f32,
        /// True allocates copied identities; false transfers the existing identities.
        copy: bool,
    },
    SetEvent {
        /// Stable ID of the motion clip owning the affected track or event.
        clip: String,
        /// Owned motion event with a stable ID, time in seconds and string payload.
        event: MotionEvent,
    },
    DeleteEvents {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
    },
    MoveEvents {
        /// Ordered stable IDs selected by this operation; names and row numbers are not identities.
        ids: Vec<String>,
        /// Explicit destination owner/container; order is controlled separately by before.
        destination: String,
        /// Offset in seconds added when moving or copying keys/events.
        time_offset: f32,
        /// True allocates copied identities; false transfers the existing identities.
        copy: bool,
    },
}
#[derive(Debug, Clone)]
pub struct MotionPlan {
    snapshot: Arc<()>,
    revision: u64,
    groups: Vec<MotionGroup>,
    roots: Vec<MotionRegistration>,
    clips: std::collections::HashMap<String, Arc<MotionClip>>,
    order: Vec<String>,
    affected: Vec<String>,
    migrated: Vec<String>,
}
impl MotionPlan {
    pub fn affected_ids(&self) -> &[String] {
        &self.affected
    }
    /// Last registrations retained at root, with their identity and overrides.
    pub fn migrated_to_root(&self) -> &[String] {
        &self.migrated
    }
}
impl MotionClip {
    pub fn display_tracks(&self) -> Vec<&MotionTrack> {
        if self.track_order.is_empty() {
            self.tracks.iter().collect()
        } else {
            self.track_order
                .iter()
                .filter_map(|id| self.tracks.iter().find(|t| t.id == *id))
                .collect()
        }
    }
}
impl Document {
    /// A motion-only planning workspace shares curves and attachment blobs. It
    /// never copies scene geometry, bindings or physics for a library hover.
    pub(in crate::document) fn motion_authoring_candidate(&self) -> Document {
        let mut c = Document::new();
        c.content.id = self.id().into();
        c.content.parameters = self.content.parameters.clone();
        c.content.parts = self.content.parts.clone();
        c.content.motions = self.content.motions.clone();
        c.content.motion_order = self.content.motion_order.clone();
        c.content.motion_groups = self.content.motion_groups.clone();
        c.content.motion_roots = self.content.motion_roots.clone();
        c.content.package_attachments = self.content.package_attachments.clone();
        c.content.model3_settings = self.content.model3_settings.clone();
        c.content.editor_state = self.content.editor_state.clone();
        c.motion_lookup = self.motion_lookup.clone();
        c
    }
    pub(in crate::document) fn motion_identity_exists(&self, id: &str) -> bool {
        self.get_motion(id).is_some()
            || self.get_motion_group(id).is_some()
            || self.get_motion_registration(id).is_some()
            || self.get_motion_track(id).is_some()
            || self.get_motion_key(id).is_some()
            || self.get_motion_event(id).is_some()
    }
    pub fn motion_roots(&self) -> &[MotionRegistration] {
        &self.content.motion_roots
    }
    pub fn motion_registrations(&self) -> impl Iterator<Item = &MotionRegistration> {
        self.motion_groups()
            .iter()
            .flat_map(|g| &g.entries)
            .chain(self.motion_roots())
    }
    pub fn get_motion_group(&self, id: &str) -> Option<&MotionGroup> {
        self.motion_groups().iter().find(|g| g.id == id)
    }
    pub fn get_motion_registration(
        &self,
        id: &str,
    ) -> Option<(MotionContainer, &MotionRegistration)> {
        for g in self.motion_groups() {
            if let Some(e) = g.entries.iter().find(|e| e.id == id) {
                return Some((MotionContainer::Group(g.id.clone()), e));
            }
        }
        self.motion_roots()
            .iter()
            .find(|e| e.id == id)
            .map(|e| (MotionContainer::Root, e))
    }
    fn motion_lookup(&self) -> &MotionLookup {
        self.motion_lookup.get_or_init(|| {
            let mut lookup = MotionLookup::default();
            for clip in self
                .motion_order()
                .iter()
                .filter_map(|id| self.get_motion(id))
            {
                lookup.clips.insert(clip.id.clone());
                for (slot, t) in clip.tracks.iter().enumerate() {
                    lookup.tracks.insert(t.id.clone(), (clip.id.clone(), slot));
                    for (key, k) in t.keys().enumerate() {
                        lookup
                            .keys
                            .insert(k.id.clone(), (clip.id.clone(), slot, key));
                    }
                }
                for (slot, e) in clip.events.iter().enumerate() {
                    lookup.events.insert(e.id.clone(), (clip.id.clone(), slot));
                }
            }
            Arc::new(lookup)
        })
    }
    pub fn get_motion_track(&self, id: &str) -> Option<(&MotionClip, &MotionTrack)> {
        let (clip, slot) = self.motion_lookup().tracks.get(id)?;
        let clip = self.get_motion(clip)?;
        Some((clip, &clip.tracks[*slot]))
    }
    pub fn get_motion_key(&self, id: &str) -> Option<(&MotionClip, &MotionTrack, &MotionKey)> {
        let (clip, slot, key) = self.motion_lookup().keys.get(id)?;
        let clip = self.get_motion(clip)?;
        let track = &clip.tracks[*slot];
        let key = if *key == 0 {
            &track.initial
        } else {
            track.segments[*key - 1].end()
        };
        Some((clip, track, key))
    }
    pub fn get_motion_event(&self, id: &str) -> Option<(&MotionClip, &MotionEvent)> {
        let (clip, slot) = self.motion_lookup().events.get(id)?;
        let clip = self.get_motion(clip)?;
        Some((clip, &clip.events[*slot]))
    }
    fn container_mut(
        &mut self,
        dest: &MotionContainer,
    ) -> Result<&mut Vec<MotionRegistration>, Status> {
        match dest {
            MotionContainer::Root => Ok(&mut self.content.motion_roots),
            MotionContainer::Group(id) => self
                .content
                .motion_groups
                .iter_mut()
                .find(|g| g.id == *id)
                .map(|g| &mut g.entries)
                .ok_or_else(|| Status::error("MISSING_MOTION_GROUP", id)),
        }
    }
    fn retain_registrations(&mut self, ids: &[String]) {
        self.content.motion_roots.retain(|e| !ids.contains(&e.id));
        for g in &mut self.content.motion_groups {
            g.entries.retain(|e| !ids.contains(&e.id));
        }
    }
    fn selected_registrations(&self, ids: &[String]) -> Result<Vec<MotionRegistration>, Status> {
        let entries: Vec<_> = self
            .motion_registrations()
            .filter(|e| ids.contains(&e.id))
            .cloned()
            .collect();
        if ids.is_empty() || entries.len() != ids.len() {
            return Err(Status::error(
                "INVALID_SELECTION",
                "Select distinct existing registrations",
            ));
        }
        Ok(entries)
    }
    fn insert_registrations(
        &mut self,
        dest: &MotionContainer,
        before: &Option<String>,
        entries: Vec<MotionRegistration>,
    ) -> Result<(), Status> {
        let target = self.container_mut(dest)?;
        let index = match before {
            None => target.len(),
            Some(id) => target
                .iter()
                .position(|e| e.id == *id)
                .ok_or_else(|| Status::error("INVALID_PLACEMENT", id))?,
        };
        target.splice(index..index, entries);
        Ok(())
    }
    fn remove_registrations_preserving_last(
        &mut self,
        entries: Vec<MotionRegistration>,
    ) -> Vec<String> {
        let selected: Vec<_> = entries.iter().map(|e| e.id.clone()).collect();
        let mut protected: HashSet<String> = HashSet::new();
        let mut migrate = vec![];
        for e in &entries {
            if self
                .motion_registrations()
                .filter(|r| r.clip_id == e.clip_id)
                .all(|r| selected.contains(&r.id))
                && !protected.iter().any(|id| {
                    self.get_motion_registration(id)
                        .is_some_and(|(_, r)| r.clip_id == e.clip_id)
                })
            {
                let keep = self
                    .motion_roots()
                    .iter()
                    .find(|r| r.clip_id == e.clip_id)
                    .unwrap_or(e);
                protected.insert(keep.id.clone());
                if !self.motion_roots().iter().any(|r| r.id == keep.id) {
                    migrate.push(keep.clone());
                }
            }
        }
        self.retain_registrations(
            &selected
                .into_iter()
                .filter(|id| !protected.contains(id))
                .collect::<Vec<_>>(),
        );
        self.retain_registrations(&migrate.iter().map(|e| e.id.clone()).collect::<Vec<_>>());
        let migrated = migrate.iter().map(|e| e.id.clone()).collect();
        self.content.motion_roots.extend(migrate);
        migrated
    }
    pub fn plan_motion_edit(&self, edit: MotionEdit) -> Result<MotionPlan, Status> {
        if !self.initialized() {
            return Err(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        let mut c = self.motion_authoring_candidate();
        let mut migrated = vec![];
        let mut sources = vec![];
        let copying_registration = matches!(&edit, MotionEdit::DuplicateRegistrations { .. });
        let dissolving = matches!(&edit, MotionEdit::DissolveGroup { .. });
        match edit {
            MotionEdit::Create {
                id,
                name,
                registration_id,
                destination,
            } => {
                if self.contains_id(&id)
                    || self.motion_identity_exists(&id)
                    || self.contains_id(&registration_id)
                    || self.motion_identity_exists(&registration_id)
                {
                    return Err(Status::error("DUPLICATE_ID", &id));
                }
                let clip = MotionClip {
                    id: id.clone(),
                    name,
                    duration: 2.,
                    fps: 30.,
                    looping: false,
                    restricted_beziers: false,
                    fade_in: Some(0.),
                    fade_out: Some(0.),
                    track_order: vec![],
                    tracks: Default::default(),
                    events: Default::default(),
                    extensions: Default::default(),
                    meta_extensions: Default::default(),
                    opaque_source_ids: None,
                    opaque_source_content_hash: None,
                };
                let result = c.create_motion(clip);
                if !result.status.is_ok() {
                    return Err(result.status);
                }
                let auto = c.content.motion_roots.pop().unwrap();
                c.insert_registrations(
                    &destination,
                    &None,
                    vec![MotionRegistration {
                        id: registration_id,
                        ..auto
                    }],
                )?;
            }
            MotionEdit::CreateGroup { id, name } => {
                if self.contains_id(&id) || self.motion_identity_exists(&id) {
                    return Err(Status::error("DUPLICATE_ID", id));
                }
                c.content.motion_groups.push(MotionGroup {
                    id,
                    name,
                    entries: vec![],
                });
            }
            MotionEdit::RenameGroup { id, name } => {
                c.content
                    .motion_groups
                    .iter_mut()
                    .find(|g| g.id == id)
                    .ok_or_else(|| Status::error("MISSING_MOTION_GROUP", &id))?
                    .name = name;
            }
            MotionEdit::ReorderGroups { ids } => {
                let old = c.content.motion_groups.clone();
                if ids.len() != old.len() || ids.iter().collect::<HashSet<_>>().len() != ids.len() {
                    return Err(Status::error("INVALID_ORDER", "Every group exactly once"));
                }
                c.content.motion_groups = ids
                    .iter()
                    .map(|id| {
                        old.iter()
                            .find(|g| g.id == *id)
                            .cloned()
                            .ok_or_else(|| Status::error("MISSING_MOTION_GROUP", id))
                    })
                    .collect::<Result<_, _>>()?;
            }
            MotionEdit::Register {
                entry,
                destination,
                before,
            } => {
                if self.contains_id(&entry.id) {
                    return Err(Status::error("DUPLICATE_ID", &entry.id));
                }
                c.insert_registrations(&destination, &before, vec![entry])?;
            }
            MotionEdit::PatchRegistration {
                id,
                expected,
                entry,
            } => {
                let (owner, current) = self
                    .get_motion_registration(&id)
                    .ok_or_else(|| Status::error("MISSING_MOTION_REGISTRATION", &id))?;
                if current != &expected || id != entry.id {
                    return Err(Status::error("STALE_FIELD", &id));
                }
                *c.container_mut(&owner)?
                    .iter_mut()
                    .find(|e| e.id == id)
                    .unwrap() = entry;
                // Retargeting the last registration must keep the previous clip visible.
                c.ensure_motion_roots();
            }
            MotionEdit::MoveRegistrations {
                ids,
                destination,
                before,
            }
            | MotionEdit::DuplicateRegistrations {
                ids,
                destination,
                before,
            } => {
                let copying = copying_registration;
                let mut entries = self.selected_registrations(&ids)?;
                if copying {
                    sources.extend(ids);
                    for e in &mut entries {
                        e.id = uuid::Uuid::new_v4().to_string();
                    }
                } else {
                    if before.as_ref().is_some_and(|id| ids.contains(id)) {
                        return Err(Status::error("INVALID_PLACEMENT", "Anchor is selected"));
                    }
                    c.retain_registrations(&ids);
                }
                c.insert_registrations(&destination, &before, entries)?;
            }
            MotionEdit::DeleteRegistrations { ids } => {
                migrated =
                    c.remove_registrations_preserving_last(self.selected_registrations(&ids)?);
            }
            MotionEdit::DissolveGroup { id } | MotionEdit::DeleteGroup { id } => {
                let dissolve = dissolving;
                let g = c
                    .get_motion_group(&id)
                    .cloned()
                    .ok_or_else(|| Status::error("MISSING_MOTION_GROUP", &id))?;
                if dissolve {
                    c.content.motion_roots.extend(g.entries.clone());
                    c.content.motion_groups.retain(|g| g.id != id);
                } else {
                    migrated = c.remove_registrations_preserving_last(g.entries);
                    c.content.motion_groups.retain(|g| g.id != id);
                }
            }
            MotionEdit::DuplicateGroup { source, id, name } => {
                if self.contains_id(&id) || self.motion_identity_exists(&id) {
                    return Err(Status::error("DUPLICATE_ID", id));
                }
                let mut g = self
                    .get_motion_group(&source)
                    .cloned()
                    .ok_or_else(|| Status::error("MISSING_MOTION_GROUP", &source))?;
                sources.push(source);
                sources.extend(g.entries.iter().map(|e| e.id.clone()));
                g.id = id;
                g.name = name;
                for e in &mut g.entries {
                    e.id = uuid::Uuid::new_v4().to_string();
                }
                c.content.motion_groups.push(g);
            }
            MotionEdit::DuplicateClip {
                source,
                id,
                name,
                registration_id,
                destination,
            } => {
                if self.contains_id(&id)
                    || self.motion_identity_exists(&id)
                    || self.contains_id(&registration_id)
                    || self.motion_identity_exists(&registration_id)
                {
                    return Err(Status::error("DUPLICATE_ID", &id));
                }
                let mut clip = self
                    .get_motion(&source)
                    .cloned()
                    .ok_or_else(|| Status::error("MISSING_MOTION", &source))?;
                sources.extend(clip_ids(&clip));
                sources.push(source);
                let old_order = clip.track_order.clone();
                let mut remap = BTreeMap::new();
                clip.id = id;
                clip.name = name;
                for t in &mut clip.tracks {
                    let new = uuid::Uuid::new_v4().to_string();
                    remap.insert(t.id.clone(), new.clone());
                    t.id = new;
                    t.initial.id = uuid::Uuid::new_v4().to_string();
                    for s in Arc::make_mut(&mut t.segments) {
                        s.end_mut().id = uuid::Uuid::new_v4().to_string();
                    }
                }
                clip.track_order = old_order.iter().map(|id| remap[id].clone()).collect();
                for e in &mut clip.events {
                    e.id = uuid::Uuid::new_v4().to_string();
                }
                let result = c.create_motion(clip);
                if !result.status.is_ok() {
                    return Err(result.status);
                }
                let auto = c.content.motion_roots.pop().unwrap();
                c.insert_registrations(
                    &destination,
                    &None,
                    vec![MotionRegistration {
                        id: registration_id,
                        ..auto
                    }],
                )?;
            }
            MotionEdit::DeleteClip { id } => {
                if self.get_motion(&id).is_none() {
                    return Err(Status::error("MISSING_MOTION", id));
                }
                c.content.motions.remove(&id);
                c.content.motion_order.retain(|i| i != &id);
                let ids = c
                    .motion_registrations()
                    .filter(|e| e.clip_id == id)
                    .map(|e| e.id.clone())
                    .collect::<Vec<_>>();
                c.retain_registrations(&ids);
            }
            MotionEdit::ReorderTracks { clip, ids } => {
                let a = Arc::make_mut(
                    c.content
                        .motions
                        .get_mut(&clip)
                        .ok_or_else(|| Status::error("MISSING_MOTION", &clip))?,
                );
                a.track_order = ids;
            }
            MotionEdit::CreateTrack {
                clip,
                id,
                key_id,
                time,
            } => {
                let a = Arc::make_mut(
                    c.content
                        .motions
                        .get_mut(&clip)
                        .ok_or_else(|| Status::error("MISSING_MOTION", &clip))?,
                );
                if !a.track_order.is_empty() {
                    a.track_order.push(id.clone());
                }
                a.tracks.push(MotionTrack {
                    id,
                    target: MotionTrackTarget::Unspecified,
                    initial: MotionKey {
                        id: key_id,
                        time,
                        value: 0.,
                    },
                    segments: Arc::new(vec![]),
                    fade_in: None,
                    fade_out: None,
                    extensions: Default::default(),
                });
            }
            MotionEdit::SetTrackTarget { id, target } => {
                let (owner, _) = self
                    .get_motion_track(&id)
                    .ok_or_else(|| Status::error("MISSING_MOTION_TRACK", &id))?;
                let a = Arc::make_mut(c.content.motions.get_mut(&owner.id).unwrap());
                a.tracks.iter_mut().find(|t| t.id == id).unwrap().target = target;
            }
            MotionEdit::MoveTracks {
                ids,
                destination,
                before,
                copy,
            } => {
                let target = self
                    .get_motion(&destination)
                    .ok_or_else(|| Status::error("MISSING_MOTION", &destination))?;
                let mut tracks = vec![];
                for clip in self
                    .motion_order()
                    .iter()
                    .filter_map(|id| self.get_motion(id))
                {
                    for t in &clip.tracks {
                        if ids.contains(&t.id) {
                            if clip.restricted_beziers != target.restricted_beziers
                                && t.segments
                                    .iter()
                                    .any(|s| matches!(s, MotionSegment::Bezier { .. }))
                            {
                                return Err(Status::error(
                                    "INCOMPATIBLE_BEZIER_MODE",
                                    "Track Beziers require the same runtime mode",
                                ));
                            }
                            tracks.push(t.clone());
                        }
                    }
                }
                if ids.is_empty() || tracks.len() != ids.len() {
                    return Err(Status::error("INVALID_SELECTION", "Select distinct tracks"));
                }
                let source_ids: Vec<_> = tracks.iter().map(|t| t.id.clone()).collect();
                sources.extend(ids.clone());
                sources.extend(tracks.iter().flat_map(|t| t.keys().map(|k| k.id.clone())));
                if copy {
                    for t in &mut tracks {
                        t.id = uuid::Uuid::new_v4().to_string();
                        t.initial.id = uuid::Uuid::new_v4().to_string();
                        for s in Arc::make_mut(&mut t.segments) {
                            s.end_mut().id = uuid::Uuid::new_v4().to_string();
                        }
                    }
                } else {
                    for a in c.content.motions.values_mut().filter(|a| {
                        a.id != destination && a.tracks.iter().any(|t| ids.contains(&t.id))
                    }) {
                        let a = Arc::make_mut(a);
                        a.tracks.retain(|t| !ids.contains(&t.id));
                        a.track_order.retain(|id| !ids.contains(id));
                    }
                }
                let a = Arc::make_mut(
                    c.content
                        .motions
                        .get_mut(&destination)
                        .ok_or_else(|| Status::error("MISSING_MOTION", &destination))?,
                );
                let mut order = a
                    .display_tracks()
                    .iter()
                    .filter(|t| copy || !ids.contains(&t.id))
                    .map(|t| t.id.clone())
                    .collect::<Vec<_>>();
                let pos = match before {
                    None => order.len(),
                    Some(id) => order
                        .iter()
                        .position(|i| i == &id)
                        .ok_or_else(|| Status::error("INVALID_PLACEMENT", id))?,
                };
                let display_order = self
                    .motion_order()
                    .iter()
                    .filter_map(|id| self.get_motion(id))
                    .flat_map(|clip| clip.display_tracks())
                    .filter(|t| ids.contains(&t.id))
                    .map(|t| {
                        tracks[source_ids.iter().position(|id| id == &t.id).unwrap()]
                            .id
                            .clone()
                    });
                order.splice(pos..pos, display_order);
                a.track_order = order;
                let existing: HashSet<_> = a.tracks.iter().map(|t| t.id.clone()).collect();
                a.tracks
                    .extend(tracks.into_iter().filter(|t| !existing.contains(&t.id)));
            }
            MotionEdit::DeleteTracks { ids } => {
                for id in &ids {
                    if self.get_motion_track(id).is_none() {
                        return Err(Status::error("MISSING_MOTION_TRACK", id));
                    }
                }
                for a in c
                    .content
                    .motions
                    .values_mut()
                    .filter(|a| a.tracks.iter().any(|t| ids.contains(&t.id)))
                {
                    let a = Arc::make_mut(a);
                    a.tracks.retain(|t| !ids.contains(&t.id));
                    a.track_order.retain(|id| !ids.contains(id));
                }
            }
            MotionEdit::TransferKeys {
                ids,
                destination_track,
                time_offset,
                copy,
            } => {
                if !time_offset.is_finite() {
                    return Err(Status::error(
                        "INVALID_MOTION_POINT",
                        "Time offset must be finite",
                    ));
                }
                let (destination_clip, dest) = self
                    .get_motion_track(&destination_track)
                    .ok_or_else(|| Status::error("MISSING_MOTION_TRACK", &destination_track))?;
                let mut moving = vec![];
                let mut internal = BTreeMap::new();
                let mut remap = BTreeMap::new();
                for id in &ids {
                    let (_, _, key) = self
                        .get_motion_key(id)
                        .ok_or_else(|| Status::error("MISSING_MOTION_KEY", id))?;
                    if remap
                        .insert(
                            id.clone(),
                            if copy {
                                uuid::Uuid::new_v4().to_string()
                            } else {
                                key.id.clone()
                            },
                        )
                        .is_some()
                    {
                        return Err(Status::error("INVALID_SELECTION", "Select distinct keys"));
                    }
                }
                if ids.is_empty() {
                    return Err(Status::error("INVALID_SELECTION", "Select keys"));
                }
                for clip in self
                    .motion_order()
                    .iter()
                    .filter_map(|id| self.get_motion(id))
                {
                    for t in &clip.tracks {
                        let selected: Vec<_> = t.keys().filter(|k| ids.contains(&k.id)).collect();
                        if selected.is_empty() {
                            continue;
                        }
                        let compatible = matches!(
                            (&t.target, &dest.target),
                            (
                                MotionTrackTarget::Parameter { .. },
                                MotionTrackTarget::Parameter { .. }
                            ) | (
                                MotionTrackTarget::PartOpacity { .. },
                                MotionTrackTarget::PartOpacity { .. }
                            )
                        ) || matches!((&t.target,&dest.target),(MotionTrackTarget::Model{runtime_id:a},MotionTrackTarget::Model{runtime_id:b}) if a==b);
                        if !compatible {
                            return Err(Status::error(
                                "INCOMPATIBLE_MOTION_TARGET",
                                "Configure compatible source and destination tracks",
                            ));
                        }
                        sources.push(t.id.clone());
                        sources.extend(selected.iter().map(|k| k.id.clone()));
                        let all: Vec<_> = t.keys().collect();
                        for k in selected {
                            let mut k = k.clone();
                            k.id = remap[&k.id].clone();
                            k.time += time_offset;
                            moving.push(k);
                        }
                        for (i, segment) in t.segments.iter().enumerate() {
                            let left = &all[i].id;
                            let right = &all[i + 1].id;
                            if ids.contains(left) && ids.contains(right) {
                                if clip.restricted_beziers != destination_clip.restricted_beziers
                                    && matches!(segment, MotionSegment::Bezier { .. })
                                {
                                    return Err(Status::error(
                                        "INCOMPATIBLE_BEZIER_MODE",
                                        "Internal Beziers require the same runtime mode",
                                    ));
                                }
                                let mut segment = segment.clone();
                                segment.end_mut().id = remap[right].clone();
                                segment.end_mut().time += time_offset;
                                if let MotionSegment::Bezier {
                                    control1, control2, ..
                                } = &mut segment
                                {
                                    control1.time += time_offset;
                                    control2.time += time_offset;
                                }
                                internal
                                    .insert((remap[left].clone(), remap[right].clone()), segment);
                            }
                        }
                    }
                }
                if !copy {
                    for a in c.content.motions.values_mut().filter(|a| {
                        a.tracks
                            .iter()
                            .any(|t| t.keys().any(|k| ids.contains(&k.id)))
                    }) {
                        let a = Arc::make_mut(a);
                        let mut empty = HashSet::new();
                        for t in &mut a.tracks {
                            if !t.keys().any(|k| ids.contains(&k.id)) {
                                continue;
                            }
                            let keys = t
                                .keys()
                                .filter(|k| !ids.contains(&k.id))
                                .cloned()
                                .collect::<Vec<_>>();
                            if keys.is_empty() {
                                empty.insert(t.id.clone());
                            } else {
                                rebuild_key_connections(t, keys, &BTreeMap::new())?;
                            }
                        }
                        a.tracks
                            .retain(|t| !empty.contains(&t.id) || t.id == destination_track);
                        a.track_order
                            .retain(|id| !empty.contains(id) || *id == destination_track);
                    }
                }
                let a = Arc::make_mut(c.content.motions.get_mut(&destination_clip.id).unwrap());
                let t = a
                    .tracks
                    .iter_mut()
                    .find(|t| t.id == destination_track)
                    .unwrap();
                let mut keys = if !copy {
                    dest.keys()
                        .filter(|k| !ids.contains(&k.id))
                        .cloned()
                        .collect::<Vec<_>>()
                } else {
                    t.keys().cloned().collect::<Vec<_>>()
                };
                keys.extend(moving);
                keys.sort_by(|a, b| a.time.total_cmp(&b.time));
                rebuild_key_connections(t, keys, &internal)?;
            }
            MotionEdit::SetEvent { clip, event } => {
                let a = Arc::make_mut(
                    c.content
                        .motions
                        .get_mut(&clip)
                        .ok_or_else(|| Status::error("MISSING_MOTION", &clip))?,
                );
                if let Some(e) = a.events.iter_mut().find(|e| e.id == event.id) {
                    *e = event;
                } else {
                    a.events.push(event);
                }
                a.events.sort_by(|a, b| a.time.total_cmp(&b.time));
            }
            MotionEdit::DeleteEvents { ids } => {
                for id in &ids {
                    if self.get_motion_event(id).is_none() {
                        return Err(Status::error("MISSING_MOTION_EVENT", id));
                    }
                }
                for a in c
                    .content
                    .motions
                    .values_mut()
                    .filter(|a| a.events.iter().any(|e| ids.contains(&e.id)))
                {
                    Arc::make_mut(a).events.retain(|e| !ids.contains(&e.id));
                }
            }
            MotionEdit::MoveEvents {
                ids,
                destination,
                time_offset,
                copy,
            } => {
                let mut events = vec![];
                for a in self
                    .motion_order()
                    .iter()
                    .filter_map(|id| self.get_motion(id))
                {
                    for e in &a.events {
                        if ids.contains(&e.id) {
                            events.push(e.clone());
                        }
                    }
                }
                if ids.is_empty() || events.len() != ids.len() {
                    return Err(Status::error("INVALID_SELECTION", "Select distinct events"));
                }
                if copy {
                    sources.extend(ids.clone());
                    for e in &mut events {
                        e.id = uuid::Uuid::new_v4().to_string();
                    }
                } else {
                    for a in c
                        .content
                        .motions
                        .values_mut()
                        .filter(|a| a.events.iter().any(|e| ids.contains(&e.id)))
                    {
                        Arc::make_mut(a).events.retain(|e| !ids.contains(&e.id));
                    }
                }
                for e in &mut events {
                    e.time += time_offset;
                }
                let a = Arc::make_mut(
                    c.content
                        .motions
                        .get_mut(&destination)
                        .ok_or_else(|| Status::error("MISSING_MOTION", &destination))?,
                );
                a.events.extend(events);
                a.events.sort_by(|a, b| a.time.total_cmp(&b.time));
            }
        }
        let clips_changed = self.content.motions != c.content.motions;
        if clips_changed {
            c.motion_lookup.take();
        }
        let mut final_ids = HashSet::new();
        let content_ids: Vec<_> = if clips_changed {
            c.motion_order()
                .iter()
                .cloned()
                .chain(
                    c.motion_order()
                        .iter()
                        .flat_map(|id| clip_ids(c.get_motion(id).unwrap())),
                )
                .collect()
        } else {
            vec![]
        };
        for id in content_ids
            .into_iter()
            .chain(c.motion_groups().iter().map(|g| g.id.clone()))
            .chain(c.motion_registrations().map(|e| e.id.clone()))
        {
            if !final_ids.insert(id.clone())
                || (!self.motion_identity_exists(&id) && self.contains_id(&id))
            {
                return Err(Status::error("DUPLICATE_ID", id));
            }
        }
        let status = c.validate_motion_groups(c.motion_groups());
        if !status.is_ok() {
            return Err(status);
        }
        for id in c
            .motion_order()
            .iter()
            .filter(|id| self.get_motion(id) != c.get_motion(id))
        {
            let status = c.validate_motion(c.get_motion(id).unwrap());
            if !status.is_ok() {
                return Err(status);
            }
        }
        self.validate_motion_changes_unlocked(&c)?;
        for id in sources {
            self.motion_unlocked(&id)?;
        }
        let mut affected = vec![];
        for id in self.motion_order().iter().chain(c.motion_order()) {
            if self.get_motion(id) != c.get_motion(id) {
                affected.push(id.clone());
            }
        }
        for g in self.motion_groups().iter().chain(c.motion_groups()) {
            if self.get_motion_group(&g.id) != c.get_motion_group(&g.id)
                || self.motion_groups().iter().position(|a| a.id == g.id)
                    != c.motion_groups().iter().position(|a| a.id == g.id)
            {
                affected.push(g.id.clone());
            }
        }
        for e in self.motion_registrations().chain(c.motion_registrations()) {
            if self.get_motion_registration(&e.id) != c.get_motion_registration(&e.id)
                || self.motion_registration_position(&e.id) != c.motion_registration_position(&e.id)
            {
                affected.push(e.id.clone());
            }
        }
        if clips_changed {
            for (id, _) in self
                .motion_lookup()
                .tracks
                .iter()
                .chain(c.motion_lookup().tracks.iter())
            {
                if self.get_motion_track(id).map(|(a, t)| (&a.id, t))
                    != c.get_motion_track(id).map(|(a, t)| (&a.id, t))
                {
                    affected.push(id.clone());
                }
            }
            for (id, _) in self
                .motion_lookup()
                .keys
                .iter()
                .chain(c.motion_lookup().keys.iter())
            {
                if self.get_motion_key(id).map(|(a, t, k)| (&a.id, &t.id, k))
                    != c.get_motion_key(id).map(|(a, t, k)| (&a.id, &t.id, k))
                {
                    affected.push(id.clone());
                }
            }
            for (id, _) in self
                .motion_lookup()
                .events
                .iter()
                .chain(c.motion_lookup().events.iter())
            {
                if self.get_motion_event(id).map(|(a, e)| (&a.id, e))
                    != c.get_motion_event(id).map(|(a, e)| (&a.id, e))
                {
                    affected.push(id.clone());
                }
            }
        }
        affected.sort();
        affected.dedup();
        Ok(MotionPlan {
            snapshot: self.snapshot_identity.clone(),
            revision: self.revision(),
            groups: c.content.motion_groups,
            roots: c.content.motion_roots,
            clips: c.content.motions,
            order: c.content.motion_order,
            affected,
            migrated,
        })
    }
    pub fn apply_motion_edit(&mut self, plan: &MotionPlan) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !Arc::ptr_eq(&self.snapshot_identity, &plan.snapshot) || self.revision() != plan.revision
        {
            return self.failed(Status::error(
                "STALE_PLAN",
                "Replan against current document",
            ));
        }
        if self.content.motion_groups == plan.groups
            && self.content.motion_roots == plan.roots
            && self.content.motions == plan.clips
            && self.content.motion_order == plan.order
        {
            return self.failed(Status::ok());
        }
        let flagged: Vec<_> = self
            .editor_state()
            .objects
            .keys()
            .filter(|id| self.motion_identity_exists(id))
            .cloned()
            .collect();
        let clips_changed = self.content.motions != plan.clips;
        self.content.motion_groups = plan.groups.clone();
        self.content.motion_roots = plan.roots.clone();
        self.content.motions = plan.clips.clone();
        self.content.motion_order = plan.order.clone();
        if clips_changed {
            self.motion_lookup.take();
        }
        for id in flagged {
            if !self.motion_identity_exists(&id) {
                self.content.editor_state.objects.remove(&id);
            }
        }
        self.changed(ChangeKind::Metadata, vec![], plan.affected.clone())
    }
    pub(super) fn ensure_motion_roots(&mut self) {
        for id in self.content.motion_order.clone() {
            if !self.motion_registrations().any(|e| e.clip_id == id) {
                self.content.motion_roots.push(MotionRegistration {
                    id: uuid::Uuid::new_v4().to_string(),
                    clip_id: id,
                    fade_in: None,
                    fade_out: None,
                    sound: None,
                    extensions: Default::default(),
                });
            }
        }
    }
    pub fn motion_unlocked(&self, id: &str) -> Result<(), Status> {
        // Top-level resource fields have no owner context. Avoid building the
        // key index just to validate a clip fade or attachment content patch.
        if self.get_motion(id).is_some()
            || self.get_motion_group(id).is_some()
            || self.get_attachment(id).is_some()
        {
            return if self.editor_state().get(id).locked {
                Err(Status::error("OBJECT_LOCKED", id))
            } else {
                Ok(())
            };
        }
        let mut owners = vec![id.to_owned()];
        if let Some((MotionContainer::Group(group), _)) = self.get_motion_registration(id) {
            owners.push(group);
        }
        if let Some((a, _)) = self.get_motion_track(id) {
            owners.push(a.id.clone());
        }
        if let Some((a, t, _)) = self.get_motion_key(id) {
            owners.push(a.id.clone());
            owners.push(t.id.clone());
        }
        if let Some((a, _)) = self.get_motion_event(id) {
            owners.push(a.id.clone());
        }
        if let Some(id) = owners.iter().find(|id| self.editor_state().get(id).locked) {
            return Err(Status::error("OBJECT_LOCKED", id));
        }
        Ok(())
    }
    pub(in crate::document) fn validate_motion_changes_unlocked(
        &self,
        c: &Document,
    ) -> Result<(), Status> {
        for g in self.motion_groups() {
            if self.get_motion_group(&g.id) != c.get_motion_group(&g.id)
                || self.motion_groups().iter().position(|a| a.id == g.id)
                    != c.motion_groups().iter().position(|a| a.id == g.id)
            {
                self.motion_unlocked(&g.id)?;
            }
        }
        for g in c.motion_groups() {
            if self.get_motion_group(&g.id) != Some(g) {
                c.motion_unlocked(&g.id)?;
            }
        }
        for e in self.motion_registrations() {
            if self.get_motion_registration(&e.id) != c.get_motion_registration(&e.id)
                || self.motion_registration_position(&e.id) != c.motion_registration_position(&e.id)
            {
                self.motion_unlocked(&e.id)?;
            }
        }
        for e in c.motion_registrations() {
            if self.get_motion_registration(&e.id) != c.get_motion_registration(&e.id) {
                c.motion_unlocked(&e.id)?;
            }
        }
        for id in self.motion_order() {
            if let Some(clip) = c.get_motion(id) {
                self.validate_motion_unlocked(clip)?;
            } else {
                self.motion_unlocked(id)?;
                for child in clip_ids(self.get_motion(id).unwrap()) {
                    self.motion_unlocked(&child)?;
                }
            }
        }
        Ok(())
    }
    pub(super) fn validate_motion_unlocked(&self, clip: &MotionClip) -> Result<(), Status> {
        if self.get_motion(&clip.id) == Some(clip) {
            return Ok(());
        }
        self.motion_unlocked(&clip.id)?;
        if let Some(old) = self.get_motion(&clip.id) {
            let order: std::collections::HashMap<_, _> = clip
                .display_tracks()
                .into_iter()
                .enumerate()
                .map(|(index, t)| (&t.id, index))
                .collect();
            for (index, track) in old.display_tracks().into_iter().enumerate() {
                if order.get(&track.id) != Some(&index) {
                    self.motion_unlocked(&track.id)?;
                }
            }
            let tracks: std::collections::HashMap<_, _> =
                clip.tracks.iter().map(|t| (&t.id, t)).collect();
            let events: std::collections::HashMap<_, _> =
                clip.events.iter().map(|e| (&e.id, e)).collect();
            for t in &old.tracks {
                if tracks.get(&t.id).copied() != Some(t) {
                    self.motion_unlocked(&t.id)?;
                    for k in t.keys() {
                        self.motion_unlocked(&k.id)?;
                    }
                }
            }
            for e in &old.events {
                if events.get(&e.id).copied() != Some(e) {
                    self.motion_unlocked(&e.id)?;
                }
            }
        }
        Ok(())
    }
}
impl Document {
    fn motion_registration_position(&self, id: &str) -> Option<usize> {
        let (owner, _) = self.get_motion_registration(id)?;
        let entries = match owner {
            MotionContainer::Root => self.motion_roots(),
            MotionContainer::Group(group) => &self.get_motion_group(&group)?.entries,
        };
        entries.iter().position(|entry| entry.id == id)
    }
}

fn clip_ids(c: &MotionClip) -> Vec<String> {
    c.tracks
        .iter()
        .flat_map(|t| std::iter::once(t.id.clone()).chain(t.keys().map(|k| k.id.clone())))
        .chain(c.events.iter().map(|e| e.id.clone()))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum MotionDisabledReason {
    NeedsConfiguration,
    UnresolvedTarget,
    UnsupportedModelChannel,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DisabledMotionTrack {
    pub track_id: String,
    pub authored_slot: usize,
    pub reason: MotionDisabledReason,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MotionProjection {
    pub clip_id: String,
    pub runtime_to_authored: Vec<usize>,
    pub authored_to_runtime: Vec<Option<usize>>,
    pub disabled: Vec<DisabledMotionTrack>,
}
impl Document {
    pub fn motion_track_disabled_reason(
        &self,
        target: &MotionTrackTarget,
    ) -> Option<MotionDisabledReason> {
        match target {
            MotionTrackTarget::Unspecified => Some(MotionDisabledReason::NeedsConfiguration),
            MotionTrackTarget::Unresolved { .. } => Some(MotionDisabledReason::UnresolvedTarget),
            MotionTrackTarget::Model { runtime_id } if runtime_id != "Opacity" => {
                Some(MotionDisabledReason::UnsupportedModelChannel)
            }
            _ => None,
        }
    }
    pub fn motion_projection(&self, id: &str) -> Result<MotionProjection, Status> {
        let clip = self
            .get_motion(id)
            .ok_or_else(|| Status::error("MISSING_MOTION", id))?;
        let mut p = MotionProjection {
            clip_id: id.into(),
            runtime_to_authored: vec![],
            authored_to_runtime: vec![],
            disabled: vec![],
        };
        for (slot, t) in clip.tracks.iter().enumerate() {
            if let Some(reason) = self.motion_track_disabled_reason(&t.target) {
                p.authored_to_runtime.push(None);
                p.disabled.push(DisabledMotionTrack {
                    track_id: t.id.clone(),
                    authored_slot: slot,
                    reason,
                });
            } else {
                p.authored_to_runtime
                    .push(Some(p.runtime_to_authored.len()));
                p.runtime_to_authored.push(slot);
            }
        }
        Ok(p)
    }
}

#[derive(Debug, Clone, Default)]
pub(in crate::document) struct MotionLookup {
    pub(in crate::document) clips: HashSet<String>,
    tracks: std::collections::HashMap<String, (String, usize)>,
    keys: std::collections::HashMap<String, (String, usize, usize)>,
    events: std::collections::HashMap<String, (String, usize)>,
}

/// Keep untouched neighbouring segments and copied internal segments. New paste
/// boundaries are linear; a paste does not claim to preserve the destination curve.
fn rebuild_key_connections(
    track: &mut MotionTrack,
    keys: Vec<MotionKey>,
    internal: &BTreeMap<(String, String), MotionSegment>,
) -> Result<(), Status> {
    if keys.windows(2).any(|p| p[0].time >= p[1].time) {
        return Err(Status::error(
            "MOTION_KEY_COLLISION",
            "Key times must be strictly increasing",
        ));
    }
    let old: Vec<_> = track.keys().collect();
    let connections: BTreeMap<_, _> = track
        .segments
        .iter()
        .enumerate()
        .map(|(i, s)| ((old[i].id.clone(), old[i + 1].id.clone()), s.clone()))
        .collect();
    let segments = keys
        .windows(2)
        .map(|p| {
            let pair = (p[0].id.clone(), p[1].id.clone());
            internal
                .get(&pair)
                .or_else(|| {
                    connections
                        .get(&pair)
                        .filter(|segment| segment.end() == &p[1] && old.iter().any(|k| **k == p[0]))
                })
                .cloned()
                .unwrap_or_else(|| MotionSegment::Linear { end: p[1].clone() })
        })
        .collect();
    track.initial = keys[0].clone();
    track.segments = Arc::new(segments);
    Ok(())
}

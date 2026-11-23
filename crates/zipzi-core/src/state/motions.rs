use super::*;
use std::collections::HashSet;

pub(super) fn clip_ids(clip: &MotionClip) -> impl Iterator<Item = &str> {
    std::iter::once(clip.id.as_str())
        .chain(
            clip.tracks.iter().flat_map(|t| {
                std::iter::once(t.id.as_str()).chain(t.keys().map(|k| k.id.as_str()))
            }),
        )
        .chain(clip.events.iter().map(|e| e.id.as_str()))
}

#[derive(Debug)]
pub(super) struct MotionIdentityCheck {
    source: ContentId,
    ids: Vec<String>,
    occupied: bool,
}

impl DocumentState {
    fn motion_id_collision(&self, added: &BTreeSet<&str>) -> bool {
        // An interactive insertion reuses the same key and optional track ID
        // throughout its gesture. Remember that query without indexing the
        // whole library or keeping a growing cache of inserted IDs.
        let mut cache = (added.len() <= 2).then(|| self.read_cache.motion_id_check.lock().unwrap());
        if let Some(check) = cache.as_ref().and_then(|cache| cache.as_ref()) {
            if check.source == self.content_id
                && check
                    .ids
                    .iter()
                    .map(String::as_str)
                    .eq(added.iter().copied())
            {
                return check.occupied;
            }
        }
        let occupied = self.contains_any_identity(added);
        if let Some(cache) = cache.as_mut() {
            **cache = Some(MotionIdentityCheck {
                source: self.content_id,
                ids: added.iter().map(|id| (*id).to_owned()).collect(),
                occupied,
            });
        }
        occupied
    }
}

fn normalize_frame_steps(clip: &mut MotionClip) -> Result<(), Status> {
    for index in 0..clip.tracks.len() {
        if clip.tracks[index]
            .segments
            .iter()
            .any(|s| s.frame_step.normalized() != Some(s.frame_step))
        {
            for segment in Arc::make_mut(&mut clip.tracks[index].segments) {
                segment.frame_step = segment
                    .frame_step
                    .normalized()
                    .ok_or_else(|| Status::error("INVALID_MOTION_FRAME_STEP", &segment.end.id))?;
            }
        }
    }
    Ok(())
}
impl DocumentBuilder {
    pub(super) fn validate_motion_payload(&self) -> Result<bool, Status> {
        let Some(ids) = self.touched.get(&RecordDomain::Motion) else {
            return self.validate_motion_registration_payload();
        };
        let Some(id) = ids.first() else {
            return Ok(false);
        };
        let Some(status) = self
            .motion(id)
            .and_then(|clip| self.scoped_motion_validation(clip))
        else {
            return Ok(false);
        };
        if status.is_ok() {
            Ok(true)
        } else {
            Err(status)
        }
    }

    fn validate_motion_registration_payload(&self) -> Result<bool, Status> {
        if self.setup_touched
            || !self.scene_initializations.is_empty()
            || !self.touched_assets.is_empty()
            || !self.touched_parameters.is_empty()
            || !self.touched.is_empty()
            || self.roots_touched.len() != 1
        {
            return Ok(false);
        }
        let model = self.content.model();
        let validate_entries = |before: &[MotionRegistration], after: &[MotionRegistration]| {
            if before.len() != after.len() {
                return Ok(false);
            }
            for (old, new) in before.iter().zip(after) {
                // Identity, registration ownership and clip coverage are already admitted.
                if old.id != new.id || old.clip_id != new.clip_id {
                    return Ok(false);
                }
                if old != new {
                    let status = model.validate_motion_registration(new);
                    if !status.is_ok() {
                        return Err(status);
                    }
                }
            }
            Ok(true)
        };
        if self.roots_touched.contains(&RootDomain::MotionRoots) {
            return validate_entries(self.base.motion_roots(), self.motion_roots());
        }
        if !self.roots_touched.contains(&RootDomain::MotionGroups)
            || self.base.motion_groups().len() != self.motion_groups().len()
        {
            return Ok(false);
        }
        for (old, new) in self.base.motion_groups().iter().zip(self.motion_groups()) {
            if old.id != new.id
                || old.name != new.name
                || !validate_entries(&old.entries, &new.entries)?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// A replacement against an admitted base only needs its own clip rules
    /// and checks for identities added to or removed from that clip.
    pub(super) fn scoped_motion_validation(&self, clip: &MotionClip) -> Option<Status> {
        if self.setup_touched
            || !self.roots_touched.is_empty()
            || !self.scene_initializations.is_empty()
            || !self.touched_assets.is_empty()
            || !self.touched_parameters.is_empty()
            || !self.touched.iter().all(|(domain, ids)| {
                *domain == RecordDomain::Motion && ids.len() == 1 && ids.contains(&clip.id)
            })
            || !self
                .content
                .motions
                .shares_order(&self.base.content.motions)
        {
            return None;
        }
        let old = self.base.motion(&clip.id)?;
        if !clip_ids(old).eq(clip_ids(clip)) {
            let old_ids: HashSet<_> = clip_ids(old).collect();
            let mut new_ids = HashSet::with_capacity(old_ids.len());
            for id in clip_ids(clip) {
                if !new_ids.insert(id) || (!old_ids.contains(id) && !valid_uuid(id)) {
                    return None;
                }
            }
            if old_ids
                .difference(&new_ids)
                .any(|id| self.editor_state().objects.contains_key(*id))
            {
                return None;
            }
            let added: BTreeSet<_> = new_ids.difference(&old_ids).copied().collect();
            if !added.is_empty() && self.base.motion_id_collision(&added) {
                return None;
            }
        }
        Some(self.content.model().validate_admitted_motion(clip))
    }

    pub fn create_motion(&mut self, mut clip: MotionClip) -> Result<(), Status> {
        self.write(|this| {
            normalize_frame_steps(&mut clip)?;
            this.create_motion_record(clip)
        })
    }
    pub fn replace_motion(&mut self, mut clip: MotionClip) -> Result<(), Status> {
        self.write(|this| {
            normalize_frame_steps(&mut clip)?;
            this.replace_motion_record(clip)
        })
    }
    pub fn create_authored_motion(&mut self, clip: MotionClip) -> Result<(), Status> {
        self.write(|this| {
            this.content.model().validate_motion_unlocked(&clip)?;
            let id = clip.id.clone();
            this.create_motion(clip)?;
            let mut roots = this.motion_roots().clone();
            roots.push(MotionRegistration {
                id: uuid::Uuid::new_v4().to_string(),
                clip_id: id,
                fade_in: None,
                fade_out: None,
                sound: None,
                extensions: Default::default(),
            });
            this.set_motion_roots(roots)
        })
    }
    pub fn replace_authored_motion(&mut self, clip: MotionClip) -> Result<(), Status> {
        self.write(|this| {
            let old = this
                .motion(&clip.id)
                .ok_or_else(|| Status::error("MISSING_MOTION", &clip.id))?;
            this.content.model().validate_motion_unlocked(&clip)?;
            let retained: std::collections::HashSet<_> = clip_ids(&clip).collect();
            let removed: Vec<_> = clip_ids(old)
                .filter(|id| !retained.contains(id))
                .map(str::to_owned)
                .collect();
            this.clear_owned_editor_objects(&removed);
            this.replace_motion(clip)
        })
    }
    pub fn edit_motion(&mut self, edit: MotionEdit) -> Result<(), Status> {
        self.write(|this| {
            match &edit {
                MotionEdit::PatchRegistration {
                    id,
                    expected,
                    entry,
                } if expected.clip_id == entry.clip_id && this.all_motions_registered() => {
                    return this.patch_motion_registration(id, expected, entry);
                }
                _ => {}
            }
            let changes = this.content.model().motion_changes(edit)?;
            // A domain operation knows its affected records; publication never
            // reconstructs a global document diff to discover these writes.
            let mut removed = Vec::new();
            let retained_clips: std::collections::HashSet<_> =
                changes.clips.values().flat_map(|c| clip_ids(c)).collect();
            for id in &changes.affected {
                if let Some(old) = this.motion(id) {
                    removed.extend(
                        clip_ids(old)
                            .filter(|id| !retained_clips.contains(id))
                            .map(str::to_owned),
                    );
                }
                match (this.motion(id), changes.clips.get(id)) {
                    (Some(_), None) => this.remove_motion(id)?,
                    (None, Some(clip)) => this.create_motion(clip.as_ref().clone())?,
                    (Some(old), Some(clip)) if old != clip.as_ref() => {
                        this.replace_motion(clip.as_ref().clone())?
                    }
                    _ => {}
                }
            }
            let retained: std::collections::HashSet<_> = changes
                .groups
                .iter()
                .map(|g| g.id.as_str())
                .chain(
                    changes
                        .groups
                        .iter()
                        .flat_map(|g| g.entries.iter().map(|e| e.id.as_str())),
                )
                .chain(changes.roots.iter().map(|e| e.id.as_str()))
                .collect();
            removed.extend(
                this.motion_groups()
                    .iter()
                    .map(|g| g.id.as_str())
                    .chain(
                        this.motion_groups()
                            .iter()
                            .flat_map(|g| g.entries.iter().map(|e| e.id.as_str())),
                    )
                    .chain(this.motion_roots().iter().map(|e| e.id.as_str()))
                    .filter(|id| !retained.contains(id))
                    .map(str::to_owned),
            );
            this.clear_owned_editor_objects(&removed);
            if this.motion_groups() != &changes.groups {
                this.set_motion_groups(changes.groups)?;
            }
            if this.motion_roots() != &changes.roots {
                this.set_motion_roots(changes.roots)?;
            }
            if this.motion_order() != changes.order {
                this.reorder(RecordDomain::Motion, changes.order)?;
            }
            Ok(())
        })
    }

    fn all_motions_registered(&self) -> bool {
        let registered: HashSet<_> = self
            .motion_groups()
            .iter()
            .flat_map(|group| &group.entries)
            .chain(self.motion_roots())
            .map(|entry| &entry.clip_id)
            .collect();
        self.motion_order().iter().all(|id| registered.contains(id))
    }

    fn patch_motion_registration(
        &mut self,
        id: &str,
        expected: &MotionRegistration,
        entry: &MotionRegistration,
    ) -> Result<(), Status> {
        let model = self.content.model();
        let (owner, current) = model
            .get_motion_registration(id)
            .ok_or_else(|| Status::error("MISSING_MOTION_REGISTRATION", id))?;
        if current != expected || id != entry.id {
            return Err(Status::error("STALE_FIELD", id));
        }
        if current == entry {
            return Ok(());
        }
        let status = model.validate_motion_registration(entry);
        if !status.is_ok() {
            return Err(status);
        }
        let state = self.editor_state();
        if let MotionContainer::Group(group) = &owner {
            if state.get(group).locked {
                return Err(Status::error("OBJECT_LOCKED", group));
            }
        }
        if state.get(id).locked {
            return Err(Status::error("OBJECT_LOCKED", id));
        }
        let entries = match owner {
            MotionContainer::Root => self.motion_roots_mut(),
            MotionContainer::Group(group) => {
                &mut self
                    .motion_groups_mut()
                    .iter_mut()
                    .find(|item| item.id == group)
                    .unwrap()
                    .entries
            }
        };
        *entries.iter_mut().find(|item| item.id == id).unwrap() = entry.clone();
        Ok(())
    }
}

impl DocumentState {
    pub fn motion_registration(&self, id: &str) -> Option<(MotionContainer, &MotionRegistration)> {
        for group in self.motion_groups() {
            if let Some(entry) = group.entries.iter().find(|e| e.id == id) {
                return Some((MotionContainer::Group(group.id.clone()), entry));
            }
        }
        self.motion_roots()
            .iter()
            .find(|e| e.id == id)
            .map(|e| (MotionContainer::Root, e))
    }
}

impl DocumentState {
    pub fn motion_registrations(&self) -> impl Iterator<Item = &MotionRegistration> {
        self.motion_groups()
            .iter()
            .flat_map(|g| g.entries.iter())
            .chain(self.motion_roots())
    }
}

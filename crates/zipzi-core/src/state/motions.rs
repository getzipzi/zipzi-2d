use super::*;
fn clip_ids(clip: &MotionClip) -> impl Iterator<Item = &str> {
    std::iter::once(clip.id.as_str())
        .chain(
            clip.tracks.iter().flat_map(|t| {
                std::iter::once(t.id.as_str()).chain(t.keys().map(|k| k.id.as_str()))
            }),
        )
        .chain(clip.events.iter().map(|e| e.id.as_str()))
}
impl DocumentBuilder {
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

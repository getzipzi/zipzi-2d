use super::*;

impl Document {
    pub(super) fn install_deletion_dependents(
        &self,
        candidate: &mut Document,
        ids: &BTreeSet<&str>,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        let parts: BTreeSet<_> = self
            .part_order()
            .iter()
            .filter(|id| ids.contains(id.as_str()))
            .map(String::as_str)
            .collect();
        if parts.is_empty() {
            return Ok(());
        }
        if let Some(pose) = &mut candidate.content.pose {
            let pose = Arc::make_mut(pose);
            for group in &mut pose.groups {
                control.checkpoint()?;
                group.entries.retain(|e| !matches!(&e.part, PosePartRef::Resolved { part_id } if parts.contains(part_id.as_str())));
                for entry in &mut group.entries {
                    if let Some(links) = &mut entry.links {
                        links.retain(|l| !matches!(&l.target, PosePartRef::Resolved { part_id } if parts.contains(part_id.as_str())));
                    }
                }
            }
        }
        for clip in candidate.content.motions.values_mut() {
            control.checkpoint()?;
            if clip.tracks.iter().any(|t| matches!(&t.target, MotionTrackTarget::PartOpacity { part_id } if parts.contains(part_id.as_str()))) {
                let clip = Arc::make_mut(clip);
                clip.tracks.retain(|t| !matches!(&t.target, MotionTrackTarget::PartOpacity { part_id } if parts.contains(part_id.as_str())));
                clip.track_order.retain(|id| clip.tracks.iter().any(|t| &t.id == id));
            }
        }
        for expression in candidate.content.expressions.values_mut() {
            control.checkpoint()?;
            if let Some(mut scene) = expression.source_scene() {
                let old_len = scene.tracks.len();
                scene.tracks.retain(|t| !matches!(&t.target, MotionTrackTarget::PartOpacity { part_id } if parts.contains(part_id.as_str())));
                if old_len != scene.tracks.len() {
                    scene
                        .track_order
                        .retain(|id| scene.tracks.iter().any(|t| &t.id == id));
                    expression.preserve_source_scene(&scene);
                }
            }
        }
        if let Some(entries) = &mut candidate.content.display_info.parts {
            entries.retain(|e| !matches!(e, CdiPartEntry::Resolved { part_id, .. } if parts.contains(part_id.as_str())));
        }
        candidate.motion_lookup.take();
        candidate.expression_entries.take();
        Ok(())
    }

    pub(super) fn deletion_unlocked(&self, key: &DeletionObject) -> Result<(), Status> {
        match key.kind {
            DeletionKind::PoseEntry | DeletionKind::PoseLink => self.pose_unlocked(&key.id),
            DeletionKind::MotionTrack | DeletionKind::MotionKey => self.motion_unlocked(&key.id),
            DeletionKind::ExpressionTrack | DeletionKind::ExpressionKey => {
                self.authoring_unlocked(key.owner_id.as_deref().unwrap())
            }
            DeletionKind::DisplayInfo => Ok(()),
            _ => self.authoring_unlocked(&key.id),
        }
    }
}

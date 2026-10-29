use super::*;

fn scoped(kind: DeletionKind, id: &str, owner: &str) -> DeletionObject {
    DeletionObject {
        kind,
        id: id.into(),
        owner_id: Some(owner.into()),
    }
}
impl Document {
    pub(super) fn deletion_dependents(
        &self,
        removed: &mut BTreeMap<DeletionObject, DeletionReason>,
        report: &mut DeletionReport,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        let parts: BTreeSet<_> = removed
            .keys()
            .filter(|k| k.kind == DeletionKind::Part)
            .map(|k| k.id.clone())
            .collect();
        if parts.is_empty() {
            return Ok(());
        }
        if let Some(pose) = self.pose() {
            for entry in pose.groups.iter().flat_map(|g| &g.entries) {
                control.checkpoint()?;
                let owner = object(DeletionKind::PoseEntry, &entry.id);
                let remove_entry = matches!(&entry.part, PosePartRef::Resolved { part_id } if parts.contains(part_id));
                if let PosePartRef::Resolved { part_id } = &entry.part {
                    if remove_entry {
                        removed.insert(owner.clone(), DeletionReason::Dependency);
                        report.relations.push(DeletionRelation {
                            owner: owner.clone(),
                            field: DeletionField::PosePart,
                            target: object(DeletionKind::Part, part_id),
                        });
                        report
                            .effects
                            .push(DeletionEffect::PartAnimationRemoved(owner.clone()));
                    }
                }
                for link in entry.links.iter().flatten() {
                    control.checkpoint()?;
                    let key = object(DeletionKind::PoseLink, &link.id);
                    if remove_entry {
                        add_owned(removed, report, owner.clone(), key);
                    } else if let PosePartRef::Resolved { part_id } = &link.target {
                        if parts.contains(part_id) {
                            removed.insert(key.clone(), DeletionReason::Dependency);
                            report.relations.push(DeletionRelation {
                                owner: key,
                                field: DeletionField::PoseLinkPart,
                                target: object(DeletionKind::Part, part_id),
                            });
                        }
                    }
                }
            }
        }
        for id in self.motion_order() {
            control.checkpoint()?;
            for track in &self.get_motion(id).unwrap().tracks {
                collect_track(removed, report, track, None, &parts, control)?;
            }
        }
        for id in self.expression_order() {
            control.checkpoint()?;
            if let Some(scene) = self.get_expression(id).unwrap().source_scene() {
                for track in &scene.tracks {
                    collect_track(removed, report, track, Some(id), &parts, control)?;
                }
            }
        }
        for (slot, entry) in self.display_info().parts.iter().flatten().enumerate() {
            if let CdiPartEntry::Resolved { part_id, .. } = entry {
                if parts.contains(part_id) {
                    report.rewritten.push(DeletionRelation {
                        owner: object(DeletionKind::DisplayInfo, self.id()),
                        field: DeletionField::CdiPart { slot },
                        target: object(DeletionKind::Part, part_id),
                    });
                }
            }
        }
        Ok(())
    }

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

fn collect_track(
    removed: &mut BTreeMap<DeletionObject, DeletionReason>,
    report: &mut DeletionReport,
    track: &MotionTrack,
    expression: Option<&str>,
    parts: &BTreeSet<String>,
    control: &ConversionControl,
) -> Result<(), Status> {
    control.checkpoint()?;
    let MotionTrackTarget::PartOpacity { part_id } = &track.target else {
        return Ok(());
    };
    if !parts.contains(part_id) {
        return Ok(());
    }
    let (owner, field) = if let Some(expression) = expression {
        (
            scoped(DeletionKind::ExpressionTrack, &track.id, expression),
            DeletionField::ExpressionSourcePart,
        )
    } else {
        (
            object(DeletionKind::MotionTrack, &track.id),
            DeletionField::MotionPart,
        )
    };
    removed.insert(owner.clone(), DeletionReason::Dependency);
    report.relations.push(DeletionRelation {
        owner: owner.clone(),
        field,
        target: object(DeletionKind::Part, part_id),
    });
    report
        .effects
        .push(DeletionEffect::PartAnimationRemoved(owner.clone()));
    for key in track.keys() {
        control.checkpoint()?;
        let key = if let Some(expression) = expression {
            scoped(DeletionKind::ExpressionKey, &key.id, expression)
        } else {
            object(DeletionKind::MotionKey, &key.id)
        };
        add_owned(removed, report, owner.clone(), key);
    }
    Ok(())
}

use super::*;

impl Document {
    /// Native domain replacement rules also protect retained keys, ordered
    /// Physics nodes and container members whose positions change implicitly.
    pub(super) fn validate_deletion_business_locks(
        &self,
        candidate: &Document,
        report: &mut DeletionReport,
        control: &ConversionControl,
    ) -> Result<bool, Status> {
        if !self.editor_state().objects.values().any(|v| v.locked) {
            return Ok(true);
        }
        let mut checks = vec![];
        control.checkpoint()?;
        if self.content.motions != candidate.content.motions
            || self.content.motion_groups != candidate.content.motion_groups
            || self.content.motion_roots != candidate.content.motion_roots
        {
            checks.push(self.validate_motion_changes_unlocked(candidate));
        }
        control.checkpoint()?;
        if self.pose() != candidate.pose() {
            checks.push(self.validate_pose_replacement(candidate.pose()));
        }
        if self.physics() != candidate.physics() {
            if let Some(asset) = candidate.physics() {
                checks.push(self.validate_physics_replacement_unlocked(asset));
            }
        }
        for id in self.expression_order() {
            control.checkpoint()?;
            if let Some(expression) = candidate
                .get_expression(id)
                .filter(|e| self.get_expression(id) != Some(*e))
            {
                checks.push(self.validate_expression_unlocked(expression));
            }
        }
        let index = self.deletion_index(control)?;
        for status in checks.into_iter().filter_map(Result::err) {
            if status.code == "OBJECT_LOCKED" {
                let key = index
                    .objects
                    .iter()
                    .find(|k| k.id == status.message && k.owner_id.is_none())
                    .cloned()
                    .unwrap_or_else(|| object(DeletionKind::DisplayInfo, &status.message));
                if !report
                    .blockers
                    .iter()
                    .any(|b| b.source_id == status.message)
                {
                    report.blockers.push(DeletionLock {
                        object: key,
                        source_id: status.message,
                    });
                }
            } else {
                report.structural_blockers.push(DeletionStructuralBlocker {
                    object_id: String::new(),
                    code: status.code,
                    reason: status.message,
                });
            }
        }
        Ok(report.blockers.is_empty() && report.structural_blockers.is_empty())
    }
    pub(super) fn deletion_unlocked(&self, key: &DeletionObject) -> Result<(), Status> {
        if let Some(owner) = &key.owner_id {
            return self.authoring_unlocked(owner);
        }
        if let Some((expression, _)) = self.get_expression_entry(&key.id) {
            self.authoring_unlocked(&expression.id)?;
        }
        if matches!(
            key.kind,
            DeletionKind::BlendKeyTable | DeletionKind::BlendConstraint
        ) {
            let samples: Vec<_> = self
                .get_blend_key_table(&key.id)
                .map(|t| t.samples.iter().map(|s| s.id.as_str()).collect())
                .or_else(|| {
                    self.get_blend_constraint(&key.id)
                        .map(|c| c.samples.iter().map(|s| s.id.as_str()).collect())
                })
                .unwrap_or_default();
            for id in std::iter::once(key.id.as_str()).chain(samples) {
                if self.editor_state().get(id).locked {
                    return Err(Status::error("OBJECT_LOCKED", id));
                }
            }
            return Ok(());
        }
        match key.kind {
            DeletionKind::PoseEntry | DeletionKind::PoseLink => self.pose_unlocked(&key.id),
            DeletionKind::Motion
            | DeletionKind::MotionGroup
            | DeletionKind::MotionRegistration
            | DeletionKind::MotionEvent
            | DeletionKind::MotionTrack
            | DeletionKind::MotionKey => self.motion_unlocked(&key.id),
            DeletionKind::Physics
            | DeletionKind::PhysicsGroup
            | DeletionKind::PhysicsInput
            | DeletionKind::PhysicsOutput
            | DeletionKind::PhysicsNode => self.physics_unlocked(&key.id),
            DeletionKind::Pose | DeletionKind::PoseGroup => self.pose_unlocked(&key.id),
            DeletionKind::ExpressionTrack | DeletionKind::ExpressionKey => {
                self.authoring_unlocked(key.owner_id.as_deref().unwrap())
            }
            DeletionKind::DisplayInfo => Ok(()),
            _ => self.authoring_unlocked(&key.id),
        }
    }
}

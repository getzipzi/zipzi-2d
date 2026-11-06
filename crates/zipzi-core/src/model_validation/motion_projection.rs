use super::*;
impl Model<'_> {
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

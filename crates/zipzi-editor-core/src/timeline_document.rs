use super::Document;
use anyhow::Result;
use zipzi_sdk::{MotionClipCandidate, Version};

impl Document {
    pub(crate) fn commit_timeline_candidate(
        &mut self,
        candidate: MotionClipCandidate,
        version: Version,
        label: &str,
    ) -> Result<()> {
        self.finish_edit()?;
        let (_, receipt) = self.session.edit(label, Some(version), |edit| {
            edit.replace_motion_candidate(candidate)
        })?;
        self.session.acknowledge_receipts();
        if receipt.changed {
            self.pristine_blank = None;
            self.invalidate_projection(false);
        }
        Ok(())
    }
    pub(crate) fn create_timeline_clip(&mut self) -> Result<crate::OperationResult> {
        self.finish_edit()?;
        let plan = self.plan_animation_resource_creation(
            crate::ObjectKind::Motion,
            "New Motion".into(),
            None,
        )?;
        self.execute_animation_library(&plan)
    }
}

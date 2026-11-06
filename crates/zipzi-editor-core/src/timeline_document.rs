use super::Document;
use anyhow::Result;

impl Document {
    pub(crate) fn commit_timeline_draft(
        &mut self,
        draft: &mut crate::runtime_host::HostedDraft,
    ) -> Result<()> {
        self.finish_edit()?;
        let receipt = draft.commit_to(&mut self.session)?;
        self.session.acknowledge_receipts();
        if receipt.kind != zipzi_sdk::PublicationKind::Unchanged {
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

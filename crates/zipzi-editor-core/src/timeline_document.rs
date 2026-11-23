use super::Document;
use anyhow::Result;

impl Document {
    pub(crate) fn commit_timeline_draft(
        &mut self,
        draft: &mut crate::runtime_host::HostedDraft,
    ) -> Result<()> {
        self.finish_edit()?;
        let before = self.session.document();
        let receipt = draft.commit_to(&mut self.session)?;
        if receipt.kind == zipzi_sdk::PublicationKind::Unchanged {
            self.session.acknowledge_receipts();
            return Ok(());
        }
        self.pristine_blank = None;
        self.consume_authoring_receipt(&receipt, Some(&before));
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

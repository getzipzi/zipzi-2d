use super::*;
impl Editor {
    pub fn export_canvas(&self, id: DocumentId) -> Result<crate::ExportCanvas> {
        ensure!(self.active_id() == Some(id), "Document changed");
        self.document()
            .ok_or_else(|| anyhow!("No document"))?
            .export_canvas()
    }
    pub fn prepare_export(
        &mut self,
        id: DocumentId,
        revision: u64,
        canvas: crate::ExportCanvas,
    ) -> Result<crate::TransferSnapshot> {
        ensure!(
            self.active_id() == Some(id) && !self.exiting(),
            "Document changed"
        );
        let document = &mut self
            .entry_mut(id)?
            .session_mut()
            .ok_or_else(|| anyhow!("Document is saving"))?
            .document;
        ensure!(
            document.model_generation() == revision,
            "Document changed while configuring export"
        );
        document.remember_export_settings(canvas.validated()?)?;
        document.transfer_snapshot()
    }
}

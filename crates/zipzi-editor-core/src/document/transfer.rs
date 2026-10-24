use super::*;
impl Document {
    pub(crate) fn export_canvas(&self) -> Result<crate::ExportCanvas> {
        let mut frame = zipzi_core::DrawableFrame::default();
        let status = zipzi_core::evaluation::evaluate_frame_including_hidden(
            self.model(),
            &Default::default(),
            &mut frame,
        );
        ensure!(status.is_ok(), "{}", status.message);
        let settings = self.export_settings();
        let canvas = zipzi_core::geometry::finite_export_canvas(
            settings,
            frame
                .drawables
                .iter()
                .flat_map(|d| d.positions.iter().copied()),
        )
        .map_err(|s| anyhow!("{}", s.message))?;
        Ok(crate::ExportCanvas::new(settings, canvas))
    }
    pub(crate) fn remember_export_settings(
        &mut self,
        settings: zipzi_core::ExportSettings,
    ) -> Result<()> {
        self.finish_edit()?;
        if settings == self.export_settings() {
            return Ok(());
        }
        let mut edit = self.session.begin_owned_edit("Export settings", None)?;
        edit.replace_export_settings(settings)?;
        edit.commit_to(&mut self.session)?;
        self.session.drain_events();
        self.pristine_blank = false;
        self.invalidate_projection(false);
        Ok(())
    }
    pub(crate) fn transfer_snapshot(&self) -> Result<zipzi_project::TransferSnapshot> {
        Ok(self.session.transfer_snapshot()?)
    }
    pub fn import_cubism(path: &Path) -> Result<(Self, Vec<String>)> {
        let mut session = Self::empty_session()?;
        session.import_cubism(path, None)?;
        let mut document = Self::from_session(session)?;
        document.import_name = path.file_stem().map(|s| s.to_string_lossy().into_owned());
        Ok((document, Vec::new()))
    }
}

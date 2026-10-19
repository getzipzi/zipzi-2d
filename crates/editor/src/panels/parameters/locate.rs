use super::*;

impl ParametersPanel {
    pub(crate) fn locate_created_parameter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_with_editor(window, cx);
        let focus = self.owner.upgrade().and_then(|owner| {
            owner
                .read(cx)
                .editor
                .document()?
                .context()
                .parameter_focus
                .clone()
        });
        if let Some(index) = focus.and_then(|focus| {
            self.rows
                .iter()
                .position(|row| row.parameter.id == focus.id)
        }) {
            self.layout.reveal(index, &mut self.collapsed_groups);
            self.refresh_visible_rows(cx);
            if let Some(visible_index) = self.visible_rows.iter().position(|row| {
                matches!(row, ListRow::Parameter { index: row_index, .. } if *row_index == index)
            }) {
                self.scroll.scroll_to_item(visible_index, ScrollStrategy::Center);
            }
        }
        cx.notify();
    }
}

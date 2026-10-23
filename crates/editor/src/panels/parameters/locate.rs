use super::*;

impl ParametersPanel {
    pub(crate) fn locate_moved_object(
        &mut self,
        document: DocumentId,
        object: &editor_core::ObjectRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_with_editor(window, cx);
        if self.document_id != Some(document) {
            return;
        }
        if object.kind == editor_core::ObjectKind::ParameterGroup {
            self.locate_parameter_group(&object.id, window, cx);
        } else {
            self.sync_with_editor(window, cx);
            self.locate_parameter(&object.id, cx);
        }
    }

    fn locate_parameter(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(index) = self.rows.iter().position(|row| row.parameter.id == id) {
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

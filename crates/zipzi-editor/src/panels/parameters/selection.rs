//! Single-row inspection and presentation-only navigation. Parameters retain
//! their independent preview focus; browsing folders never replaces scene selection.
use super::*;
use zipzi_editor_core::{EditingContextToken, ObjectKind, ObjectRef};

#[derive(Clone, PartialEq, Eq)]
pub(super) enum ParameterCursor {
    Object(ObjectRef),
    Ungrouped,
}
impl ParameterCursor {
    fn object(&self) -> Option<ObjectRef> {
        match self {
            Self::Object(object) => Some(object.clone()),
            Self::Ungrouped => None,
        }
    }
}

#[derive(Default)]
pub(super) struct ParameterNavigation {
    context: Option<EditingContextToken>,
    pub cursor: Option<ParameterCursor>,
    pub inspected: Option<ObjectRef>,
}

impl ParametersPanel {
    fn row_cursor(&self, row: &ListRow) -> ParameterCursor {
        match row {
            ListRow::Group { id: Some(id), .. } => {
                ParameterCursor::Object(ObjectRef::new(ObjectKind::ParameterGroup, id))
            }
            ListRow::Group { id: None, .. } => ParameterCursor::Ungrouped,
            ListRow::Parameter { index, .. } => ParameterCursor::Object(ObjectRef::new(
                ObjectKind::Parameter,
                &self.rows[*index].parameter.id,
            )),
        }
    }

    fn cursor_index(&self, cursor: &ParameterCursor) -> Option<usize> {
        self.visible_rows
            .iter()
            .position(|row| self.row_cursor(row) == *cursor)
    }

    pub(crate) fn movement_object(&self) -> Option<ObjectRef> {
        self.navigation
            .cursor
            .as_ref()
            .and_then(|cursor| self.cursor_index(cursor).and_then(|_| cursor.object()))
    }

    pub(super) fn sync_navigation(&mut self, cx: &mut Context<Self>) {
        let (context, inspected) = self.owner.upgrade().map_or((None, None), |owner| {
            let editor = &owner.read(cx).editor;
            (
                editor.editing_context_token(),
                editor
                    .document()
                    .and_then(|doc| doc.context().inspected.clone())
                    .filter(|r| {
                        matches!(r.kind, ObjectKind::Parameter | ObjectKind::ParameterGroup)
                    }),
            )
        });
        if self.navigation.context != context || self.navigation.inspected != inspected {
            self.navigation = ParameterNavigation {
                context,
                cursor: inspected.clone().map(ParameterCursor::Object),
                inspected,
            };
            self.reconcile_navigation(cx);
        }
    }

    pub(super) fn reconcile_navigation(&mut self, cx: &mut Context<Self>) {
        if self
            .navigation
            .cursor
            .as_ref()
            .is_some_and(|cursor| self.cursor_index(cursor).is_none())
        {
            self.navigation.cursor = None;
        }
        if self.navigation.cursor.is_none() {
            self.navigation.cursor = self
                .navigation
                .inspected
                .clone()
                .map(ParameterCursor::Object)
                .filter(|cursor| self.cursor_index(cursor).is_some());
        }
        self.refresh_navigation_styles(cx);
    }

    pub(super) fn refresh_navigation_styles(&self, cx: &mut Context<Self>) {
        for row in &self.rows {
            let object = ObjectRef::new(ObjectKind::Parameter, &row.parameter.id);
            let selected = self.navigation.inspected.as_ref() == Some(&object);
            let cursor = self.navigation_focused
                && self.navigation.cursor.as_ref() == Some(&ParameterCursor::Object(object));
            let view = row.view.read(cx);
            if view.selected != selected || view.cursor != cursor {
                row.view.update(cx, |view, cx| {
                    view.selected = selected;
                    view.cursor = cursor;
                    cx.notify();
                });
            }
        }
    }

    pub(super) fn activate_row(
        &mut self,
        document: DocumentId,
        cursor: ParameterCursor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id != Some(document)
            || !self.slider_interaction_allowed(window, cx)
            || self.cursor_index(&cursor).is_none()
        {
            return;
        }
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let success = owner.update(cx, |owner, cx| {
            if !owner.prepare_property_inputs(document, window, cx) {
                return false;
            }
            let result = match cursor.object() {
                Some(object) if object.kind == ObjectKind::Parameter => owner
                    .editor
                    .execute_for(document, EditorCommand::FocusParameter(object.id)),
                Some(object) => owner
                    .editor
                    .execute_for(document, EditorCommand::Navigate(object)),
                None => Ok(()),
            };
            let success = result.is_ok();
            owner.complete_edit(result, cx);
            success
        });
        if success {
            self.navigation.cursor = Some(cursor);
            window.focus(&self.focus, cx);
            self.navigation_focused = true;
            self.sync_navigation(cx);
            self.refresh_navigation_styles(cx);
            cx.notify();
        }
    }

    pub(super) fn toggle_group(
        &mut self,
        group: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.slider_interaction_allowed(window, cx) {
            return;
        }
        let cursor = group.as_ref().map_or(ParameterCursor::Ungrouped, |id| {
            ParameterCursor::Object(ObjectRef::new(ObjectKind::ParameterGroup, id))
        });
        if self.cursor_index(&cursor).is_none() {
            return;
        }
        self.finish_slider_drag(cx);
        self.navigation.cursor = Some(cursor);
        if !self.collapsed_groups.remove(&group) {
            self.collapsed_groups.insert(group);
        }
        window.focus(&self.focus, cx);
        self.navigation_focused = true;
        self.refresh_visible_rows(cx);
        cx.notify();
    }

    pub(super) fn parameter_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if !self.focus.is_focused(window)
            || event.prefer_character_input
            || cx.has_active_drag()
            || modifiers.alt
            || modifiers.control
            || modifiers.platform
            || modifiers.shift
            || !self.slider_interaction_allowed(window, cx)
        {
            return;
        }
        self.sync_navigation(cx);
        let current = self
            .navigation
            .cursor
            .as_ref()
            .and_then(|cursor| self.cursor_index(cursor));
        let next = match event.keystroke.key.as_str() {
            "down" => Some(current.map_or(0, |i| {
                (i + 1).min(self.visible_rows.len().saturating_sub(1))
            })),
            "up" => Some(
                current.map_or(self.visible_rows.len().saturating_sub(1), |i| {
                    i.saturating_sub(1)
                }),
            ),
            "home" => Some(0),
            "end" => self.visible_rows.len().checked_sub(1),
            "enter" => {
                if let (Some(document), Some(cursor)) =
                    (self.document_id, self.navigation.cursor.clone())
                {
                    self.activate_row(document, cursor, window, cx);
                }
                cx.stop_propagation();
                return;
            }
            "left" | "right" => {
                let right = event.keystroke.key == "right";
                current.and_then(|i| {
                    let row = &self.visible_rows[i];
                    if let ListRow::Group { id, .. } = row {
                        if self.collapsed_groups.contains(id) == right {
                            self.toggle_group(id.clone(), window, cx);
                            return None;
                        }
                    }
                    if right {
                        self.visible_rows
                            .get(i + 1)
                            .filter(|child| child.depth() > row.depth())
                            .map(|_| i + 1)
                    } else {
                        (0..i)
                            .rev()
                            .find(|&parent| self.visible_rows[parent].depth() < row.depth())
                    }
                })
            }
            _ => return,
        };
        if let Some(i) = next.filter(|&i| i < self.visible_rows.len()) {
            self.navigation.cursor = Some(self.row_cursor(&self.visible_rows[i]));
            self.scroll.scroll_to_item(i, ScrollStrategy::Center);
            self.refresh_navigation_styles(cx);
            cx.notify();
        }
        cx.stop_propagation();
    }
}

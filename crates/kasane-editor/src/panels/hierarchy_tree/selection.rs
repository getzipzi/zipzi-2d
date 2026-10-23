//! Presentation-only cursor and range anchor. Model selection stays in core;
//! ranges use visible rows and never use their indices as object identities.
use super::*;
use editor_core::{EditingContext, ObjectRef};

#[derive(Default)]
pub(super) struct TreeSelection {
    pub cursor: Option<String>,
    anchor: Option<ObjectRef>,
}

impl TreeSelection {
    pub(super) fn reconcile(&mut self, index: &editor_core::ObjectIndex) {
        if self
            .anchor
            .as_ref()
            .is_some_and(|anchor| index.get(anchor).is_none())
        {
            self.anchor = None;
        }
    }
}

impl<const DEFORMER: bool> HierarchyTreePanel<DEFORMER> {
    pub(crate) fn movement_object(&self) -> Option<ObjectRef> {
        match &self.selection.cursor {
            Some(key) => self
                .rows
                .iter()
                .find(|row| node_key(&row.node) == *key)
                .and_then(|row| row.node.reference()),
            None => self
                .rows
                .iter()
                .find(|row| {
                    row.node.reference().is_some()
                        && row.node.reference().as_ref() == self.last_selection.as_ref()
                })
                .and_then(|row| row.node.reference()),
        }
    }

    pub(super) fn select_row(
        &mut self,
        document: DocumentId,
        key: &str,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_id != Some(document) || window.has_active_dialog(cx) {
            return;
        }
        let Some(row) = self.rows.iter().find(|row| node_key(&row.node) == key) else {
            return;
        };
        let reference = row.node.reference();
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let state = owner.read(cx);
        if state.modal
            || state.editor.is_busy()
            || state.editor.active_id() != Some(document)
            || state.editor.editing_context() != Some(EditingContext::Modeling)
        {
            return;
        }
        let context = state.editor.document().unwrap().context();
        let anchor = self
            .selection
            .anchor
            .clone()
            .or(context.selection.clone())
            .or(reference.clone());
        let objects = reference.as_ref().map(|reference| {
            selected_objects(
                &self.rows,
                anchor.as_ref(),
                reference,
                &context.selections,
                modifiers.shift,
                modifiers.secondary(),
            )
        });
        let ready = owner.update(cx, |state, cx| {
            if !state.prepare_property_inputs(document, window, cx) {
                return false;
            }
            if let Some(objects) = objects {
                let result = state
                    .editor
                    .execute_for(document, EditorCommand::SelectObjects(objects));
                let success = result.is_ok();
                state.complete_edit(result, cx);
                success
            } else {
                true
            }
        });
        if ready {
            self.selection.anchor = if modifiers.shift { anchor } else { reference };
            self.selection.cursor = Some(key.to_owned());
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    pub(super) fn tree_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus.is_focused(window)
            || event.prefer_character_input
            || cx.has_active_drag()
            || event.keystroke.modifiers.alt
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.platform
            || window.has_active_dialog(cx)
        {
            return;
        }
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let state = owner.read(cx);
        if state.modal
            || state.editor.is_busy()
            || state.editor.editing_context() != Some(EditingContext::Modeling)
            || state.editor.active_id() != self.document_id
        {
            return;
        }
        let Some(document) = self.document_id else {
            return;
        };
        let current = self
            .selection
            .cursor
            .as_ref()
            .and_then(|key| self.rows.iter().position(|row| node_key(&row.node) == *key))
            .or_else(|| {
                self.rows.iter().position(|row| {
                    row.node.reference().is_some()
                        && row.node.reference().as_ref() == self.last_selection.as_ref()
                })
            });
        let next = match event.keystroke.key.as_str() {
            "down" => Some(current.map_or(0, |i| (i + 1).min(self.rows.len().saturating_sub(1)))),
            "up" => {
                Some(current.map_or(self.rows.len().saturating_sub(1), |i| i.saturating_sub(1)))
            }
            "home" => Some(0),
            "end" => self.rows.len().checked_sub(1),
            "enter" => {
                if let Some(i) = current {
                    let key = node_key(&self.rows[i].node);
                    self.select_row(document, &key, event.keystroke.modifiers, window, cx);
                }
                cx.stop_propagation();
                return;
            }
            "left" | "right" if !event.keystroke.modifiers.shift => {
                if let Some(i) = current {
                    let row = &self.rows[i];
                    let key = node_key(&row.node);
                    let collapsed = self.collapsed().contains(&key);
                    let right = event.keystroke.key == "right";
                    if row.has_children && collapsed == right {
                        self.selection.cursor = Some(key.clone());
                        self.toggle_expanded(&key, cx);
                        cx.stop_propagation();
                        return;
                    }
                    if right {
                        self.rows
                            .get(i + 1)
                            .filter(|child| child.depth > row.depth)
                            .map(|_| i + 1)
                    } else {
                        (0..i)
                            .rev()
                            .find(|&parent| self.rows[parent].depth < row.depth)
                    }
                } else {
                    None
                }
            }
            _ => return,
        };
        if let Some(i) = next.filter(|&i| i < self.rows.len()) {
            let key = node_key(&self.rows[i].node);
            if event.keystroke.modifiers.shift {
                self.select_row(document, &key, event.keystroke.modifiers, window, cx);
            } else {
                self.selection.cursor = Some(key);
            }
            self.scroll.scroll_to_item(i, ScrollStrategy::Center);
            cx.notify();
        }
        cx.stop_propagation();
    }
}

fn selected_objects(
    rows: &[VisibleRow],
    anchor: Option<&ObjectRef>,
    clicked: &ObjectRef,
    current: &[ObjectRef],
    range: bool,
    additive: bool,
) -> Vec<ObjectRef> {
    let mut objects = if range {
        let anchor = anchor.and_then(|anchor| {
            rows.iter()
                .position(|row| row.node.reference().as_ref() == Some(anchor))
        });
        let clicked_index = rows
            .iter()
            .position(|row| row.node.reference().as_ref() == Some(clicked));
        let mut objects = if additive {
            current.to_vec()
        } else {
            Vec::new()
        };
        if let (Some(a), Some(b)) = (anchor, clicked_index) {
            for object in rows[a.min(b)..=a.max(b)]
                .iter()
                .filter_map(|row| row.node.reference())
            {
                if !objects.contains(&object) {
                    objects.push(object);
                }
            }
        }
        objects
    } else if additive {
        let mut objects = current.to_vec();
        if objects.contains(clicked) {
            objects.retain(|object| object != clicked);
            return objects;
        }
        objects
    } else {
        Vec::new()
    };
    // Core's primary selection is the last member, including reverse ranges.
    objects.retain(|object| object != clicked);
    objects.push(clicked.clone());
    objects
}

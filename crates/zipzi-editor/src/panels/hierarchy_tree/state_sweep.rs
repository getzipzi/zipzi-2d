use super::{HierarchyTreePanel, VisibleRow, locking};
use gpui_kit::{component::WindowExt, prelude::*, *};
use std::rc::Rc;
use zipzi_editor_core::{DocumentId, EditingContext, EditorCommand, ObjectRef, ObjectState};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StateColumn {
    Visibility,
    Lock,
}

impl StateColumn {
    pub(super) fn restricted(self, state: &ObjectState) -> bool {
        match self {
            Self::Visibility => state.explicit.hidden_in_editor,
            Self::Lock => state.explicit.locked,
        }
    }

    pub(super) fn command(self, objects: Vec<ObjectRef>, restricted: bool) -> EditorCommand {
        match self {
            Self::Visibility => EditorCommand::SetObjectsVisible {
                objects,
                visible: !restricted,
            },
            Self::Lock => EditorCommand::SetObjectsLocked {
                objects,
                locked: restricted,
            },
        }
    }
}

pub(super) struct StateSweep {
    document: DocumentId,
    column: StateColumn,
    restricted: bool,
    rows: Rc<Vec<VisibleRow>>,
    first_row: usize,
    last_row: usize,
}

impl<const DEFORMER: bool> HierarchyTreePanel<DEFORMER> {
    pub(super) fn state_control(
        &self,
        document: DocumentId,
        column: StateColumn,
        row: usize,
        disabled: bool,
        control: impl IntoElement,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        div()
            .h_full()
            .flex()
            .flex_shrink_0()
            .items_center()
            .child(control)
            .when(!disabled, |control| {
                control
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.start_state_sweep(document, column, row, window, cx);
                        }),
                    )
                    .on_mouse_move(cx.listener(move |this, event: &MouseMoveEvent, _, cx| {
                        if event.pressed_button == Some(MouseButton::Left) {
                            this.extend_state_sweep(column, row, cx);
                        }
                    }))
            })
    }

    fn start_state_sweep(
        &mut self,
        document: DocumentId,
        column: StateColumn,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_state_sweep(cx);
        if window.has_active_dialog(cx) || self.rows_dirty {
            return;
        }
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
        let Some(object) = self.rows.get(row).and_then(|row| row.node.reference()) else {
            return;
        };
        self.state_sweep = Some(StateSweep {
            document,
            column,
            restricted: !column.restricted(&state.editor.document().unwrap().object_state(&object)),
            rows: self.rows.clone(),
            first_row: row,
            last_row: row,
        });
    }

    fn extend_state_sweep(&mut self, column: StateColumn, row: usize, cx: &mut Context<Self>) {
        let Some(sweep) = &mut self.state_sweep else {
            return;
        };
        if sweep.column != column
            || !Rc::ptr_eq(&sweep.rows, &self.rows)
            || (sweep.first_row..=sweep.last_row).contains(&row)
        {
            return;
        }
        sweep.first_row = sweep.first_row.min(row);
        sweep.last_row = sweep.last_row.max(row);
        cx.notify();
    }

    pub(super) fn preview_state(&self, column: StateColumn, row: usize) -> Option<bool> {
        let sweep = self.state_sweep.as_ref()?;
        (sweep.column == column
            && sweep.first_row != sweep.last_row
            && Rc::ptr_eq(&sweep.rows, &self.rows)
            && (sweep.first_row..=sweep.last_row).contains(&row))
        .then_some(sweep.restricted)
    }

    pub(super) fn cancel_state_sweep(&mut self, cx: &mut Context<Self>) {
        if self
            .state_sweep
            .take()
            .is_some_and(|sweep| sweep.first_row != sweep.last_row)
        {
            cx.notify();
        }
    }

    pub(super) fn release_state_sweep(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(sweep) = self.state_sweep.take() else {
            return false;
        };
        if sweep.first_row == sweep.last_row {
            return false;
        }
        if Rc::ptr_eq(&sweep.rows, &self.rows) && !self.rows_dirty {
            let objects = sweep.rows[sweep.first_row..=sweep.last_row]
                .iter()
                .filter_map(|row| row.node.reference())
                .collect();
            locking::execute(
                &self.owner,
                sweep.document,
                sweep.column.command(objects, sweep.restricted),
                window,
                cx,
            );
        }
        cx.notify();
        true
    }
}

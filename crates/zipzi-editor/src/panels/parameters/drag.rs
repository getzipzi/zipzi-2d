//! Captured track and Pad targets, applied before scene preparation.
use super::{track, view::LinkedRole, *};
use crate::frame_input::{FrameInput, Offer};
use std::{cell::Cell, rc::Rc};
use zipzi_editor_core::{PanelKind, ParameterPreviewInput};

#[derive(Clone, Copy, PartialEq, Eq)]
struct DragToken {
    document: DocumentId,
    slider: EntityId,
    serial: u64,
}

#[derive(Clone, Copy)]
enum DragRows {
    Track(usize),
    Pad {
        top: usize,
        bottom: usize,
        role: LinkedRole,
    },
}

impl DragRows {
    fn indices(self) -> impl Iterator<Item = usize> {
        let (first, second) = match self {
            Self::Track(row) => (row, None),
            Self::Pad { top, bottom, .. } => (top, Some(bottom)),
        };
        std::iter::once(first).chain(second)
    }
}

struct Drag {
    source: ParameterPreviewInput,
    slider: Entity<SliderState>,
    bounds: Bounds<Pixels>,
    ui_scale: f32,
    rows: DragRows,
    press: Point<Pixels>,
    sequence: u64,
    rejected: bool,
}

struct Target {
    sequence: u64,
    trace: u64,
    values: Vec<(String, f32)>,
}

#[derive(Default)]
pub(super) struct ParameterDragState {
    // InputRegion can snapshot the capture even while the panel is preparing
    // another operation from inside an entity update.
    owner: Rc<Cell<Option<DragToken>>>,
    active: Option<Drag>,
    serial: u64,
    frame: FrameInput<DragToken, Target>,
}

impl ParameterDragState {
    pub(super) fn active(&self) -> bool {
        self.active.is_some()
    }
}

impl ParameterRow {
    pub(super) fn sync_value_display(
        &self,
        preserve_focused_text: bool,
        window: &mut Window,
        cx: &mut App,
    ) {
        let value = self
            .desired
            .map_or(self.parameter.value, |(_, value)| value);
        if self.slider.read(cx).value() != SliderValue::Single(value) {
            self.slider
                .update(cx, |slider, cx| slider.set_value(value, window, cx));
        }
        if !self.input_dirty
            && !(preserve_focused_text && self.input.read(cx).focus_handle(cx).is_focused(window))
        {
            let text = format_value(value);
            if self.input.read(cx).value().as_ref() != text {
                self.input
                    .update(cx, |input, cx| input.set_value(text, window, cx));
            }
        }
    }
}

impl ParametersPanel {
    #[cfg(test)]
    pub(crate) fn parameter_display_in_test(&self, id: &str, cx: &App) -> (f32, String) {
        let row = self.rows.iter().find(|row| row.parameter.id == id).unwrap();
        let SliderValue::Single(value) = row.slider.read(cx).value() else {
            unreachable!("parameter rows have one slider value");
        };
        (value, row.input.read(cx).value().to_string())
    }

    pub(crate) fn input_region(
        panel: &Entity<Self>,
        cx: &App,
    ) -> crate::input_routing::InputRegion {
        use crate::input_routing::{CaptureCancelKey, CapturedInput, InputRegion, LocalTarget};
        let state = panel.read(cx);
        let capture = state.drag.owner.clone();
        let panel = panel.downgrade();
        InputRegion::panel(state.focus.clone(), PanelKind::Parameters).with_capture(
            move |editor, _| {
                let token = capture.get()?;
                let panel = panel.clone();
                Some(CapturedInput::new(
                    LocalTarget::Panel(PanelKind::Parameters),
                    editor.active_id() == Some(token.document) && !editor.editing_input_active(),
                    CaptureCancelKey::Escape,
                    move |window, cx| {
                        let _ = panel.update(cx, |panel, cx| {
                            if panel.drag.owner.get() == Some(token) {
                                panel.cancel_parameter_drag(window, cx);
                            }
                        });
                        None
                    },
                ))
            },
        )
    }

    pub(super) fn begin_slider_drag(
        &mut self,
        document: DocumentId,
        slider: &Entity<SliderState>,
        position: Point<Pixels>,
        move_thumb: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.rows.iter().position(|row| &row.slider == slider) else {
            return;
        };
        self.begin_parameter_drag(
            document,
            slider,
            DragRows::Track(row),
            position,
            move_thumb,
            window,
            cx,
        );
    }

    pub(super) fn begin_pad_drag(
        &mut self,
        document: DocumentId,
        top_id: &str,
        bottom_id: &str,
        slider: &Entity<SliderState>,
        role: LinkedRole,
        position: Point<Pixels>,
        move_thumb: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(top), Some(bottom)) = (
            self.rows.iter().position(|row| row.parameter.id == top_id),
            self.rows
                .iter()
                .position(|row| row.parameter.id == bottom_id),
        ) else {
            return;
        };
        let focused = if role == LinkedRole::Top { top } else { bottom };
        if &self.rows[focused].slider == slider {
            self.begin_parameter_drag(
                document,
                slider,
                DragRows::Pad { top, bottom, role },
                position,
                move_thumb,
                window,
                cx,
            );
        }
    }

    fn begin_parameter_drag(
        &mut self,
        document: DocumentId,
        slider: &Entity<SliderState>,
        rows: DragRows,
        position: Point<Pixels>,
        move_thumb: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = slider.read(cx).bounds();
        if self.document_id != Some(document)
            || !self.slider_interaction_allowed(window, cx)
            || bounds.size.width <= px(0.)
            || bounds.size.height <= px(0.)
        {
            return;
        }
        self.cancel_parameter_drag(window, cx);
        // Settle a focused numeric draft before capturing the track. Its blur
        // can then reconcile the text without writing into this new operation.
        if let Some(row) = self
            .rows
            .iter()
            .find(|row| row.input.read(cx).focus_handle(cx).is_focused(window))
        {
            self.commit_input(document, &row.parameter.id.clone(), window, cx);
        }
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let focused = self
            .rows
            .iter()
            .find(|row| &row.slider == slider)
            .unwrap()
            .parameter
            .id
            .clone();
        let source = owner.update(cx, |owner, cx| {
            if !owner.prepare_property_inputs(document, window, cx) {
                return None;
            }
            let result = owner
                .editor
                .execute_for(document, EditorCommand::FocusParameter(focused));
            let ok = result.is_ok();
            owner.complete_preview_edit(result, cx);
            ok.then(|| owner.editor.parameter_preview_input()).flatten()
        });
        let Some(source) = source else {
            return;
        };
        window.focus(&self.focus, cx);
        self.drag.serial += 1;
        self.drag.owner.set(Some(DragToken {
            document,
            slider: slider.entity_id(),
            serial: self.drag.serial,
        }));
        self.drag.active = Some(Drag {
            source,
            slider: slider.clone(),
            bounds,
            ui_scale: crate::ui::compact::scale(cx),
            rows,
            press: position,
            sequence: 0,
            rejected: false,
        });
        if move_thumb && let Some(target) = self.parameter_target(position, cx) {
            self.apply_drag_target(target, window, cx);
        }
    }

    pub(super) fn parameter_drag_valid(&self, window: &mut Window, cx: &mut App) -> bool {
        let Some(drag) = self.drag.active.as_ref() else {
            return false;
        };
        self.slider_interaction_allowed(window, cx)
            && window.is_visible()
            && self.focus.is_focused(window)
            && drag.ui_scale == crate::ui::compact::scale(cx)
            && drag.bounds == drag.slider.read(cx).bounds()
            && self.owner.upgrade().is_some_and(|owner| {
                let owner = owner.read(cx);
                owner
                    .dock
                    .as_ref()
                    .is_some_and(|dock| dock.displayed(PanelKind::Parameters, cx))
                    && drag.source.valid(&owner.editor)
            })
    }

    pub(super) fn move_slider_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if crate::benchmark::ignore_native_pointer_event() {
            return true;
        }
        let Some(token) = self.drag.owner.get() else {
            return false;
        };
        if event.pressed_button != Some(MouseButton::Left) || !self.parameter_drag_valid(window, cx)
        {
            self.cancel_parameter_drag(window, cx);
            return false;
        }
        let Some(target) = self.parameter_target(event.position, cx) else {
            return true;
        };
        // A stationary or quantized duplicate cannot delay the first setter.
        // Once a batch is open it can still replace a different pending target.
        if !self.drag.frame.is_scheduled(token) && !self.drag_target_changes_values(&target) {
            self.apply_drag_target(target, window, cx);
            return true;
        }
        let trace = target.trace;
        self.show_drag_target(&target, window, cx);
        match self.drag.frame.offer(token, target, |_, _| {}) {
            Offer::ApplyNow(target, ticket) => {
                let panel = cx.weak_entity();
                window.on_next_frame(move |window, cx| {
                    let _ = panel.update(cx, |panel, cx| {
                        let Some(target) = panel.drag.frame.take_for_frame(ticket) else {
                            return;
                        };
                        if panel.drag.owner.get() == Some(ticket.owner) {
                            let trace = target.trace;
                            frame_trace::emit(trace, "parameter_frame_begin", &[]);
                            panel.apply_drag_target(target, window, cx);
                            frame_trace::emit(trace, "parameter_frame_end", &[]);
                        }
                    });
                });
                self.apply_drag_target(target, window, cx);
            }
            Offer::Retained => frame_trace::emit(trace, "parameter_target_queued", &[]),
        }
        true
    }

    pub(super) fn release_parameter_drag(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = self.drag.active.as_ref() else {
            return false;
        };
        let apply =
            (drag.sequence != 0 || drag.press != position) && self.parameter_drag_valid(window, cx);
        let pending = self.drag.frame.drain_for_barrier();
        if apply && let Some(target) = self.parameter_target(position, cx).or(pending) {
            self.apply_drag_target(target, window, cx);
        }
        self.cancel_parameter_drag(window, cx);
        true
    }

    pub(crate) fn cancel_parameter_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.drag.frame.invalidate();
        self.drag.owner.set(None);
        if let Some(drag) = self.drag.active.take() {
            for index in drag.rows.indices() {
                let row = &mut self.rows[index];
                row.desired = None;
                row.sync_value_display(false, window, cx);
            }
        }
    }

    fn parameter_target(&mut self, position: Point<Pixels>, cx: &App) -> Option<Target> {
        if !position.x.as_f32().is_finite() || !position.y.as_f32().is_finite() {
            return None;
        }
        let drag = self.drag.active.as_mut()?;
        let values = match drag.rows {
            DragRows::Track(index) => {
                let parameter = &self.rows[index].parameter;
                let fraction =
                    ((position.x - drag.bounds.left()) / drag.bounds.size.width).clamp(0., 1.);
                let raw = parameter.minimum + (parameter.maximum - parameter.minimum) * fraction;
                let step = track::parameter_step(parameter);
                let value =
                    ((raw / step).round() * step).clamp(parameter.minimum, parameter.maximum);
                vec![(parameter.id.clone(), value)]
            }
            DragRows::Pad { top, bottom, role } => {
                let top = &self.rows[top].parameter;
                let bottom = &self.rows[bottom].parameter;
                let (x, y) = track::pad_values_at_position(
                    top,
                    bottom,
                    track::pad_full_bounds(drag.bounds, role),
                    position,
                    cx,
                );
                vec![(top.id.clone(), x), (bottom.id.clone(), y)]
            }
        };
        if values.iter().any(|(_, value)| !value.is_finite()) {
            return None;
        }
        drag.sequence += 1;
        let trace = frame_trace::current_input();
        frame_trace::emit(
            trace,
            "parameter_target",
            &[("sequence", drag.sequence), ("axes", values.len() as u64)],
        );
        Some(Target {
            sequence: drag.sequence,
            trace,
            values,
        })
    }

    fn show_drag_target(&mut self, target: &Target, window: &mut Window, cx: &mut Context<Self>) {
        let drag = self.drag.active.as_ref().unwrap();
        for (index, (_, value)) in drag.rows.indices().zip(&target.values) {
            let row = &mut self.rows[index];
            row.desired = Some((target.sequence, *value));
            row.sync_value_display(false, window, cx);
        }
    }

    fn drag_target_changes_values(&self, target: &Target) -> bool {
        self.drag
            .active
            .as_ref()
            .unwrap()
            .rows
            .indices()
            .zip(&target.values)
            .any(|(index, (_, value))| self.rows[index].parameter.value != *value)
    }

    fn apply_drag_target(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        if !self.parameter_drag_valid(window, cx) {
            self.cancel_parameter_drag(window, cx);
            return;
        }
        let changed = self.drag_target_changes_values(&target);
        let drag = self.drag.active.as_mut().unwrap();
        let rows = drag.rows;
        let outcome = if changed {
            let trace = target.trace;
            self.owner
                .update(cx, |owner, cx| {
                    frame_trace::emit(
                        trace,
                        "parameter_setter_begin",
                        &[
                            ("sequence", target.sequence),
                            ("axes", target.values.len() as u64),
                        ],
                    );
                    let result = crate::benchmark::parameter_overlay::capture(5, || {
                        owner
                            .editor
                            .apply_parameter_preview_input(&mut drag.source, target.values.clone())
                    });
                    frame_trace::emit(
                        trace,
                        "parameter_setter_end",
                        &[(
                            "applied",
                            u64::from(result.as_ref().is_ok_and(|applied| *applied)),
                        )],
                    );
                    match result {
                        Ok(false) => {
                            owner.redraw(cx);
                            None
                        }
                        result => {
                            let applied = result.is_ok();
                            owner.complete_preview_edit_for_input(result.map(|_| ()), trace, cx);
                            Some(applied)
                        }
                    }
                })
                .ok()
                .flatten()
        } else {
            if drag.rejected {
                let _ = self
                    .owner
                    .update(cx, |owner, cx| owner.clear_edit_error(cx));
            }
            Some(true)
        };
        let Some(applied) = outcome else {
            self.cancel_parameter_drag(window, cx);
            return;
        };
        drag.rejected = !applied;
        for (index, (_, value)) in rows.indices().zip(target.values) {
            let row = &mut self.rows[index];
            if applied {
                row.parameter.value = value;
            }
            if row
                .desired
                .is_some_and(|(sequence, _)| sequence <= target.sequence)
            {
                row.desired = None;
            }
            row.sync_value_display(false, window, cx);
        }
    }
}

//! Modeling content, tool input and object menus, independent of workspace layout.
use super::*;
use crate::frame_input::{FrameInput, FrameTicket, Offer};
use zipzi_editor_core::ViewportInteractionToken;

pub struct ModelingViewport {
    pub(crate) surface: Entity<ViewportSurface>,
    _observe: Subscription,
    header: Option<zoom_bar::HeaderState>,
}
impl ModelingViewport {
    pub fn new(
        owner: WeakEntity<EditorView>,
        viewport: zipzi_editor_core::ViewportId,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let surface = ViewportSurface::new(
            owner,
            viewport,
            zipzi_editor_core::ViewportKind::Modeling,
            Self::content,
            if benchmark::parameter_overlay::animation_viewport_style() {
                |_, _, _, _| {}
            } else {
                ViewportSurface::modeling_input
            },
            if benchmark::parameter_overlay::animation_viewport_style() {
                toolbar::ToolbarRegistry::navigation()
            } else {
                toolbar::ToolbarRegistry::with_defaults()
            },
            window,
            cx,
        );
        if !benchmark::parameter_overlay::animation_viewport_style() {
            surface.update(cx, |surface, _| {
                surface.object_menu = Some(ViewportSurface::object_menu)
            });
        }
        cx.new(|cx| Self {
            _observe: cx.observe_in(&surface, window, |this: &mut Self, surface, window, cx| {
                let next = ViewportSurface::header_state(&surface, window, cx);
                if this.header != next {
                    this.header = next;
                    cx.notify();
                }
            }),
            header: None,
            surface,
        })
    }
    fn content(owner: &EditorView, viewport: zipzi_editor_core::ViewportId) -> SurfaceContent {
        let frame = owner.modeling_frame_for(viewport);
        if benchmark::parameter_overlay::animation_viewport_style() {
            // Animation presentation over the same Modeling frames: changing the
            // core kind would reject them and require the motion worker instead.
            return SurfaceContent::navigation(
                frame.map(|f| (f.image.clone(), f.canvas.transform.metrics)),
                owner.viewport_error(viewport).cloned(),
            );
        }
        let overlay = benchmark::parameter_overlay::capture(1, || {
            owner
                .editor
                .overlay_for_frame(frame.map(|f| f.canvas.version))
        });
        SurfaceContent {
            image: frame.map(|f| (f.image.clone(), f.canvas.transform.metrics)),
            overlay,
            error: owner.viewport_error(viewport).cloned(),
            hand_tool: owner.editor.active_tool() == ToolId::Hand,
            controls: Some(modeling_toolbar::render_secondary),
        }
    }
}
viewport_panel!(ModelingViewport, "dock.modeling_viewport", 260.);

#[derive(Default)]
pub(super) struct MeshInputScheduling {
    preview: FrameInput<ViewportInteractionToken, PreviewDemand>,
    preview_modifiers: (bool, bool),
}

/// All modeling views of the captured document share the dirty scene revision.
/// Request preparation from the current candidate, using each view's own projection.
struct PreviewDemand {
    first_input: u64,
    latest_input: u64,
    updates: u64,
}

impl PreviewDemand {
    fn new(input: u64) -> Self {
        Self {
            first_input: input,
            latest_input: input,
            updates: 1,
        }
    }

    fn merge(previous: Self, latest: &mut Self) {
        latest.first_input = previous.first_input;
        latest.updates += previous.updates;
    }

    fn trace(&self, input: u64, stage: &'static str) {
        frame_trace::emit(
            input,
            stage,
            &[
                ("first_input", self.first_input),
                ("latest_input", self.latest_input),
                ("updates", self.updates),
            ],
        );
    }
}

impl ViewportSurface {
    #[cfg(test)]
    pub(crate) fn mesh_input_pending(&self) -> bool {
        self.mesh_input.preview.has_pending()
    }

    pub(super) fn discard_mesh_input(&mut self) {
        self.mesh_input.preview.invalidate();
    }

    fn offer_mesh_preview(
        &mut self,
        token: ViewportInteractionToken,
        trace: u64,
        surface: WeakEntity<Self>,
        window: &mut Window,
    ) -> bool {
        match self
            .mesh_input
            .preview
            .offer(token, PreviewDemand::new(trace), PreviewDemand::merge)
        {
            Offer::Retained => {
                frame_trace::emit(trace, "mesh_scene_queued", &[]);
                false
            }
            Offer::ApplyNow(demand, ticket) => {
                window.on_next_frame(move |window, cx| {
                    let _ = surface.update(cx, |surface, cx| {
                        surface.flush_mesh_preview(ticket, window, cx);
                    });
                });
                demand.trace(trace, "mesh_scene_leading");
                true
            }
        }
    }

    fn flush_mesh_preview(
        &mut self,
        ticket: FrameTicket<ViewportInteractionToken>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(demand) = self.mesh_input.preview.take_for_frame(ticket) else {
            return;
        };
        if self.interaction != Some(ticket.owner) {
            return;
        }
        if window.has_active_dialog(cx) {
            self.cancel(cx);
            return;
        }
        let _ = self.owner.update(cx, |owner, cx| {
            if owner.modal
                || owner.editor.is_busy()
                || owner.mesh_generation.is_some()
                || !owner.editor.viewport_interaction_valid(ticket.owner)
            {
                return;
            }
            let trace = demand.latest_input;
            demand.trace(trace, "mesh_frame_begin");
            owner.redraw_canvas_for_input(cx, trace);
            frame_trace::emit(trace, "mesh_frame_end", &[]);
        });
    }

    pub(super) fn modeling_input(
        &mut self,
        input: CanvasInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let trace = frame_trace::current_input();
        let modifiers = window.modifiers();
        if matches!(
            input,
            CanvasInput::Down { .. } | CanvasInput::BrushResizeDown { .. }
        ) {
            self.navigation.wheel_zoom = None;
        }
        if window.has_active_dialog(cx) {
            self.cancel(cx);
            return;
        }
        let owner = self.owner.clone();
        let surface = cx.weak_entity();
        let _ = owner.update(cx, |owner, cx| {
            if owner.mesh_generation.is_some() {
                return;
            }
            if owner.modal
                || owner.editor.is_busy()
                || owner.editor.editing_context()
                    != Some(zipzi_editor_core::EditingContext::Modeling)
            {
                return;
            }
            if self
                .interaction
                .is_some_and(|token| !owner.editor.viewport_interaction_valid(token))
            {
                self.cancel_pointer();
                if !matches!(
                    input,
                    CanvasInput::Down { .. } | CanvasInput::BrushResizeDown { .. }
                ) {
                    return;
                }
            }
            let Some(binding) = self
                .binding(&owner.editor)
                .filter(|binding| binding.kind == zipzi_editor_core::ViewportKind::Modeling)
            else {
                return;
            };
            if matches!(input, CanvasInput::Up { .. }) && self.interaction.is_none() {
                return;
            }
            if !owner.editor.has_presented_frame(
                owner
                    .modeling_frame_for(self.viewport)
                    .map(|f| f.canvas.version),
            ) {
                // A revisited tab can retain geometry before its image arrives.
                // Never hit-test controls that the user cannot yet see.
                let editing = self
                    .cancel_pointer()
                    .is_some_and(|token| owner.editor.cancel_viewport_interaction(token))
                    || owner.editor.cancel_gesture_for(binding);
                if editing {
                    owner.redraw(cx);
                } else {
                    // Layout can deliver hover/leave callbacks before the first
                    // image arrives. There is no document edit to reconcile.
                    owner.redraw_canvas(cx);
                }
                return;
            }
            let moving = matches!(
                input,
                CanvasInput::Move { .. }
                    | CanvasInput::BrushTick { .. }
                    | CanvasInput::Leave
                    | CanvasInput::ModifiersChanged { .. }
            );
            let mesh_token = self
                .interaction
                .filter(|token| owner.editor.mesh_vertex_drag_for(token.binding));
            let preview_move = matches!(
                input,
                CanvasInput::Move {
                    left_down: true,
                    ..
                }
            ) && mesh_token.is_some();
            let modifiers_changed = preview_move
                && self.mesh_input.preview_modifiers != (modifiers.shift, modifiers.alt);
            let release_demand = match input {
                CanvasInput::Up { .. } => self.mesh_input.preview.drain_for_barrier(),
                CanvasInput::Down { .. }
                | CanvasInput::BrushResizeDown { .. }
                | CanvasInput::Cancel
                | CanvasInput::Move {
                    left_down: false, ..
                } => {
                    self.mesh_input.preview.invalidate();
                    None
                }
                _ => {
                    if (modifiers_changed || matches!(input, CanvasInput::ModifiersChanged { .. }))
                        && let Some(demand) = self.mesh_input.preview.drain_for_barrier()
                    {
                        demand.trace(demand.latest_input, "mesh_scene_modifier_barrier");
                        owner.redraw_canvas_for_input(cx, demand.latest_input);
                    }
                    None
                }
            };
            let trace_candidate = mesh_token.is_some() && !matches!(input, CanvasInput::Leave);
            if trace_candidate {
                frame_trace::emit(trace, "mesh_candidate_begin", &[]);
            }
            let before_selection = matches!(input, CanvasInput::Down { .. }).then(|| {
                owner
                    .editor
                    .document()
                    .and_then(|doc| doc.context().selection.clone())
            });
            let result = match input {
                CanvasInput::BrushResizeDown { point } => {
                    let modifiers = window.modifiers();
                    owner
                        .editor
                        .update_canvas_modifiers(binding, modifiers.shift, modifiers.alt)
                        .and_then(|_| owner.editor.begin_brush_size_gesture(binding, point))
                }
                CanvasInput::Down {
                    point,
                    shift,
                    alt,
                    primary,
                } => owner
                    .editor
                    .begin_canvas_gesture(binding, point, shift, alt, primary),
                input @ (CanvasInput::Move { .. } | CanvasInput::Up { .. })
                    if self.interaction.is_some() =>
                {
                    owner.editor.update_canvas_gesture_with_modifiers(
                        self.interaction.unwrap(),
                        input,
                        modifiers.shift,
                        modifiers.alt,
                    )
                }
                input if self.interaction.is_some() => owner
                    .editor
                    .update_canvas_gesture(self.interaction.unwrap(), input),
                CanvasInput::Move { point, .. } => {
                    // The pointer may enter this window with Alt already held.
                    // Synchronize access feedback before picking its hovered handle.
                    let modifiers = window.modifiers();
                    owner
                        .editor
                        .update_canvas_modifiers(binding, modifiers.shift, modifiers.alt)
                        .and_then(|_| owner.editor.hover_canvas(binding, Some(point)))
                        .map(|_| Default::default())
                }
                CanvasInput::Leave => owner
                    .editor
                    .hover_canvas(binding, None)
                    .map(|_| Default::default()),
                CanvasInput::ModifiersChanged { shift, alt } => owner
                    .editor
                    .update_canvas_modifiers(binding, shift, alt)
                    .map(|_| Default::default()),
                _ => return,
            };
            if trace_candidate {
                frame_trace::emit(
                    trace,
                    "mesh_candidate_end",
                    &[(
                        "changed",
                        u64::from(result.as_ref().is_ok_and(|update| update.model_changed)),
                    )],
                );
            }
            match result {
                Ok(update) => {
                    owner.sync_viewport_notice(cx);
                    let previous = self.interaction;
                    self.interaction = update.capture;
                    if previous != self.interaction {
                        self.discard_mesh_input();
                        crate::workspace::status_bar::StatusBar::sync(&owner.status_bar, owner, cx);
                    }
                    if matches!(
                        input,
                        CanvasInput::Down { .. } | CanvasInput::BrushResizeDown { .. }
                    ) {
                        owner.clear_edit_error(cx);
                    }
                    if let Some(demand) = &release_demand {
                        demand.trace(trace, "mesh_scene_release");
                    }
                    let selected_glue = before_selection.is_some_and(|previous| {
                        owner
                            .editor
                            .document()
                            .and_then(|doc| doc.context().selection.as_ref())
                            .is_some_and(|selected| {
                                selected.kind == zipzi_editor_core::ObjectKind::Glue
                                    && Some(selected) != previous.as_ref()
                            })
                    });
                    if selected_glue {
                        owner.redraw(cx);
                    } else if update.model_changed || release_demand.is_some() {
                        if moving {
                            let immediate = if preview_move && let Some(token) = self.interaction {
                                self.mesh_input.preview_modifiers =
                                    (modifiers.shift, modifiers.alt);
                                self.offer_mesh_preview(token, trace, surface, window)
                            } else {
                                true
                            };
                            if immediate {
                                owner.redraw_canvas_for_input(cx, trace);
                            } else {
                                owner.notify_canvas(cx);
                                crate::workspace::status_bar::StatusBar::sync(
                                    &owner.status_bar,
                                    owner,
                                    cx,
                                );
                            }
                        } else {
                            owner.redraw(cx);
                        }
                    } else if moving {
                        crate::workspace::status_bar::StatusBar::sync(&owner.status_bar, owner, cx);
                        owner.notify_canvas(cx);
                    } else {
                        crate::workspace::status_bar::StatusBar::sync(&owner.status_bar, owner, cx);
                        cx.notify();
                    }
                }
                Err(e) => {
                    self.discard_mesh_input();
                    self.interaction = self
                        .interaction
                        .filter(|token| owner.editor.viewport_interaction_valid(*token));
                    owner.set_edit_error(e.to_string(), cx);
                    owner.redraw(cx);
                }
            }
        });
    }
}

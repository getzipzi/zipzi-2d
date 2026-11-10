//! Modeling content, tool input and object menus, independent of workspace layout.
use super::*;

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

pub(super) struct PendingMeshMove {
    input: CanvasInput,
    token: zipzi_editor_core::ViewportInteractionToken,
    trace: u64,
    modifiers: Modifiers,
}

impl ViewportSurface {
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn queued_mesh_move(&self) -> bool {
        self.pending_mesh_move.is_some()
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
                CanvasInput::Move {
                    left_down: true,
                    ..
                }
            )
            // Shift can latch the axis from an early sample. Preserve that order.
            && !modifiers.shift
            && let Some(token) = self.interaction
            && std::env::var_os("ZIPZI_MESH_MOVE_COALESCE").as_deref() == Some("2".as_ref())
            && !window.has_active_dialog(cx)
            && let Some(edit_started) = self.owner.upgrade().and_then(|owner| {
                let owner = owner.read(cx);
                (!owner.modal
                    && !owner.editor.is_busy()
                    && owner.mesh_generation.is_none()
                    && owner.editor.viewport_interaction_valid(token)
                    && owner.editor.mesh_vertex_drag_for(token.binding))
                    .then(|| owner.editor.document().is_some_and(|doc| doc.has_pending_edit()))
            })
        {
            if self.mesh_move_frame == Some(token) && edit_started {
                frame_trace::emit(trace, "mesh_move_queued", &[]);
                self.pending_mesh_move = Some(PendingMeshMove {
                    input,
                    token,
                    trace,
                    modifiers,
                });
                return;
            }
            if self.mesh_move_frame != Some(token) {
                self.mesh_move_frame = Some(token);
                let surface = cx.weak_entity();
                window.on_next_frame(move |window, cx| {
                    let _ = surface.update(cx, |surface, cx| {
                        if surface.mesh_move_frame == Some(token) {
                            surface.mesh_move_frame = None;
                            surface.flush_mesh_move(window, cx);
                        }
                    });
                });
            }
            // Keep processing the first moves until the drag threshold starts an edit.
            frame_trace::emit(trace, "mesh_move_immediate", &[]);
            self.apply_modeling_input(input, trace, modifiers, window, cx);
            return;
        }
        if matches!(
            input,
            CanvasInput::Up { .. }
                | CanvasInput::Cancel
                | CanvasInput::Down { .. }
                | CanvasInput::BrushResizeDown { .. }
                | CanvasInput::Move {
                    left_down: false,
                    ..
                }
        ) {
            // Up evaluates its own final position before committing in core.
            self.discard_mesh_move();
        } else {
            self.flush_mesh_move(window, cx);
        }
        self.apply_modeling_input(input, trace, modifiers, window, cx);
    }

    pub(super) fn discard_mesh_move(&mut self) {
        self.pending_mesh_move = None;
        self.mesh_move_frame = None;
    }

    fn flush_mesh_move(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(sample) = self.pending_mesh_move.take()
            && self.interaction == Some(sample.token)
        {
            self.apply_modeling_input(sample.input, sample.trace, sample.modifiers, window, cx);
        }
    }

    fn apply_modeling_input(
        &mut self,
        input: CanvasInput,
        trace: u64,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
                    .interaction
                    .take()
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
            let result = match input {
                CanvasInput::BrushResizeDown { point } => {
                    owner.editor.begin_brush_size_gesture(binding, point)
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
            match result {
                Ok(update) => {
                    owner.sync_viewport_notice(cx);
                    let previous = self.interaction;
                    self.interaction = update.capture;
                    if previous != self.interaction {
                        crate::workspace::status_bar::StatusBar::sync(&owner.status_bar, owner, cx);
                    }
                    if matches!(
                        input,
                        CanvasInput::Down { .. } | CanvasInput::BrushResizeDown { .. }
                    ) {
                        owner.clear_edit_error(cx);
                    }
                    if update.model_changed {
                        if moving {
                            owner.redraw_canvas_for_input(cx, trace);
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

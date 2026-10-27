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
            ViewportSurface::modeling_input,
            toolbar::ToolbarRegistry::with_defaults(),
            window,
            cx,
        );
        surface.update(cx, |surface, _| {
            surface.object_menu = Some(ViewportSurface::object_menu)
        });
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
        let overlay = benchmark::parameter_overlay::capture(1, || {
            owner
                .editor
                .overlay_for_frame(frame.map(|f| f.canvas.version))
        });
        let active_handle = overlay.active_handle;
        let cursor = if overlay.point_unavailable.is_some() {
            Some(CursorStyle::OperationNotAllowed)
        } else {
            active_handle
                .map(|handle| {
                    use zipzi_editor_core::DeformerHandle as H;
                    match handle {
                        H::Move => CursorStyle::OpenHand,
                        H::Rotate => CursorStyle::Arrow,
                        H::RotationScale => CursorStyle::Crosshair,
                        H::Scale(1 | 5) => CursorStyle::ResizeUpDown,
                        H::Scale(3 | 7) => CursorStyle::ResizeLeftRight,
                        H::Scale(0 | 4) => CursorStyle::ResizeUpLeftDownRight,
                        H::Scale(_) => CursorStyle::ResizeUpRightDownLeft,
                    }
                })
                .or_else(|| {
                    if overlay.point_unavailable.is_some() {
                        Some(CursorStyle::OperationNotAllowed)
                    } else if overlay.active_component.is_some() {
                        if overlay.glues.iter().flat_map(|g| &g.points).any(|p| {
                            p.hovered && p.state == zipzi_editor_core::GluePointState::Editable
                        }) {
                            Some(if owner.editor.current_viewport_interaction().is_some() {
                                CursorStyle::ClosedHand
                            } else {
                                CursorStyle::OpenHand
                            })
                        } else {
                            Some(CursorStyle::Arrow)
                        }
                    } else if let Some(tool) = owner.editor.manual_tool() {
                        Some(match tool {
                            zipzi_editor_core::ManualTool::Select
                                if overlay.vertices.iter().any(|v| v.hovered) =>
                            {
                                if owner.editor.current_viewport_interaction().is_some() {
                                    CursorStyle::ClosedHand
                                } else {
                                    CursorStyle::OpenHand
                                }
                            }
                            zipzi_editor_core::ManualTool::Delete => CursorStyle::Arrow,
                            _ => CursorStyle::Crosshair,
                        })
                    } else if owner.editor.active_tool() == ToolId::Lasso {
                        Some(CursorStyle::Crosshair)
                    } else {
                        None
                    }
                })
        };
        SurfaceContent {
            image: frame.map(|f| (f.image.clone(), f.canvas.transform.metrics)),
            overlay,
            error: owner.viewport_error(viewport).cloned(),
            hand_tool: owner.editor.active_tool() == ToolId::Hand,
            controls: Some(deformer_bar::render_deformer_bar),
            cursor,
        }
    }
}
viewport_panel!(ModelingViewport, "dock.modeling_viewport", 232.);

impl ViewportSurface {
    pub(super) fn modeling_input(
        &mut self,
        input: CanvasInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(input, CanvasInput::Down { .. }) {
            self.navigation.wheel_zoom = None;
        }
        if window.has_active_dialog(cx) {
            self.cancel(cx);
            return;
        }
        let owner = self.owner.clone();
        let _ = owner.update(cx, |owner, cx| {
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
                if !matches!(input, CanvasInput::Down { .. }) {
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
            let moving = matches!(input, CanvasInput::Move { .. } | CanvasInput::Leave);
            let result = match input {
                CanvasInput::Down {
                    point,
                    shift,
                    alt,
                    primary,
                } => owner
                    .editor
                    .begin_canvas_gesture(binding, point, shift, alt, primary),
                input if self.interaction.is_some() => owner
                    .editor
                    .update_canvas_gesture(self.interaction.unwrap(), input),
                CanvasInput::Move { point, .. } => owner
                    .editor
                    .hover_canvas(binding, Some(point))
                    .map(|_| Default::default()),
                CanvasInput::Leave => owner
                    .editor
                    .hover_canvas(binding, None)
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
                    if matches!(input, CanvasInput::Down { .. }) {
                        owner.clear_edit_error(cx);
                    }
                    if update.model_changed {
                        if moving {
                            owner.redraw_canvas(cx);
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

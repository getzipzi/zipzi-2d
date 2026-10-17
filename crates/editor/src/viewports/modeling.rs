//! Modeling content, tool input and object menus, independent of workspace layout.
use super::*;

pub struct ModelingViewport {
    pub(crate) surface: Entity<ViewportSurface>,
    _observe: Subscription,
    header: Option<(i32, [bool; 3])>,
}
impl ModelingViewport {
    pub fn new(
        owner: WeakEntity<EditorView>,
        viewport: editor_core::ViewportId,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        let surface = ViewportSurface::new(
            owner,
            viewport,
            editor_core::ViewportKind::Modeling,
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
    fn content(owner: &EditorView, viewport: editor_core::ViewportId) -> SurfaceContent {
        let frame = owner.modeling_frame_for(viewport);
        let overlay = owner
            .editor
            .overlay_for_frame(frame.map(|f| f.canvas.version));
        let vertex_notice = overlay.unavailable.clone();
        let active_handle = overlay.active_handle;
        let interaction_label = overlay.interaction_label.clone().or_else(|| {
            active_handle.map(|handle| {
                use editor_core::DeformerHandle as H;
                t!(match handle {
                    H::Move => "editor.deformer_move_hint",
                    H::Rotate => "editor.deformer_rotate_hint",
                    H::Scale(_) | H::RotationScale => "editor.deformer_scale_hint",
                })
                .into_owned()
            })
        });
        let target_label = overlay.target_label.as_ref().map(|label| {
            if label == "Base shape" {
                t!("editor.edit_base").into_owned()
            } else if let Some(keys) = label.strip_prefix("Keyform · ") {
                format!("{} · {}", t!("editor.edit_keyform"), keys)
            } else {
                label.clone()
            }
        });
        let cursor = active_handle.map(|handle| {
            use editor_core::DeformerHandle as H;
            match handle {
                H::Move => CursorStyle::OpenHand,
                H::Rotate => CursorStyle::Crosshair,
                H::Scale(1 | 5) => CursorStyle::ResizeUpDown,
                H::Scale(3 | 7) => CursorStyle::ResizeLeftRight,
                H::Scale(0 | 4) => CursorStyle::ResizeUpLeftDownRight,
                H::Scale(_) => CursorStyle::ResizeUpRightDownLeft,
                H::RotationScale => CursorStyle::Crosshair,
            }
        });
        SurfaceContent {
            image: frame.map(|f| (f.image.clone(), f.canvas.transform.metrics)),
            overlay,
            error: owner.viewport_error(viewport).cloned(),
            hand_tool: owner.editor.active_tool() == ToolId::Hand,
            controls: Some(deformer_bar::render_deformer_bar),
            cursor,
            interaction_label,
            target_label,
            vertex_notice,
        }
    }
}
viewport_panel!(ModelingViewport, "dock.modeling_viewport");

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
                || owner.editor.editing_context() != Some(editor_core::EditingContext::Modeling)
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
                .filter(|binding| binding.kind == editor_core::ViewportKind::Modeling)
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
                self.sync_capture_hint();
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
            if matches!(input, CanvasInput::Down { .. }) {
                match owner.editor.begin_viewport_interaction(
                    binding,
                    editor_core::ViewportInteractionKind::Modeling,
                ) {
                    Ok(token) => {
                        self.interaction = Some(token);
                        self.sync_capture_hint();
                    }
                    Err(_) => return,
                }
            }
            let result = if let Some(token) = self.interaction {
                owner.editor.canvas_interaction_input(token, input)
            } else {
                owner.editor.canvas_input_for(binding, input)
            };
            if matches!(input, CanvasInput::Up { .. }) || result.is_err() {
                if let Some(token) = self.interaction.take() {
                    if result.is_err() {
                        owner.editor.cancel_viewport_interaction(token);
                    } else {
                        owner.editor.finish_viewport_interaction(token);
                    }
                }
                self.sync_capture_hint();
            }
            match result {
                Ok(changed) => {
                    if matches!(input, CanvasInput::Down { .. }) {
                        owner.clear_edit_error(cx);
                    }
                    if changed {
                        if moving {
                            owner.redraw_canvas(cx);
                        } else {
                            owner.redraw(cx);
                        }
                    } else if moving {
                        owner.notify_canvas(cx);
                    } else {
                        crate::workspace::status_bar::StatusBar::sync(&owner.status_bar, owner, cx);
                        cx.notify();
                    }
                }
                Err(e) => {
                    owner.set_edit_error(e.to_string(), cx);
                    owner.redraw(cx);
                }
            }
        });
    }
}

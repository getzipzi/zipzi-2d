//! One core lifecycle for UI captures and direct canvas input.
use crate::{CanvasInput, Editor, InteractionHint, ViewportBinding, ViewportKind};
use anyhow::{Result, ensure};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewportInteractionKind {
    Navigation,
    Modeling,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ViewportInteractionToken {
    pub binding: ViewportBinding,
    pub kind: ViewportInteractionKind,
    serial: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ViewportInputUpdate {
    pub model_changed: bool,
    pub capture: Option<ViewportInteractionToken>,
}

impl Editor {
    fn acquire_viewport_interaction(
        &mut self,
        binding: ViewportBinding,
        kind: ViewportInteractionKind,
    ) -> Result<ViewportInteractionToken> {
        ensure!(
            self.viewport_binding_valid(binding),
            "Viewport binding changed"
        );
        if let Some(previous) = self.viewport_interaction {
            self.cancel_viewport_interaction(previous);
        }
        self.next_viewport_interaction += 1;
        let token = ViewportInteractionToken {
            binding,
            kind,
            serial: self.next_viewport_interaction,
        };
        self.viewport_interaction = Some(token);
        Ok(token)
    }
    pub fn viewport_interaction_valid(&self, token: ViewportInteractionToken) -> bool {
        self.viewport_interaction == Some(token) && self.viewport_binding_valid(token.binding)
    }
    pub fn cancel_viewport_interaction(&mut self, token: ViewportInteractionToken) -> bool {
        if self.viewport_interaction != Some(token) {
            return false;
        }
        self.viewport_interaction = None;
        if token.kind == ViewportInteractionKind::Modeling
            && self.active_id() == Some(token.binding.document)
            && self.gesture_active()
        {
            self.cancel_gesture();
        }
        true
    }
    pub fn current_viewport_interaction(&self) -> Option<ViewportInteractionToken> {
        self.viewport_interaction
            .filter(|token| self.viewport_interaction_valid(*token))
    }
    pub fn viewport_interaction_hints(
        &self,
        token: ViewportInteractionToken,
    ) -> &'static [InteractionHint] {
        if !self.viewport_interaction_valid(token) {
            return &[];
        }
        match token.kind {
            ViewportInteractionKind::Navigation => crate::tools::navigation_hints(),
            ViewportInteractionKind::Modeling => self
                .active_session()
                .map_or(&[], |s| s.modeling.tools.interaction_hints()),
        }
    }
    pub fn begin_viewport_navigation(
        &mut self,
        binding: ViewportBinding,
    ) -> Result<ViewportInteractionToken> {
        self.acquire_viewport_interaction(binding, ViewportInteractionKind::Navigation)
    }

    /// Validate the displayed pixels before replacing the previous owner.
    pub fn begin_canvas_gesture(
        &mut self,
        binding: ViewportBinding,
        point: crate::ViewportPoint,
        shift: bool,
        alt: bool,
    ) -> Result<ViewportInputUpdate> {
        self.validate_modeling_binding(binding)?;
        self.validate_modeling_press(binding)?;
        let token =
            self.acquire_viewport_interaction(binding, ViewportInteractionKind::Modeling)?;
        self.process_canvas_gesture(token, CanvasInput::Down { point, shift, alt })
    }

    /// Continue the captured gesture. A new press must acquire a new owner through begin.
    pub fn update_canvas_gesture(
        &mut self,
        token: ViewportInteractionToken,
        input: CanvasInput,
    ) -> Result<ViewportInputUpdate> {
        ensure!(
            self.viewport_interaction_valid(token)
                && token.kind == ViewportInteractionKind::Modeling,
            "Interaction owner changed"
        );
        ensure!(
            !matches!(input, CanvasInput::Down { .. }),
            "Begin a new canvas gesture for a pointer press"
        );
        self.process_canvas_gesture(token, input)
    }

    /// Uncaptured hover never cancels a gesture, including one owned by a peer viewport.
    pub fn hover_canvas(
        &mut self,
        binding: ViewportBinding,
        point: Option<crate::ViewportPoint>,
    ) -> Result<()> {
        self.validate_modeling_binding(binding)?;
        if self.viewport_interaction_active() {
            return Ok(());
        }
        let input = point.map_or(CanvasInput::Leave, |point| CanvasInput::Move {
            point,
            left_down: false,
        });
        self.modeling_input(binding, input).map(|_| ())
    }

    fn process_canvas_gesture(
        &mut self,
        token: ViewportInteractionToken,
        input: CanvasInput,
    ) -> Result<ViewportInputUpdate> {
        let result = self.modeling_input(token.binding, input);
        if result.is_err() || !self.gesture_active_for(token.binding) {
            self.cancel_viewport_interaction(token);
        }
        result.map(|model_changed| ViewportInputUpdate {
            model_changed,
            capture: self
                .current_viewport_interaction()
                .filter(|current| *current == token),
        })
    }

    fn validate_modeling_binding(&self, binding: ViewportBinding) -> Result<()> {
        ensure!(
            self.viewport_binding_valid(binding),
            "Viewport binding changed"
        );
        ensure!(
            binding.kind == ViewportKind::Modeling
                && self.editing_context() == Some(crate::EditingContext::Modeling),
            "Viewport does not support Modeling input"
        );
        Ok(())
    }
    pub(crate) fn validate_modeling_press(&self, binding: ViewportBinding) -> Result<()> {
        let frame = self
            .displayed_modeling_frame(binding)
            .ok_or_else(|| anyhow::anyhow!("Wait for the current model frame"))?;
        let session = self.active_session().unwrap();
        ensure!(
            frame.version.scene == session.scene_revision
                && frame.version.viewport == session.view_state(binding.viewport).revision,
            "Wait for the current model frame"
        );
        Ok(())
    }
}

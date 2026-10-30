//! Modeling content is evaluated once; projections and presented geometry belong to instances.
use super::*;
use crate::{CanvasFrame, FrameVersion, ViewportBinding, ViewportKind};

impl Editor {
    pub fn gesture_active_for(&self, binding: ViewportBinding) -> bool {
        self.viewport_binding_valid(binding)
            && self.active_session().is_some_and(|session| {
                session.modeling.tools.interaction_viewport() == Some(binding.viewport)
            })
    }
    pub fn cancel_gesture_for(&mut self, binding: ViewportBinding) -> bool {
        if !self.gesture_active_for(binding) {
            return false;
        }
        self.cancel_gesture();
        true
    }

    pub(crate) fn modeling_input(
        &mut self,
        binding: ViewportBinding,
        input: crate::CanvasInput,
    ) -> Result<bool> {
        let frame = self
            .viewport_presentation(binding.viewport)
            .and_then(|state| state.modeling.clone());
        self.active_session_mut()
            .unwrap()
            .input(binding.viewport, input, frame)
    }
    pub fn displayed_modeling_frame(&self, binding: ViewportBinding) -> Option<&CanvasFrame> {
        self.viewport_binding_valid(binding).then_some(())?;
        self.viewport_presentation(binding.viewport)?
            .modeling
            .as_ref()
    }
    /// Geometry is usable only when it belongs to the image held by this viewport.
    pub fn has_presented_frame(&self, frame: Option<FrameVersion>) -> bool {
        let Some(frame) = frame else {
            return false;
        };
        self.displayed_modeling_frame(frame.binding)
            .is_some_and(|shown| shown.version == frame)
    }
    pub fn overlay_for_frame(&self, frame: Option<FrameVersion>) -> crate::OverlaySnapshot {
        frame
            .filter(|frame| self.has_presented_frame(Some(*frame)))
            .map_or_else(Default::default, |frame| {
                self.overlay_for_viewport(frame.binding)
            })
    }
    pub fn overlay_for_viewport(&self, binding: ViewportBinding) -> crate::OverlaySnapshot {
        self.displayed_modeling_frame(binding)
            .zip(self.active_session())
            .map_or_else(Default::default, |(frame, session)| session.overlay(frame))
    }
    pub fn canvas_hover_hint(&self) -> Option<String> {
        let session = self.active_session()?;
        if session.modeling.tools.dragging() {
            return None;
        }
        let binding = session.modeling.tools.hovered_binding()?;
        let frame = self.displayed_modeling_frame(binding)?;
        session.modeling.tools.hover_hint(session.document(), frame)
    }
    /// Persistent edit restrictions for the primary presented canvas; no vertex/edge allocation.
    pub fn canvas_notice(&self) -> Option<String> {
        let frame = self.displayed_modeling_frame(self.primary_viewport_binding()?)?;
        let session = self.active_session()?;
        session.modeling.tools.notice(session.document(), frame)
    }
    pub fn pick_candidates(
        &self,
        point: crate::ViewportPoint,
        image: Option<FrameVersion>,
    ) -> Vec<crate::PickCandidate> {
        let Some(version) = image.filter(|version| self.has_presented_frame(Some(*version))) else {
            return Vec::new();
        };
        let frame = self.displayed_modeling_frame(version.binding).unwrap();
        let session = self.active_session().unwrap();
        frame.geometry.candidates_with_locks(
            point,
            frame.transform,
            &session.document().context().selections,
            session.modeling.tools.active,
            session.modeling.tools.deformer_visibility,
            &session.document().object_state_index(),
        )
    }
    pub fn preview_if_needed(&mut self) -> Result<Option<EditorPreview>> {
        let Some(binding) = self
            .primary_viewport_binding()
            .filter(|b| b.kind == ViewportKind::Modeling)
        else {
            return Ok(None);
        };
        self.preview_for_if_needed(binding)
    }
    pub fn preview_for_if_needed(
        &mut self,
        binding: ViewportBinding,
    ) -> Result<Option<EditorPreview>> {
        if !self.viewport_binding_valid(binding) || binding.kind != ViewportKind::Modeling {
            return Ok(None);
        }
        let metrics = self.viewport_metrics(binding.viewport).unwrap();
        if !metrics.visible() {
            return Ok(None);
        }
        let session = self.active_session().unwrap();
        let versions = (
            session.scene_revision,
            session.view_state(binding.viewport).revision,
        );
        if self
            .viewport_presentation(binding.viewport)
            .unwrap()
            .requested_modeling
            == Some(versions)
        {
            return Ok(None);
        }
        self.preview_for(binding).map(Some)
    }
    pub fn preview(&mut self) -> Result<EditorPreview> {
        let binding = self
            .primary_viewport_binding()
            .ok_or_else(|| anyhow!("No primary viewport"))?;
        self.preview_for(binding)
    }
    pub fn preview_for(&mut self, binding: ViewportBinding) -> Result<EditorPreview> {
        ensure!(
            self.viewport_binding_valid(binding) && binding.kind == ViewportKind::Modeling,
            "Modeling viewport binding changed"
        );
        let metrics = self.viewport_metrics(binding.viewport).unwrap();
        let session = self.active_session_mut().unwrap();
        session.fit_default_view(binding.viewport, metrics)?;
        let (mut scene, mut geometry) = session.scene()?;
        let uv_plane = session.modeling.tools.manual_uv_plane(binding.viewport);
        if uv_plane {
            let target = session.modeling.tools.manual_target().unwrap();
            scene = scene.with_manual_reference(target);
            geometry = std::sync::Arc::new(geometry.with_manual_uv(
                &session.document,
                target,
                scene.clone(),
            ));
        } else if !session.modeling.tools.manual_active() && session.modeling.tools.original_points
        {
            geometry = std::sync::Arc::new(geometry.with_original_points(&session.document)?);
        }
        let view_state = session.view_state(binding.viewport);
        let scene_revision = session.scene_revision;
        let presentation = self.viewport_presentation_mut(binding.viewport).unwrap();
        if !uv_plane
            && let Some((isolated_scene, isolated_geometry)) =
                presentation.isolation.project(&scene, &geometry)
        {
            scene = isolated_scene;
            geometry = isolated_geometry;
        }
        let revision = presentation.next();
        presentation.requested_modeling = Some((scene_revision, view_state.revision));
        let canvas =
            crate::ViewportTransform::new(metrics, view_state.view).map(|transform| CanvasFrame {
                version: FrameVersion {
                    binding,
                    scene: scene_revision,
                    viewport: view_state.revision,
                    request: revision,
                },
                transform,
                geometry,
            });
        Ok(EditorPreview {
            revision,
            scene,
            view: view_state.view,
            canvas,
        })
    }
    pub fn accepts_preview(&self, revision: u64) -> bool {
        self.primary_viewport_binding()
            .is_some_and(|binding| self.accepts_viewport_preview(binding, revision))
    }
    pub fn accepts_viewport_preview(&self, binding: ViewportBinding, revision: u64) -> bool {
        if !self.viewport_binding_valid(binding) {
            return false;
        }
        let session = self.active_session().unwrap();
        let presentation = self.viewport_presentation(binding.viewport).unwrap();
        presentation.sequence == revision
            && presentation.requested_modeling
                == Some((
                    session.scene_revision,
                    session.view_state(binding.viewport).revision,
                ))
    }
    pub fn present_frame(&mut self, frame: CanvasFrame) -> bool {
        let binding = frame.version.binding;
        if !self.viewport_binding_valid(binding)
            || binding.kind != ViewportKind::Modeling
            || self.viewport_metrics(binding.viewport) != Some(frame.transform.metrics)
        {
            return false;
        }
        let session = self.active_session().unwrap();
        let presentation = self.viewport_presentation(binding.viewport).unwrap();
        // Continuous candidates may progress through intermediate completed images.
        if frame.version.request < presentation.floor
            || frame.version.request > presentation.sequence
            || frame.version.scene > session.scene_revision
            || (frame.version.scene != session.scene_revision
                && !session
                    .parameter_preview_floor
                    .is_some_and(|floor| frame.version.scene >= floor)
                && !session.modeling.tools.accepts_drag_frame(frame.version))
            || frame.version.viewport > session.view_state(binding.viewport).revision
            || presentation
                .modeling
                .as_ref()
                .is_some_and(|shown| shown.version.request >= frame.version.request)
        {
            return false;
        }
        self.viewport_presentation_mut(binding.viewport)
            .unwrap()
            .modeling = Some(frame);
        true
    }
}

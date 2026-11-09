use crate::animation::ExpressionTrigger;
use crate::{
    AnimationSession, Document, DocumentId, Editor, ViewportBinding, ViewportInteractionKind,
    ViewportInteractionToken, ViewportKind, ViewportPoint,
};
use anyhow::{ensure, Result};
use std::collections::BTreeMap;
use zipzi_animation::ExpressionTransition;
pub use zipzi_core::document::{CursorPreviewAxis, CursorPreviewBinding};

pub type CursorPreviewBindings = BTreeMap<String, CursorPreviewBinding>;

const RETURN_SECONDS: f32 = 0.18;

pub(crate) struct CursorPhysicsPreview {
    pub(crate) eye_blink_seed: u32,
    pub(crate) expressions: Vec<ExpressionTrigger>,
    bindings: CursorPreviewBindings,
    right_button: bool,
    dragging: bool,
    point: ViewportPoint,
    sampled: Option<(ViewportPoint, bool)>,
    released_at: Option<f32>,
}

impl CursorPhysicsPreview {
    pub(super) fn rebase_expression(&mut self) {
        if let Some(mut current) = self.expressions.pop() {
            self.expressions.clear();
            current.time = 0.;
            current.transition = ExpressionTransition::Immediate;
            self.expressions.push(current);
        }
    }

    pub(super) fn reset(&mut self) {
        self.dragging = false;
        self.point = ViewportPoint::default();
        self.sampled = None;
        self.released_at = None;
    }

    pub(super) fn rebase_clock(&mut self, time: f32) {
        self.released_at = self.released_at.map(|released| released - time);
        self.sampled = None;
        self.rebase_expression();
    }
}

impl Document {
    pub fn cursor_preview_bindings(&self) -> CursorPreviewBindings {
        let model = self.model();
        if let Some(bindings) = &model.editor_state().cursor_preview_bindings {
            return bindings.clone();
        }
        model
            .parameter_order()
            .iter()
            .filter_map(|id| {
                let parameter = model.parameter(id)?;
                CursorPreviewBinding::standard(&parameter.runtime_id)
                    .map(|binding| (id.clone(), binding))
            })
            .collect()
    }

    pub fn has_custom_cursor_preview_bindings(&self) -> bool {
        self.model()
            .editor_state()
            .cursor_preview_bindings
            .is_some()
    }
}

impl Editor {
    pub fn cursor_physics_active(&self) -> bool {
        self.animation()
            .is_some_and(|animation| animation.cursor_physics.is_some())
    }

    pub fn set_physics_cursor(&mut self, document: DocumentId, enabled: bool) -> Result<()> {
        self.ensure_animation_target(document)?;
        if enabled == self.cursor_physics_active() {
            return Ok(());
        }
        let was_manual = self.animation().unwrap().physics_preview.manual;
        self.set_physics_manual(document, enabled)?;
        if enabled {
            if was_manual {
                self.reset_physics_inputs(document)?;
            }
            let session = self.active_session_mut().unwrap();
            let animation = &mut session.animation.playback;
            let timeline_time = animation.physics_return.map_or(0., |(time, _)| time);
            let expressions = animation
                .expressions
                .iter()
                .rev()
                .find(|trigger| trigger.time <= timeline_time)
                .map(|trigger| ExpressionTrigger {
                    expression_id: trigger.expression_id.clone(),
                    time: 0.,
                    transition: ExpressionTransition::Immediate,
                })
                .into_iter()
                .collect();
            animation.playing = true;
            animation.cursor_physics = Some(CursorPhysicsPreview {
                eye_blink_seed: uuid::Uuid::new_v4().as_u128() as u32,
                expressions,
                bindings: session.document.cursor_preview_bindings(),
                right_button: false,
                dragging: false,
                point: ViewportPoint::default(),
                sampled: None,
                released_at: None,
            });
        }
        Ok(())
    }

    pub fn replace_cursor_preview_bindings(
        &mut self,
        document: DocumentId,
        expected_revision: u64,
        bindings: Option<CursorPreviewBindings>,
    ) -> Result<()> {
        self.ensure_animation_target(document)?;
        let session = self.active_session_mut().unwrap();
        session
            .document
            .replace_cursor_preview_bindings(expected_revision, bindings)?;
        session.reconcile_animation();
        Ok(())
    }

    pub fn begin_cursor_preview_drag(
        &mut self,
        binding: ViewportBinding,
        point: ViewportPoint,
        right_button: bool,
    ) -> Result<ViewportInteractionToken> {
        self.ensure_animation_target(binding.document)?;
        ensure!(
            binding.kind == ViewportKind::Animation && self.cursor_physics_active(),
            "Cursor preview is inactive"
        );
        let point = self.normalized_cursor_point(binding, point)?;
        let token =
            self.acquire_viewport_interaction(binding, ViewportInteractionKind::CursorPreview)?;
        let cursor = self
            .active_session_mut()
            .unwrap()
            .animation
            .playback
            .cursor_physics
            .as_mut()
            .unwrap();
        if cursor.right_button != right_button {
            cursor.right_button = right_button;
            cursor.sampled = None;
        }
        self.move_cursor_preview(point);
        Ok(token)
    }

    pub fn update_cursor_preview_drag(
        &mut self,
        token: ViewportInteractionToken,
        point: ViewportPoint,
    ) -> Result<()> {
        ensure!(
            token.kind == ViewportInteractionKind::CursorPreview
                && self.viewport_interaction_valid(token),
            "Cursor preview interaction changed"
        );
        let point = self.normalized_cursor_point(token.binding, point)?;
        self.move_cursor_preview(point);
        Ok(())
    }

    fn normalized_cursor_point(
        &self,
        binding: ViewportBinding,
        point: ViewportPoint,
    ) -> Result<ViewportPoint> {
        ensure!(
            point.x.is_finite() && point.y.is_finite(),
            "Invalid cursor position"
        );
        let metrics = self
            .viewport_metrics(binding.viewport)
            .filter(|metrics| metrics.visible())
            .ok_or_else(|| anyhow::anyhow!("Viewport has no visible area"))?;
        Ok(ViewportPoint {
            x: (2. * (point.x / metrics.width) - 1.).clamp(-1., 1.),
            y: (1. - 2. * (point.y / metrics.height)).clamp(-1., 1.),
        })
    }

    fn move_cursor_preview(&mut self, point: ViewportPoint) {
        let animation = &mut self.active_session_mut().unwrap().animation.playback;
        let cursor = animation.cursor_physics.as_mut().unwrap();
        cursor.dragging = true;
        cursor.point = point;
        cursor.released_at = None;
        // A drag can resume a trial paused by an earlier playback error.
        animation.playing = true;
    }

    pub(crate) fn release_cursor_preview(&mut self, document: DocumentId) {
        if self.active_id() == Some(document)
            && let Some(session) = self.active_session_mut()
            && let Some(cursor) = &mut session.animation.playback.cursor_physics
        {
            cursor.dragging = false;
            cursor.released_at = Some(session.animation.playback.time);
        }
    }

    pub(super) fn cancel_cursor_preview_drag(&mut self) {
        if let Some(token) = self
            .viewport_interaction
            .filter(|t| t.kind == ViewportInteractionKind::CursorPreview)
        {
            self.cancel_viewport_interaction(token);
        }
    }
}

impl AnimationSession {
    pub fn has_cursor_preview_bindings(&self) -> bool {
        self.cursor_physics.as_ref().is_some_and(|cursor| {
            cursor
                .bindings
                .values()
                .any(|binding| binding.axis != CursorPreviewAxis::Disabled)
        })
    }

    pub(crate) fn reconcile_cursor_bindings(&mut self, document: &Document) {
        let Some(cursor) = &mut self.cursor_physics else {
            return;
        };
        // A model edit can also change a mapped parameter's range or default.
        cursor.sampled = None;
        let bindings = document.cursor_preview_bindings();
        if cursor.bindings == bindings {
            return;
        }
        for id in cursor.bindings.iter().filter_map(|(id, binding)| {
            (binding.axis != CursorPreviewAxis::Disabled
                && bindings
                    .get(id)
                    .is_none_or(|new| new.axis == CursorPreviewAxis::Disabled))
            .then_some(id)
        }) {
            self.physics_preview.push_cursor_input(id, self.time, None);
        }
        cursor.bindings = bindings;
        self.physics_input_changed();
    }

    pub(crate) fn sample_cursor_preview(&mut self, document: &Document) {
        let Some(cursor) = &mut self.cursor_physics else {
            return;
        };
        let mut point = cursor.point;
        if let Some(released_at) = cursor.released_at {
            let remaining = (1. - (self.time - released_at) / RETURN_SECONDS).clamp(0., 1.);
            let weight = remaining * remaining;
            point.x *= weight;
            point.y *= weight;
            if remaining == 0. {
                cursor.point = ViewportPoint::default();
                cursor.released_at = None;
            }
        }
        let active = cursor.dragging || cursor.released_at.is_some();
        let sample = (point, active);
        if cursor.sampled == Some(sample) {
            return;
        }
        cursor.sampled = Some(sample);
        let model = document.model();
        let mut changed = false;
        for (id, binding) in &cursor.bindings {
            if binding.axis == CursorPreviewAxis::Disabled {
                continue;
            }
            let axis = match binding.axis {
                CursorPreviewAxis::X if active && !cursor.right_button => Some(point.x),
                CursorPreviewAxis::Y if active && !cursor.right_button => Some(point.y),
                CursorPreviewAxis::RightX if active && cursor.right_button => Some(point.x),
                CursorPreviewAxis::RightY if active && cursor.right_button => Some(point.y),
                _ => None,
            };
            if let Some(parameter) = model.parameter(id) {
                let value = axis.map(|axis| {
                    let direction = if binding.reversed { -axis } else { axis };
                    let endpoint = if direction >= 0. {
                        parameter.maximum
                    } else {
                        parameter.minimum
                    };
                    let weight = f64::from(direction.abs()) * binding.influence_percent / 100.;
                    (f64::from(parameter.default_value) * (1. - weight)
                        + f64::from(endpoint) * weight) as f32
                });
                changed |= self.physics_preview.push_cursor_input(id, self.time, value);
            }
        }
        if changed {
            self.physics_input_changed();
        }
    }
}

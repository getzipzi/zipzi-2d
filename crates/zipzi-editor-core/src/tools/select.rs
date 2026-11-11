//! Click and rectangle selection, including deferred surface hits.
use super::{
    OverlaySnapshot,
    gesture::Press,
    region::{SelectionOperation, SelectionRegion, select_click, select_region},
};
use super::{ToolController, hints::*};
use crate::{CanvasFrame, ComponentRef, Document, HitTarget, ObjectKind, ViewportPoint};
use anyhow::Result;

// Preserve the existing generic transform guidance during click/rectangle capture.
pub(super) const CAPTURE_HINTS: &[InteractionHint] = &[
    InteractionHint::Drag(HintAction::Transform),
    InteractionHint::Shift(HintAction::AddSelection),
    InteractionHint::Alt(HintAction::RemoveSelection),
    InteractionHint::Command(crate::CommandId::CancelGesture),
];

pub(super) struct SelectGesture {
    pub press: Press,
    pub target: Option<HitTarget>,
    pub component_surface: bool,
}
impl SelectGesture {
    pub fn finish(
        &self,
        document: &mut Document,
        components: &mut Vec<ComponentRef>,
    ) -> Result<()> {
        let press = &self.press;
        if press.dragged {
            select_region(
                components,
                document,
                press,
                &SelectionRegion::rectangle(press.point, press.current),
            )?;
        } else {
            select_click(
                components,
                document,
                press,
                &self.target,
                self.component_surface,
            )?;
        }
        Ok(())
    }
    pub fn overlay(&self, out: &mut OverlaySnapshot) {
        if self.press.dragged {
            out.marquee = Some((self.press.point, self.press.current));
        }
    }
}

impl ToolController {
    pub(super) fn component_surface(
        scene: &crate::CanvasScene,
        selection: &[crate::ObjectRef],
        hit: Option<&HitTarget>,
        locks: &crate::ObjectStateIndex,
    ) -> bool {
        matches!(hit, Some(HitTarget::Object(object)) if selection.iter().any(|selected|
            !locks.is_locked(selected) && ((selected.kind == ObjectKind::Mesh && selected == object)
            || (selected.kind == ObjectKind::Deformer && (selected == object
                || (object.kind == ObjectKind::Mesh
                    && scene.control(&selected.id).is_some_and(|c| c.kind == crate::ControlKind::Warp)
                    && scene.ancestors(object).contains(selected)))))))
    }

    pub(super) fn begin_selection(
        &mut self,
        document: &Document,
        frame: &CanvasFrame,
        point: ViewportPoint,
        shift: bool,
        alt: bool,
        primary: bool,
        resize_brush: bool,
    ) {
        let locks = document.object_state_index();
        let scene = &frame.geometry;
        let t = frame.transform;
        let selection: Vec<_> = document
            .context()
            .selections
            .iter()
            .filter(|object| scene.in_isolation_scope(object))
            .cloned()
            .collect();
        let brush_drag = self.active == super::ToolId::BrushSelect && primary && !resize_brush;
        let hit = if brush_drag {
            self.brush_drag_hit(document, frame, point)
        } else {
            self.canvas_hit(document, frame, point)
        };
        // Cmd is an exact painted-point grab in every brush mode. Missing a point
        // must never turn a precise drag into a new area deformation.
        if brush_drag && hit.is_none() {
            return;
        }
        // Brush dragging edits points independently of the hidden Warp frame.
        if !brush_drag
            && matches!(&hit,Some(HitTarget::Component(c)) if self.warp_frame_info(document,frame,&c.object).is_some_and(|i|i.adjusting))
        {
            return;
        }
        self.hovered = hit.clone();
        self.hovered_binding = Some(frame.version.binding);
        let mut component_surface =
            Self::component_surface(scene, &selection, hit.as_ref(), &locks);
        // A selected Warp surface permits component marquee selection without
        // replacing the shared object selection.
        let mut hit = match hit {
            Some(HitTarget::Object(_)) if component_surface && !alt => None,
            hit => hit,
        };
        let operation = SelectionOperation::from_modifiers(shift, alt);
        let access = self.edit_access(document, scene, &selection);
        let mut deferred_toggle = None;
        match &hit {
            _ if brush_drag => {}
            Some(HitTarget::Component(c)) => {
                let rotation_center_drag = c.id == 0
                    && scene
                        .control(&c.object.id)
                        .is_some_and(|control| control.kind == crate::ControlKind::Rotation);
                if operation == SelectionOperation::Remove && !rotation_center_drag {
                    self.components.retain(|v| v != c);
                    return;
                }
                if operation == SelectionOperation::Add && self.components.contains(c) {
                    if matches!(access.probe(c), Some(Ok(()))) {
                        // A Shift click toggles on release. Crossing the drag
                        // threshold instead keeps the point and constrains motion.
                        deferred_toggle = Some(c.clone());
                    } else {
                        self.components.retain(|v| v != c);
                        return;
                    }
                }
                if !matches!(access.probe(c), Some(Ok(()))) {
                    // Retain hover feedback, but a disabled point cannot capture an edit.
                    hit = None;
                    component_surface = true;
                } else if !self.components.contains(c) {
                    if !shift {
                        self.components.clear();
                    }
                    self.components.push(c.clone());
                }
            }
            _ => {}
        }
        let press = Press {
            version: frame.version,
            point,
            transform: t,
            scene: scene.clone(),
            shift,
            alt,
            deferred_toggle,
            components: self.components.clone(),
            weights: self.component_weights.clone(),
            warp_selection: self.warp_selection_gizmo(document, frame),
            adjust_frame: matches!(&hit,Some(HitTarget::DeformerHandle(object,_))
                    if self.warp_frame_info(document,frame,object).is_some_and(|i|i.adjusting)),
            operation,
            access,
            original_points: self.original_points,
            keep_children: primary && !brush_drag,
            selection,
            current: point,
            dragged: false,
        };
        self.gesture = if self.active == super::ToolId::BrushSelect
            && !brush_drag
            && !resize_brush
            && self.brush_mode == super::BrushMode::Deform
        {
            self.begin_brush_deform(document, press)
                .map(super::gesture::Gesture::Controls)
        } else if self.active == super::ToolId::BrushSelect
            && !brush_drag
            && !resize_brush
            && self.brush_mode == super::BrushMode::Inflate
        {
            super::inflate::InflateGesture::new(document, press, self.brush_selection)
                .map(super::gesture::Gesture::Inflate)
        } else if self.active == super::ToolId::BrushSelect && !brush_drag {
            super::brush_select::BrushSelectionGesture::new(
                document,
                press,
                self.brush_selection,
                resize_brush,
            )
            .map(|mut brush| {
                brush.update(
                    &mut self.components,
                    &mut self.brush_selection,
                    &mut self.component_weights,
                );
                super::gesture::Gesture::BrushSelect(brush)
            })
        } else {
            self.active.begin(press, hit, component_surface)
        };
        self.prune_warp_frames(document);
    }
}

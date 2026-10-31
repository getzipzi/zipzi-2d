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
    pub(super) fn begin_selection(
        &mut self,
        document: &Document,
        frame: &CanvasFrame,
        point: ViewportPoint,
        shift: bool,
        alt: bool,
        primary: bool,
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
        let hit = self.canvas_hit(document, frame, point);
        if matches!(&hit,Some(HitTarget::Component(c)) if self.warp_frame_info(document,frame,&c.object).is_some_and(|i|i.adjusting))
        {
            return;
        }
        self.hovered = hit.clone();
        self.hovered_binding = Some(frame.version.binding);
        let mut component_surface = matches!(&hit, Some(HitTarget::Object(r)) if selection.iter().any(|selected|
                    !locks.is_locked(selected) && ((selected.kind == ObjectKind::Mesh && selected == r)
                    || (selected.kind == ObjectKind::Deformer
                        && (selected == r || (r.kind == ObjectKind::Mesh && scene.ancestors(r).contains(selected)))))));
        // A selected surface permits component marquee selection without
        // replacing the shared object selection.
        let mut hit = match hit {
            Some(HitTarget::Object(_)) if component_surface && !alt => None,
            hit => hit,
        };
        let operation = SelectionOperation::from_modifiers(shift, alt);
        let access = self.edit_access(document, scene, &selection);
        let mut deferred_toggle = None;
        match &hit {
            Some(HitTarget::Component(c)) => {
                if operation == SelectionOperation::Remove {
                    self.components.retain(|v| v != c);
                    return;
                }
                if operation == SelectionOperation::Add && self.components.contains(c) {
                    if scene
                        .control(&c.object.id)
                        .is_some_and(|control| control.kind == crate::ControlKind::Warp)
                        && matches!(access.probe(c), Some(Ok(())))
                    {
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
        self.gesture = self.active.begin(
            Press {
                version: frame.version,
                point,
                transform: t,
                scene: scene.clone(),
                shift,
                alt,
                deferred_toggle,
                components: self.components.clone(),
                warp_selection: self.warp_selection_gizmo(document, frame),
                adjust_frame: matches!(&hit,Some(HitTarget::DeformerHandle(object,_))
                    if self.warp_frame_info(document,frame,object).is_some_and(|i|i.adjusting)),
                operation,
                access,
                warp_fine: self.warp_fine,
                original_points: self.original_points,
                keep_children: primary || self.warp_keep_children,
                selection,
                current: point,
                dragged: false,
            },
            hit,
            component_surface,
        );
        self.prune_warp_frames(document);
    }
}

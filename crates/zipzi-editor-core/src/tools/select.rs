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
        match &hit {
            Some(HitTarget::Component(c)) => {
                if operation == SelectionOperation::Remove
                    || (operation == SelectionOperation::Add && self.components.contains(c))
                {
                    self.components.retain(|v| v != c);
                    return;
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
                operation,
                access,
                warp_fine: self.warp_fine,
                original_points: self.original_points,
                selection,
                current: point,
                dragged: false,
            },
            hit,
            component_surface,
        );
    }
}

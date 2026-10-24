//! Click and rectangle selection, including deferred surface hits.
use super::{
    OverlaySnapshot,
    gesture::Press,
    region::{SelectionOperation, SelectionRegion, select_region},
};
use super::{ToolController, hints::*};
use crate::{CanvasFrame, ComponentRef, Document, HitTarget, ObjectKind, ViewportPoint};
use anyhow::Result;

// Preserve the existing generic transform guidance during click/rectangle capture.
pub(super) const CAPTURE_HINTS: &[InteractionHint] = &[
    InteractionHint::Drag(HintAction::Transform),
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
        if !press.dragged && self.target.is_none() && !self.component_surface {
            document.select_objects(Vec::new())?;
            components.clear();
        } else if press.dragged {
            select_region(
                components,
                document,
                press,
                &SelectionRegion::rectangle(press.point, press.current),
                SelectionOperation::from_modifiers(press.shift, false),
            )?;
        } else if let Some(HitTarget::Object(object)) = &self.target {
            let mut selection = if press.shift {
                press.selection.clone()
            } else {
                Vec::new()
            };
            if press.shift && selection.contains(object) {
                selection.retain(|r| r != object);
            } else {
                selection.push(object.clone());
            }
            document.select_objects(selection)?;
            components.clear();
        } else if !press.shift {
            components.clear();
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
        let hit = scene.hit_with_mode(
            point,
            t,
            &selection,
            self.active,
            self.deformer_visibility,
            self.warp_fine,
            &locks,
        );
        self.hovered = hit.clone();
        self.hovered_binding = Some(frame.version.binding);
        let component_surface = matches!(&hit, Some(HitTarget::Object(r)) if selection.iter().any(|selected|
                    !locks.is_locked(selected) && ((selected.kind == ObjectKind::Mesh && selected == r)
                    || (selected.kind == ObjectKind::Deformer
                        && (selected == r || (r.kind == ObjectKind::Mesh && scene.ancestors(r).contains(selected)))))));
        // A selected surface permits component marquee selection without
        // replacing the shared object selection.
        let hit = match hit {
            Some(HitTarget::Object(_)) if component_surface => None,
            hit => hit,
        };
        match &hit {
            Some(HitTarget::Component(c)) => {
                if shift && self.components.contains(c) {
                    self.components.retain(|v| v != c);
                    return;
                }
                if !self.components.contains(c) {
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
                warp_fine: self.warp_fine,
                selection,
                current: point,
                dragged: false,
            },
            hit,
            component_surface,
            alt,
        );
    }
}

//! Freehand selection owns its sampling, completion, overlay and hint declaration.
use super::{
    OverlaySnapshot,
    gesture::Press,
    hints::*,
    region::{SelectionRegion, select_click, select_region},
};
use crate::{CommandId, ComponentRef, Document, HitTarget, ViewportPoint};
use anyhow::Result;

pub(super) struct LassoGesture {
    pub press: Press,
    points: Vec<ViewportPoint>,
    target: Option<HitTarget>,
    component_surface: bool,
}
impl LassoGesture {
    pub fn new(press: Press, target: Option<HitTarget>, component_surface: bool) -> Self {
        let points = vec![press.point];
        Self {
            press,
            points,
            target,
            component_surface,
        }
    }
    pub fn update(&mut self) {
        if crate::picking::distance2(*self.points.last().unwrap(), self.press.current) >= 4. {
            self.points.push(self.press.current);
        }
    }
    pub fn finish(
        &mut self,
        document: &mut Document,
        components: &mut Vec<ComponentRef>,
    ) -> Result<()> {
        if self.points.last() != Some(&self.press.current) {
            self.points.push(self.press.current);
        }
        if self.press.dragged {
            if let Some(region) = SelectionRegion::lasso(&self.points) {
                select_region(components, document, &self.press, &region)?;
            }
        } else {
            select_click(
                components,
                document,
                &self.press,
                &self.target,
                self.component_surface,
            )?;
        }
        Ok(())
    }
    pub fn overlay(&self, out: &mut OverlaySnapshot) {
        if self.press.dragged {
            out.lasso.clone_from(&self.points);
            if out.lasso.last() != Some(&self.press.current) {
                out.lasso.push(self.press.current);
            }
        }
    }
}
pub(super) const IDLE_HINTS: &[InteractionHint] = &[
    InteractionHint::Drag(HintAction::LassoSelect),
    InteractionHint::Shift(HintAction::AddSelection),
    InteractionHint::Alt(HintAction::RemoveSelection),
    InteractionHint::Command(CommandId::SelectTool),
];
pub(super) const CAPTURE_HINTS: &[InteractionHint] = &[
    InteractionHint::Drag(HintAction::LassoSelect),
    InteractionHint::Shift(HintAction::AddSelection),
    InteractionHint::Alt(HintAction::RemoveSelection),
    InteractionHint::Command(CommandId::CancelGesture),
];

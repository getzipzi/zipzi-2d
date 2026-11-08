//! Shared press projection and explicit gesture dispatch. Each variant owns its operation.
use super::{
    brush_select::BrushSelectionGesture, controls::ControlGesture, lasso::LassoGesture,
    select::SelectGesture, *,
};
use anyhow::{Result, ensure};

pub(super) struct Press {
    pub version: FrameVersion,
    pub point: ViewportPoint,
    pub transform: ViewportTransform,
    pub scene: Arc<CanvasScene>,
    pub shift: bool,
    pub alt: bool,
    pub deferred_toggle: Option<ComponentRef>,
    pub components: Vec<ComponentRef>,
    pub weights: std::collections::HashMap<ComponentRef, f32>,
    pub warp_selection: Option<crate::deformer::WarpGizmos>,
    pub adjust_frame: bool,
    pub operation: region::SelectionOperation,
    pub access: Arc<access::EditAccess>,
    pub original_points: bool,
    pub keep_children: bool,
    pub selection: Vec<ObjectRef>,
    pub current: ViewportPoint,
    pub dragged: bool,
}

pub(super) enum Gesture {
    Select(SelectGesture),
    Lasso(LassoGesture),
    BrushSelect(BrushSelectionGesture),
    Controls(ControlGesture),
}
impl Gesture {
    pub fn kind(&self) -> CanvasGestureKind {
        use crate::DeformerHandle as H;
        use CanvasGestureKind as K;
        match self {
            Self::Select(_) => K::BoxSelect,
            Self::Lasso(_) => K::LassoSelect,
            Self::BrushSelect(g) if g.resizing => K::BrushResize,
            Self::BrushSelect(_) => K::BrushSelect,
            Self::Controls(g) if g.adjusting_bone_length() => K::BoneLength,
            Self::Controls(g) => match g.target {
                HitTarget::RotationHandle(_) | HitTarget::DeformerHandle(_, H::Rotate) => K::Rotate,
                HitTarget::DeformerHandle(_, H::RotationScale | H::Scale(_)) => K::Scale,
                _ => K::Move,
            },
        }
    }

    pub fn set_modifiers(&mut self, shift: bool, alt: bool) -> bool {
        match self {
            Self::Controls(g) => g.set_modifiers(shift, alt),
            _ => false,
        }
    }
    pub fn press(&self) -> &Press {
        match self {
            Self::Select(g) => &g.press,
            Self::Lasso(g) => &g.press,
            Self::BrushSelect(g) => &g.press,
            Self::Controls(g) => &g.press,
        }
    }
    fn press_mut(&mut self) -> &mut Press {
        match self {
            Self::Select(g) => &mut g.press,
            Self::Lasso(g) => &mut g.press,
            Self::BrushSelect(g) => &mut g.press,
            Self::Controls(g) => &mut g.press,
        }
    }
    pub fn target(&self) -> Option<&HitTarget> {
        match self {
            Self::Select(g) => g.target.as_ref(),
            Self::Lasso(_) | Self::BrushSelect(_) => None,
            Self::Controls(g) => Some(&g.target),
        }
    }
    pub fn accepts_frame(&self, version: FrameVersion) -> bool {
        matches!(self, Self::Controls(g) if g.accepts_frame(version))
    }
    pub fn update(
        &mut self,
        document: &mut Document,
        components: &mut Vec<ComponentRef>,
        point: ViewportPoint,
        brush_settings: &mut BrushSelectionSettings,
        weights: &mut std::collections::HashMap<ComponentRef, f32>,
    ) -> Result<bool> {
        ensure!(
            point.x.is_finite() && point.y.is_finite(),
            "Invalid pointer position"
        );
        let press = self.press_mut();
        press.current = point;
        press.dragged |= crate::picking::distance2(press.point, point) >= 9.;
        match self {
            Self::Select(_) => Ok(false),
            Self::Lasso(g) => {
                g.update();
                Ok(false)
            }
            Self::Controls(g) => g.update(document, components),
            Self::BrushSelect(g) => {
                g.update(components, brush_settings, weights);
                Ok(false)
            }
        }
    }
    pub fn finish(
        &mut self,
        document: &mut Document,
        components: &mut Vec<ComponentRef>,
    ) -> Result<()> {
        match self {
            Self::Select(g) => g.finish(document, components),
            Self::Lasso(g) => g.finish(document, components),
            Self::Controls(g) => g.finish(document, components),
            Self::BrushSelect(_) => Ok(()),
        }
    }
    pub fn cancel(
        &self,
        document: &mut Document,
        components: &mut Vec<ComponentRef>,
        brush_settings: &mut BrushSelectionSettings,
        weights: &mut std::collections::HashMap<ComponentRef, f32>,
    ) -> bool {
        match self {
            Self::Controls(g) => g.cancel(document),
            Self::BrushSelect(g) => {
                g.cancel(components, brush_settings, weights);
                false
            }
            _ => false,
        }
    }
    pub fn overlay(&self, out: &mut OverlaySnapshot) {
        match self {
            Self::Select(g) => g.overlay(out),
            Self::Lasso(g) => g.overlay(out),
            Self::Controls(g) => g.overlay(out),
            Self::BrushSelect(_) => {}
        }
    }
    pub fn hints(&self) -> &'static [InteractionHint] {
        match self {
            Self::Lasso(_) => lasso::CAPTURE_HINTS,
            Self::Select(_) => select::CAPTURE_HINTS,
            Self::Controls(g) => g.hints(),
            Self::BrushSelect(g) if g.resizing => brush_select::RESIZE_HINTS,
            Self::BrushSelect(_) => brush_select::CAPTURE_HINTS,
        }
    }
}

impl ToolId {
    pub(super) fn begin(
        self,
        press: Press,
        hit: Option<HitTarget>,
        component_surface: bool,
    ) -> Option<Gesture> {
        if self == Self::Hand {
            return None;
        }
        match hit {
            Some(
                target @ (HitTarget::Component(_)
                | HitTarget::RotationHandle(_)
                | HitTarget::DeformerHandle(_, _)),
            ) => Some(Gesture::Controls(ControlGesture::new(press, target))),
            target if self == Self::Lasso => Some(Gesture::Lasso(LassoGesture::new(
                press,
                target,
                component_surface,
            ))),
            target => Some(Gesture::Select(SelectGesture {
                press,
                target,
                component_surface,
            })),
        }
    }
    pub fn idle_hints(self) -> &'static [InteractionHint] {
        match self {
            Self::Select => &[],
            Self::Lasso => lasso::IDLE_HINTS,
            Self::BrushSelect => brush_select::IDLE_HINTS,
            Self::Glue => glue::HINTS,
            Self::Hand => hand::IDLE_HINTS,
        }
    }
    pub fn idle_escape_command(self) -> Option<crate::CommandId> {
        self.uses_component_selection()
            .then_some(crate::CommandId::Deselect)
    }
    fn uses_component_selection(self) -> bool {
        matches!(self, Self::Select | Self::Lasso | Self::BrushSelect)
    }
    pub(super) fn preserves_components(self, next: Self) -> bool {
        self.uses_component_selection() && next.uses_component_selection()
    }
}

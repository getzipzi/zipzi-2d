//! Shared press projection and explicit gesture dispatch. Each variant owns its operation.
use super::{controls::ControlGesture, lasso::LassoGesture, select::SelectGesture, *};
use anyhow::{Result, ensure};

pub(super) struct Press {
    pub version: FrameVersion,
    pub point: ViewportPoint,
    pub transform: ViewportTransform,
    pub scene: Arc<CanvasScene>,
    pub shift: bool,
    pub operation: region::SelectionOperation,
    pub access: Arc<access::EditAccess>,
    pub warp_fine: bool,
    pub original_points: bool,
    pub selection: Vec<ObjectRef>,
    pub current: ViewportPoint,
    pub dragged: bool,
}

pub(super) enum Gesture {
    Select(SelectGesture),
    Lasso(LassoGesture),
    Controls(ControlGesture),
}
impl Gesture {
    pub fn press(&self) -> &Press {
        match self {
            Self::Select(g) => &g.press,
            Self::Lasso(g) => &g.press,
            Self::Controls(g) => &g.press,
        }
    }
    fn press_mut(&mut self) -> &mut Press {
        match self {
            Self::Select(g) => &mut g.press,
            Self::Lasso(g) => &mut g.press,
            Self::Controls(g) => &mut g.press,
        }
    }
    pub fn target(&self) -> Option<&HitTarget> {
        match self {
            Self::Select(g) => g.target.as_ref(),
            Self::Lasso(_) => None,
            Self::Controls(g) => Some(&g.target),
        }
    }
    pub fn accepts_frame(&self, version: FrameVersion) -> bool {
        matches!(self, Self::Controls(g) if g.accepts_frame(version))
    }
    pub fn update(
        &mut self,
        document: &mut Document,
        components: &[ComponentRef],
        point: ViewportPoint,
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
            Self::Controls(g) => g.finish(document),
        }
    }
    pub fn cancel(&self, document: &mut Document) -> bool {
        match self {
            Self::Controls(g) => g.cancel(document),
            _ => false,
        }
    }
    pub fn overlay(&self, out: &mut OverlaySnapshot) {
        match self {
            Self::Select(g) => g.overlay(out),
            Self::Lasso(g) => g.overlay(out),
            Self::Controls(g) => g.overlay(out),
        }
    }
    pub fn hints(&self) -> &'static [InteractionHint] {
        match self {
            Self::Lasso(_) => lasso::CAPTURE_HINTS,
            Self::Select(_) => select::CAPTURE_HINTS,
            Self::Controls(_) => controls::HINTS,
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
            Self::Glue => glue::HINTS,
            Self::Hand => hand::IDLE_HINTS,
        }
    }
    pub fn idle_escape_command(self) -> Option<crate::CommandId> {
        self.uses_component_selection()
            .then_some(crate::CommandId::Deselect)
    }
    fn uses_component_selection(self) -> bool {
        matches!(self, Self::Select | Self::Lasso)
    }
    pub(super) fn preserves_components(self, next: Self) -> bool {
        self.uses_component_selection() && next.uses_component_selection()
    }
}

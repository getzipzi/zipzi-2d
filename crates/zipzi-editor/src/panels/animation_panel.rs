//! Animation resources, physics authoring and presented-frame inspectors.
pub(crate) use crate::properties::expression_info;
mod physics_list;
mod pose;
mod resources;
use crate::{EditorView, t};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dock::{BasePanel, Panel, PanelEvent},
    tooltip::Tooltip,
};
use gpui_kit::{prelude::*, *};

use zipzi_editor_core::PanelKind;

pub struct AnimationPanel {
    owner: WeakEntity<EditorView>,
    kind: PanelKind,
    focus: FocusHandle,
    _observe: Subscription,
    resources: resources::ResourceList,
    sorting: Entity<crate::panels::sorting::SortState>,
    business_collapsed: std::collections::HashSet<String>,
    business_focus: Option<zipzi_editor_core::ObjectRef>,
    business_scroll: UniformListScrollHandle,
    location: Option<(zipzi_editor_core::DocumentId, zipzi_editor_core::ObjectRef)>,
}
impl AnimationPanel {
    pub(crate) fn locate_object(
        &mut self,
        document: zipzi_editor_core::DocumentId,
        object: &zipzi_editor_core::ObjectRef,
        cx: &mut Context<Self>,
    ) {
        self.location = Some((document, object.clone()));
        cx.notify();
    }
    pub fn new(
        owner: WeakEntity<EditorView>,
        kind: PanelKind,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let focus = cx.focus_handle();
            let sorting =
                crate::panels::sorting::SortState::new(owner.clone(), focus.clone(), window, cx);
            Self {
                sorting,
                business_collapsed: Default::default(),
                business_focus: None,
                business_scroll: UniformListScrollHandle::new(),
                resources: resources::ResourceList::default(),
                location: None,
                _observe: cx.observe(&owner.upgrade().unwrap(), |_, _, cx| cx.notify()),
                owner,
                kind,
                focus,
            }
        })
    }
}
impl EventEmitter<PanelEvent> for AnimationPanel {}
impl Focusable for AnimationPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}
impl BasePanel for AnimationPanel {
    fn panel_name(&self) -> &'static str {
        self.kind.name()
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
}
impl Panel for AnimationPanel {
    fn inner_padding(&self, _: &App) -> bool {
        !matches!(
            self.kind,
            PanelKind::AnimationMotions
                | PanelKind::AnimationExpressions
                | PanelKind::AnimationPhysics
                | PanelKind::AnimationPose
        )
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        t!(self.kind.label()).into_owned()
    }
}
impl crate::docking::header::PanelHeader for AnimationPanel {
    fn render_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.render_header_toolbar(window, cx)
    }
}

impl Render for AnimationPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = self.render_content(window, cx);
        div()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(content)
    }
}

impl AnimationPanel {
    fn render_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if self.kind == PanelKind::AnimationPose {
            return self.render_pose_list(window, cx);
        }
        if self.kind == PanelKind::AnimationPhysics {
            return self.render_physics_list(window, cx);
        }
        if matches!(
            self.kind,
            PanelKind::AnimationMotions | PanelKind::AnimationExpressions
        ) {
            return self.render_resources(window, cx);
        }
        unreachable!("invalid animation panel kind")
    }
}

fn motion_info(resource: &zipzi_editor_core::MotionResource) -> Vec<String> {
    vec![
        format!(
            "{}: {:.3} s · {:.1} fps · {}",
            t!("workspace.duration"),
            resource.duration,
            resource.fps,
            if resource.looping {
                t!("workspace.looping")
            } else {
                t!("workspace.once")
            }
        ),
        t!(
            "workspace.fades",
            fade_in = format!("{:.3}", resource.fade_in),
            fade_out = format!("{:.3}", resource.fade_out)
        )
        .into_owned(),
        t!(
            "workspace.motion_counts",
            tracks = resource.tracks,
            events = resource.events
        )
        .into_owned(),
    ]
}

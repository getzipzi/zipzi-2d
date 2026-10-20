//! Playback projections. These controls never edit the authoring document.
pub(crate) mod inspectors;
pub(crate) use crate::properties::expression_info;
mod resources;
use crate::{EditorView, t};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dock::{BasePanel, Panel, PanelEvent},
    tooltip::Tooltip,
};
use gpui_kit::{prelude::*, *};

use editor_core::PanelKind;

pub struct AnimationPanel {
    owner: WeakEntity<EditorView>,
    kind: PanelKind,
    focus: FocusHandle,
    _observe: Subscription,
    resources: resources::ResourceList,
}
impl AnimationPanel {
    pub fn new(owner: WeakEntity<EditorView>, kind: PanelKind, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            resources: resources::ResourceList::default(),
            _observe: cx.observe(&owner.upgrade().unwrap(), |_, _, cx| cx.notify()),
            owner,
            kind,
            focus: cx.focus_handle(),
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
            PanelKind::AnimationMotions | PanelKind::AnimationExpressions
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
        if matches!(
            self.kind,
            PanelKind::AnimationMotions | PanelKind::AnimationExpressions
        ) {
            return self.render_resources(window, cx);
        }
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let state = owner.read(cx);
        let Some(document) = state.editor.document() else {
            return div().p_3().child(t!("editor.welcome")).into_any_element();
        };
        let snapshot = state.displayed_animation();
        let mut content = div()
            .id(self.kind.name())
            .key_context("KasaneDocument")
            .size_full()
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .text_xs();
        match self.kind {
            PanelKind::AnimationExpressions | PanelKind::AnimationMotions => unreachable!(),
            PanelKind::AnimationPhysics => {
                content = inspectors::physics(content.gap_1(), document, snapshot, cx);
            }
            PanelKind::AnimationPose => {
                content = inspectors::pose(content.gap_1(), document, snapshot, cx);
            }
            _ => unreachable!("invalid animation panel kind"),
        }
        content
            .test_support()
            .track_focus(&self.focus)
            .into_any_element()
    }
}

fn motion_info(resource: &editor_core::MotionResource) -> Vec<String> {
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

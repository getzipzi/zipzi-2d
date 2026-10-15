//! Playback projections. These controls never edit the authoring document.
mod inspectors;
mod parameters;
mod properties;
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
    parameters: parameters::PreviewParameters,
    properties: properties::Properties,
}
impl AnimationPanel {
    pub fn new(owner: WeakEntity<EditorView>, kind: PanelKind, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self {
            resources: resources::ResourceList::default(),
            parameters: parameters::PreviewParameters::default(),
            properties: properties::Properties::default(),
            _observe: cx.observe(&owner.upgrade().unwrap(), |_, _, cx| cx.notify()),
            owner,
            kind,
            focus: cx.focus_handle(),
        })
    }
}
impl AnimationPanel {
    fn render_info_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        use gpui_kit::assets::IconName;
        let dialog = window.has_active_dialog(cx);
        let Some(owner) = self.owner.upgrade() else {
            return Empty.into_any_element();
        };
        let state = owner.read(cx);
        let disabled = state.modal || dialog;
        let command = editor_core::CommandId::ResetAnimation;
        let reset_disabled = disabled
            || !state
                .editor
                .command_available(state.editor.command(command));
        let parameters_owner = self.owner.clone();
        let reset_owner = self.owner.clone();
        super::toolbar::PanelToolbar::new("animation-info-toolbar")
            .action(move |_| {
                Button::new("animation-open-parameters")
                    .icon(IconName::SlidersHorizontal)
                    .accessibility_label(t!("workspace.parameters").into_owned())
                    .tooltip(t!("workspace.parameters_hint"))
                    .disabled(disabled)
                    .on_click(move |_, window, cx| {
                        _ = parameters_owner.update(cx, |owner, cx| {
                            if !owner.modal && !window.has_active_dialog(cx) {
                                if let Some(dock) = &owner.dock {
                                    dock.show(
                                        editor_core::PanelKind::AnimationParameters,
                                        None,
                                        window,
                                        cx,
                                    );
                                }
                                cx.notify();
                            }
                        });
                    })
            })
            .action(move |_| {
                Button::new("animation-reset")
                    .icon(IconName::RotateCcw)
                    .accessibility_label(t!("workspace.reset_preview").into_owned())
                    .tooltip(t!("workspace.reset_hint"))
                    .disabled(reset_disabled)
                    .on_click(move |_, window, cx| {
                        _ = reset_owner
                            .update(cx, |owner, cx| owner.run_command(command, window, cx));
                    })
            })
            .into_any_element()
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
                | PanelKind::AnimationParameters
        )
    }

    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        t!(self.kind.label()).into_owned()
    }
}
impl crate::docking::header::PanelHeader for AnimationPanel {
    fn render_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if matches!(self.kind, PanelKind::AnimationInfo) {
            self.render_info_header(window, cx)
        } else {
            self.render_header_toolbar(window, cx)
        }
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
        if matches!(self.kind, PanelKind::AnimationParameters) {
            return self.render_parameters(cx);
        }
        if matches!(self.kind, PanelKind::AnimationProperties) {
            return self.render_properties(cx);
        }
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let state = owner.read(cx);
        let Some(document) = state.editor.document() else {
            return div().p_3().child(t!("editor.welcome")).into_any_element();
        };
        let animation = state.editor.animation().unwrap();
        let catalog = document.animation_catalog();
        let resources = &catalog.motions;
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
            PanelKind::AnimationProperties => unreachable!(),
            PanelKind::AnimationPhysics => {
                content = inspectors::physics(content.gap_1(), document, snapshot, cx);
            }
            PanelKind::AnimationPose => {
                content = inspectors::pose(content.gap_1(), document, snapshot, cx);
            }
            PanelKind::AnimationInfo => {
                let status = if animation.error.is_some() {
                    t!("workspace.failed")
                } else if animation.playing {
                    t!("workspace.playing")
                } else if animation.presented.is_none() {
                    t!("workspace.evaluating")
                } else {
                    t!("workspace.paused")
                };
                content = content.child(div().id("animation-status").test_support().child(status));
                if let Some(error) = &animation.error {
                    content = content.child(
                        div()
                            .text_color(cx.theme().danger)
                            .child(super::animation_feedback::failure_text(error)),
                    );
                }
                if let Some(snapshot) = snapshot {
                    content = content
                        .child(
                            div()
                                .id("animation-presented-time")
                                .test_support()
                                .child(t!(
                                    "workspace.presented_time",
                                    time = format!("{:.3}", snapshot.time)
                                )),
                        )
                        .child(t!(
                            "workspace.model_opacity",
                            value = format!("{:.3}", snapshot.model_opacity)
                        ));
                }
                if resources.is_empty() && catalog.expressions.is_empty() {
                    content = content.child(t!("workspace.no_resources"));
                }
                if let Some(resource) = resources
                    .iter()
                    .find(|r| animation.motion.as_ref() == Some(&r.selection))
                {
                    content = content.child(resource.name.clone());
                    if !resource.playable() {
                        content = content.child(t!("workspace.invalid_duration"));
                    }
                } else {
                    content = content.child(t!("workspace.select_motion"));
                }
                let name = |id: &str| {
                    catalog
                        .expressions
                        .iter()
                        .find(|r| r.id == id)
                        .map_or_else(|| id.to_string(), |r| r.name.clone())
                };
                content = content.child(format!(
                    "{}: {}",
                    t!("workspace.selected_expression"),
                    animation
                        .expression
                        .as_ref()
                        .map_or_else(|| t!("workspace.none").into_owned(), |id| name(id))
                ));
                if let Some(snapshot) = snapshot {
                    content = content.child(format!(
                        "{}: {}",
                        t!("workspace.active_expressions"),
                        if snapshot.active_expressions.is_empty() {
                            t!("workspace.none").into_owned()
                        } else {
                            snapshot
                                .active_expressions
                                .iter()
                                .map(|id| name(id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        }
                    ));
                    if !snapshot.fired_events.is_empty() {
                        content = content.child(t!("workspace.frame_events"));
                        for event in &snapshot.fired_events {
                            let motion = resources
                                .iter()
                                .find(|r| r.selection.clip_id == event.motion_id)
                                .map_or_else(|| event.motion_id.clone(), |r| r.name.clone());
                            content = content.child(format!("{}: {}", motion, event.value));
                        }
                    }
                }
                for (label, loaded) in [("Physics", catalog.physics), ("Pose", catalog.pose)] {
                    content = content.child(format!(
                        "{label}: {}",
                        if loaded {
                            t!("workspace.loaded")
                        } else {
                            t!("workspace.absent")
                        }
                    ));
                }
                let diagnostics = catalog.diagnostics_for(snapshot);
                if !diagnostics.is_empty() {
                    content = content.child(t!("workspace.diagnostics"));
                    for (index, diagnostic) in diagnostics.iter().enumerate() {
                        content = content.child(
                            div()
                                .id(("animation-diagnostic", index))
                                .test_support()
                                .child(super::animation_feedback::diagnostic_text(diagnostic)),
                        );
                    }
                }
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
fn expression_info(resource: &editor_core::ExpressionResource) -> String {
    let blends = resource
        .blends
        .iter()
        .map(|blend| {
            match blend {
                kasane_core::document::ExpressionBlend::Add => t!("workspace.blend_add"),
                kasane_core::document::ExpressionBlend::Multiply => t!("workspace.blend_multiply"),
                kasane_core::document::ExpressionBlend::Overwrite => {
                    t!("workspace.blend_overwrite")
                }
            }
            .into_owned()
        })
        .collect::<Vec<_>>()
        .join(" / ");
    format!(
        "{} · {} · {}",
        t!("workspace.expression_count", count = resource.parameters),
        blends,
        t!(
            "workspace.fades",
            fade_in = format!("{:.3}", resource.fade_in),
            fade_out = format!("{:.3}", resource.fade_out)
        )
    )
}

//! Playback projections. These controls never edit the authoring document.
use crate::{EditorView, t};
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    dock::{BasePanel, Panel, PanelEvent},
};
use gpui_kit::{prelude::*, *};

#[derive(Clone, Copy)]
pub enum AnimationPanelKind {
    Motions,
    Expressions,
    Info,
}
impl AnimationPanelKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Motions => "AnimationMotions",
            Self::Expressions => "AnimationExpressions",
            Self::Info => "AnimationInfo",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Motions => "editor.motions",
            Self::Expressions => "editor.expressions",
            Self::Info => "workspace.info",
        }
    }
}
pub struct AnimationPanel {
    owner: WeakEntity<EditorView>,
    kind: AnimationPanelKind,
    focus: FocusHandle,
    _observe: Subscription,
}
impl AnimationPanel {
    pub fn new(
        owner: WeakEntity<EditorView>,
        kind: AnimationPanelKind,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| Self {
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
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        t!(self.kind.label()).into_owned()
    }
}
impl Render for AnimationPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let dialog = window.has_active_dialog(cx);
        let state = owner.read(cx);
        let Some(document) = state.editor.document() else {
            return div().p_3().child(t!("editor.welcome")).into_any_element();
        };
        let id = state.editor.active_id().unwrap();
        let animation = state.editor.animation().unwrap();
        let resources = document.motion_resources();
        let mut content = div()
            .id(self.kind.name())
            .test_support()
            .track_focus(&self.focus)
            .key_context("KasaneDocument")
            .size_full()
            .overflow_y_scroll()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .text_xs();
        match self.kind {
            AnimationPanelKind::Expressions => {
                let expressions = document.expression_resources();
                if expressions.is_empty() {
                    content = content.child(t!("workspace.no_expressions"));
                }
                for (index, resource) in expressions.iter().enumerate() {
                    let selected = animation.expression.as_ref() == Some(&resource.id);
                    let expression = resource.id.clone();
                    let owner = self.owner.clone();
                    content = content.child(
                        div()
                            .id(("animation-expression", index))
                            .test_support()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .when(selected, |e| e.bg(cx.theme().accent))
                            .cursor_pointer()
                            .child(resource.name.clone())
                            .on_click(move |_, window, cx| {
                                if window.has_active_dialog(cx) {
                                    return;
                                }
                                let _ = owner.update(cx, |owner, cx| {
                                    if owner.modal {
                                        return;
                                    }
                                    let result =
                                        owner.editor.select_expression(id, expression.clone());
                                    owner.animation_anchor = None;
                                    owner.complete_edit(result, cx);
                                });
                            }),
                    );
                    if !resource.unresolved_targets.is_empty() {
                        content =
                            content.child(div().text_color(cx.theme().danger).child(format!(
                                "{}: {}",
                                t!("workspace.unresolved_targets"),
                                resource.unresolved_targets.join(", ")
                            )));
                    }
                }
                let command = editor_core::CommandId::ClearExpression;
                let owner = self.owner.clone();
                content = content.child(
                    Button::new("animation-clear-expression")
                        .small()
                        .ghost()
                        .label(t!("workspace.clear_expression"))
                        .disabled(
                            state.modal
                                || dialog
                                || !state
                                    .editor
                                    .command_available(state.editor.command(command)),
                        )
                        .on_click(move |_, window, cx| {
                            let _ = owner
                                .update(cx, |owner, cx| owner.run_command(command, window, cx));
                        }),
                );
            }
            AnimationPanelKind::Motions => {
                if resources.is_empty() {
                    content = content.child(t!("workspace.no_motions"));
                }
                let mut previous_group = None;
                for (index, resource) in resources.iter().enumerate() {
                    let group = resource
                        .selection
                        .entry
                        .as_ref()
                        .map(|(name, _)| name.clone())
                        .unwrap_or_else(|| t!("workspace.ungrouped").into_owned());
                    if previous_group.as_ref() != Some(&group) {
                        content = content.child(
                            div()
                                .mt_2()
                                .text_color(cx.theme().muted_foreground)
                                .child(group.clone()),
                        );
                        previous_group = Some(group);
                    }
                    let selected = animation.motion.as_ref() == Some(&resource.selection);
                    let selection = resource.selection.clone();
                    let owner = self.owner.clone();
                    content = content.child(
                        div()
                            .id(("animation-motion", index))
                            .test_support()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .when(selected, |e| e.bg(cx.theme().accent))
                            .cursor_pointer()
                            .child(resource.selection.entry.as_ref().map_or_else(
                                || resource.name.clone(),
                                |(_, entry)| format!("{} · {}", entry + 1, resource.name),
                            ))
                            .on_click(move |_, window, cx| {
                                if window.has_active_dialog(cx) {
                                    return;
                                }
                                let _ = owner.update(cx, |owner, cx| {
                                    if owner.modal {
                                        return;
                                    }
                                    let result = owner.editor.select_motion(id, selection.clone());
                                    owner.animation_anchor = None;
                                    owner.complete_edit(result, cx);
                                });
                            }),
                    );
                }
            }
            AnimationPanelKind::Info => {
                if let Some(resource) = resources
                    .iter()
                    .find(|r| animation.motion.as_ref() == Some(&r.selection))
                {
                    content = content
                        .child(resource.name.clone())
                        .child(format!(
                            "{}: {:.3} s",
                            t!("workspace.duration"),
                            resource.duration
                        ))
                        .child(if resource.looping {
                            t!("workspace.looping")
                        } else {
                            t!("workspace.once")
                        });
                    if !resource.playable() {
                        content = content.child(t!("workspace.invalid_duration"));
                    }
                    if resource.sound.is_some() {
                        content = content.child(t!("workspace.sound_unsupported"));
                    }
                } else {
                    content = content.child(t!("workspace.select_motion"));
                }
                let expressions = document.expression_resources();
                let name = |id: &str| {
                    expressions
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
                if let Some(snapshot) = &animation.presented {
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
                    for diagnostic in &snapshot.coverage {
                        content = content.child(diagnostic.clone());
                    }
                    for event in &snapshot.fired_events {
                        content = content.child(format!("{event:?}"));
                    }
                }
                let command = editor_core::CommandId::ResetAnimation;
                let owner = self.owner.clone();
                content = content.child(
                    Button::new("animation-reset")
                        .small()
                        .ghost()
                        .label(t!("workspace.reset_preview"))
                        .disabled(
                            state.modal
                                || dialog
                                || !state
                                    .editor
                                    .command_available(state.editor.command(command)),
                        )
                        .on_click(move |_, window, cx| {
                            let _ = owner
                                .update(cx, |owner, cx| owner.run_command(command, window, cx));
                        }),
                );
                if let Some(error) = &animation.error {
                    content =
                        content.child(div().text_color(cx.theme().danger).child(error.clone()));
                }
            }
        }
        content.into_any_element()
    }
}

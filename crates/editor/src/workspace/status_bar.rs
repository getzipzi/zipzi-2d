use crate::{EditorView, settings::Settings, t, ui::compact};
use editor_core::{CommandId, EditingContext, ToolId};
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable, kbd::Kbd};
use gpui_kit::{prelude::*, *};

const STATUS_LINE_HEIGHT: f32 = compact::CAPTION * 1.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TimelineCaptureKind {
    Pan,
    Seek,
    Marquee,
    Keys,
    Handle,
    Event,
    Number,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HintContext {
    Quiet,
    ViewportCapture { panning: bool },
    TimelineCapture(TimelineCaptureKind),
    ModelingTool(ToolId),
    TimelinePanTool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StatusBarState {
    hint_context: HintContext,
    error: Option<String>,
    message: String,
    locale: String,
    bindings_digest: Vec<(CommandId, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShortcutHint {
    pub key: String,
    pub label: String,
}

pub struct StatusBar {
    owner: WeakEntity<EditorView>,
    state: Option<StatusBarState>,
    #[cfg(any(test, feature = "test-support"))]
    renders: u64,
    _observe: Subscription,
}

impl StatusBar {
    pub fn new(owner: WeakEntity<EditorView>, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let upgraded = owner.upgrade().unwrap();
            Self {
                _observe: cx.observe(&upgraded, |this: &mut Self, owner, cx| {
                    let next = snapshot_state(owner.read(cx), cx);
                    this.apply_snapshot(next, cx);
                }),
                owner,
                state: None,
                #[cfg(any(test, feature = "test-support"))]
                renders: 0,
            }
        })
    }

    pub(crate) fn sync(entity: &Entity<Self>, view: &EditorView, cx: &mut App) {
        let next = snapshot_state(view, cx);
        if entity.read(cx).state.as_ref() != Some(&next) {
            entity.update(cx, |this, cx| {
                this.state = Some(next);
                cx.notify();
            });
        }
    }

    pub(crate) fn sync_owner(owner: &Entity<EditorView>, cx: &mut App) {
        let (entity, next) = {
            let view = owner.read(cx);
            (view.status_bar.clone(), snapshot_state(view, cx))
        };
        if entity.read(cx).state.as_ref() != Some(&next) {
            entity.update(cx, |this, cx| {
                this.state = Some(next);
                cx.notify();
            });
        }
    }

    fn apply_snapshot(&mut self, next: StatusBarState, cx: &mut Context<Self>) {
        if self.state.as_ref() != Some(&next) {
            self.state = Some(next);
            cx.notify();
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    #[allow(dead_code)]
    pub(crate) fn renders(&self) -> u64 {
        self.renders
    }

    #[cfg(test)]
    pub(crate) fn hint_context(&self) -> Option<HintContext> {
        self.state.as_ref().map(|state| state.hint_context)
    }

    #[cfg(test)]
    pub(crate) fn hints(&self) -> Vec<(String, String)> {
        self.state
            .as_ref()
            .map(build_hints)
            .unwrap_or_default()
            .into_iter()
            .map(|h| (h.key, h.label))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn displayed_message(&self) -> Option<(&str, bool)> {
        let state = self.state.as_ref()?;
        (!state.message.is_empty()).then_some((state.message.as_str(), state.error.is_some()))
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        #[cfg(any(test, feature = "test-support"))]
        {
            self.renders += 1;
        }
        let state = self
            .state
            .get_or_insert_with(|| snapshot_state(owner.read(cx), cx))
            .clone();
        render_status_bar(&state, cx).into_any_element()
    }
}

fn snapshot_state(view: &EditorView, cx: &App) -> StatusBarState {
    let error = view.edit_error.clone().or_else(|| view.render_error());
    let message = error.clone().unwrap_or_else(|| view.message.clone());
    let hint_context = resolve_hint_context(view, cx);
    let bindings_digest = cx
        .global::<Settings>()
        .bindings
        .iter()
        .map(|b| (b.command, b.key.clone()))
        .collect();
    StatusBarState {
        hint_context,
        error,
        message,
        locale: rust_i18n::locale().to_string(),
        bindings_digest,
    }
}

pub(crate) fn resolve_hint_context(view: &EditorView, _cx: &App) -> HintContext {
    // 1. Active pointer/edit capture takes highest priority.
    if let Some((token, panning)) = view.input_router.viewport_capture.get() {
        if view.editor.viewport_interaction_valid(token) {
            return HintContext::ViewportCapture { panning };
        }
    }
    if let Some((operation, kind)) = view.input_router.timeline_capture.get() {
        if operation.valid(&view.editor) {
            return HintContext::TimelineCapture(kind);
        }
    }

    // 2. Non-default modal tools or modes; stay quiet during ordinary idle work.
    if view.editor.active_id().is_some() {
        match view.editor.editing_context() {
            Some(EditingContext::Modeling) => match view.editor.active_tool() {
                ToolId::Glue => return HintContext::ModelingTool(ToolId::Glue),
                ToolId::Hand => return HintContext::ModelingTool(ToolId::Hand),
                ToolId::Select => {}
            },
            Some(EditingContext::Animation) => {
                if view.input_router.timeline_pan_tool.get() {
                    return HintContext::TimelinePanTool;
                }
            }
            None => {}
        }
    }

    HintContext::Quiet
}

#[allow(dead_code)]
fn binding_key(bindings: &[(CommandId, String)], id: CommandId) -> Option<String> {
    bindings
        .iter()
        .find(|(cmd, _)| *cmd == id)
        .map(|(_, key)| Kbd::format(&Keystroke::parse(key).expect("validated keybinding")))
}

#[allow(dead_code)]
fn push_cmd_hint(hints: &mut Vec<ShortcutHint>, bindings: &[(CommandId, String)], id: CommandId) {
    if let Some(key) = binding_key(bindings, id) {
        hints.push(ShortcutHint {
            key,
            label: t!(id.spec().label).into_owned(),
        });
    }
}

#[allow(dead_code)]
fn build_hints(state: &StatusBarState) -> Vec<ShortcutHint> {
    let bindings = &state.bindings_digest;
    let mut hints = Vec::new();

    match state.hint_context {
        HintContext::Quiet => {}
        HintContext::ViewportCapture { panning } => {
            hints.push(ShortcutHint {
                key: t!("status.hint_drag").into_owned(),
                label: if panning {
                    t!("status.hint_pan").into_owned()
                } else {
                    t!("status.hint_transform").into_owned()
                },
            });
            push_cmd_hint(&mut hints, bindings, CommandId::CancelGesture);
        }
        HintContext::TimelineCapture(kind) => {
            let drag_label = match kind {
                TimelineCaptureKind::Pan => t!("status.hint_pan"),
                TimelineCaptureKind::Seek => t!("status.hint_scrub_time"),
                TimelineCaptureKind::Marquee => t!("status.hint_box_select"),
                TimelineCaptureKind::Keys => t!("status.hint_move_keys"),
                TimelineCaptureKind::Handle => t!("status.hint_adjust_curve"),
                TimelineCaptureKind::Event => t!("status.hint_move_event"),
                TimelineCaptureKind::Number => t!("timeline.value"),
            };
            hints.push(ShortcutHint {
                key: t!("status.hint_drag").into_owned(),
                label: drag_label.into_owned(),
            });
            if kind != TimelineCaptureKind::Pan {
                hints.push(ShortcutHint {
                    key: Kbd::format(&Keystroke::parse("escape").expect("valid keystroke")),
                    label: t!(CommandId::CancelGesture.spec().label).into_owned(),
                });
            }
        }
        HintContext::ModelingTool(ToolId::Glue) => {
            hints.push(ShortcutHint {
                key: t!("status.hint_lmb").into_owned(),
                label: t!("status.hint_glue_bind").into_owned(),
            });
            push_cmd_hint(&mut hints, bindings, CommandId::SelectTool);
        }
        HintContext::ModelingTool(ToolId::Hand) => {
            hints.push(ShortcutHint {
                key: t!("status.hint_lmb").into_owned(),
                label: t!("status.hint_pan").into_owned(),
            });
            push_cmd_hint(&mut hints, bindings, CommandId::SelectTool);
        }
        HintContext::ModelingTool(ToolId::Select) => {}
        HintContext::TimelinePanTool => {
            hints.push(ShortcutHint {
                key: t!("status.hint_lmb").into_owned(),
                label: t!("status.hint_pan").into_owned(),
            });
        }
    }

    hints
}

#[allow(dead_code)]
fn render_hint_item(index: usize, hint: ShortcutHint, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .flex()
        .items_center()
        .gap_1()
        .flex_shrink_0()
        .when(index > 0, |e| {
            e.child(
                div()
                    .mr_1()
                    .text_color(theme.muted_foreground.opacity(0.45))
                    .child("·"),
            )
        })
        .child(
            div()
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.foreground.opacity(0.72))
                .child(hint.key),
        )
        .child(div().text_color(theme.muted_foreground).child(hint.label))
}

fn render_status_bar(state: &StatusBarState, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let is_error = state.error.is_some();

    div()
        .id("app-status-bar")
        .test_support()
        .pt_0()
        .pb(compact::rem(
            (compact::STATUS_BAR - STATUS_LINE_HEIGHT) / 2.,
        ))
        .w_full()
        .flex_shrink_0()
        .bg(compact::workspace(cx))
        .flex()
        .items_center()
        .justify_center()
        .px(compact::chrome_inset(cx))
        .text_size(compact::rem(compact::CAPTION))
        .line_height(compact::rem(STATUS_LINE_HEIGHT))
        // Center section: Transient status / error message (shortcut hints hidden while keeping mechanism)
        .child(
            div()
                .id("app-status-message")
                .test_support()
                .h(compact::rem(STATUS_LINE_HEIGHT))
                .flex()
                .max_w(px(520.))
                .min_w_0()
                .items_center()
                .justify_center()
                .gap_1p5()
                .text_color(if is_error {
                    theme.danger
                } else {
                    theme.muted_foreground
                })
                .when(is_error, |e| {
                    e.child(Icon::new(IconName::CircleAlert).small())
                })
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(state.message.clone()),
                ),
        )
}

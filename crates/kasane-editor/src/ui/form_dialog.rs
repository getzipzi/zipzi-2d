//! Editor-local dialog interaction. Views own typed drafts, validation and commands;
//! this module owns the synchronous submit lifecycle and GPUI dialog wiring.
use crate::t;
use gpui_kit::base::Button as BaseButton;
use gpui_kit::component::{
    ActiveTheme, Disableable, WindowExt,
    button::{Button, ButtonVariants},
    dialog::{Cancel, Confirm, DialogFooter},
    input::InputEvent,
};
use gpui_kit::{prelude::*, *};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Editing,
    Submitting,
    Closed,
}

pub(crate) struct FormDialogState {
    pub focus: FocusHandle,
    pub cancel_focus: FocusHandle,
    pub error: Option<String>,
    phase: Phase,
    valid: bool,
    changed: bool,
}

impl FormDialogState {
    pub fn new(cx: &mut App) -> Self {
        Self {
            focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            error: None,
            phase: Phase::Editing,
            valid: false,
            changed: false,
        }
    }

    pub fn set_validation(&mut self, valid: bool, changed: bool) {
        self.valid = valid;
        self.changed = changed;
    }

    pub fn edited(&mut self) {
        self.error = None;
    }

    pub fn closed(&self) -> bool {
        self.phase == Phase::Closed
    }

    /// Also call this when replacing a dialog through WindowExt::close_dialog,
    /// which does not invoke the dialog's on_close callback.
    pub fn close(&mut self) {
        self.phase = Phase::Closed;
    }

    fn can_submit(&self) -> bool {
        self.phase == Phase::Editing && self.valid && self.changed
    }
}

/// Implemented by both editable forms and confirmations (which have no dirty draft).
pub(crate) trait FormDialogView: Render {
    /// Resolved logical pixels, including the current UI scale.
    fn width(_window: &Window, cx: &App) -> Pixels {
        crate::ui::compact::scaled(520., cx)
    }
    fn dialog(&self) -> &FormDialogState;
    fn dialog_mut(&mut self) -> &mut FormDialogState;
    /// Must revalidate against the current source immediately before executing.
    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> anyhow::Result<()>;
    fn error_message(error: &anyhow::Error) -> String;
    fn closing(&mut self) {}
    fn focus_invalid(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}
}

pub(crate) struct FormDialogOptions {
    pub title: String,
    pub submit_label: SharedString,
    pub footer_id: &'static str,
    pub cancel_id: &'static str,
    pub submit_id: &'static str,
    pub initial_focus: FocusHandle,
}

fn submit<V: FormDialogView>(view: &mut V, window: &mut Window, cx: &mut Context<V>) -> bool {
    // Invalid Enter attempts still run validation so the view can reveal/focus errors.
    // Unchanged edits and already submitted/closed dialogs never execute a command.
    if view.dialog().phase != Phase::Editing || !view.dialog().changed {
        return false;
    }
    view.dialog_mut().phase = Phase::Submitting;
    match view.submit(window, cx) {
        Ok(()) => {
            view.dialog_mut().close();
            cx.notify();
            true
        }
        Err(error) => {
            let state = view.dialog_mut();
            state.phase = Phase::Editing;
            state.error = Some(V::error_message(&error));
            view.focus_invalid(window, cx);
            cx.notify();
            false
        }
    }
}

/// Subscribe only single-line inputs to this bridge. Use the input's semantic
/// event rather than intercepting raw keys; selects and textareas keep their Enter.
pub(crate) fn input_enter<V: FormDialogView>(
    event: &InputEvent,
    window: &mut Window,
    cx: &mut Context<V>,
) {
    if matches!(
        event,
        InputEvent::PressEnter {
            secondary: false,
            shift: false
        }
    ) {
        let view = cx.weak_entity();
        window.defer(cx, move |window, cx| {
            let Some(view) = view.upgrade() else { return };
            let state = view.read(cx).dialog();
            if state.phase == Phase::Editing && state.focus.contains_focused(window, cx) {
                state
                    .focus
                    .clone()
                    .dispatch_action(&Confirm { secondary: false }, window, cx);
            }
        });
    }
}

pub(crate) fn open<V: FormDialogView>(
    view: Entity<V>,
    options: FormDialogOptions,
    window: &mut Window,
    cx: &mut App,
) {
    let initial_focus = options.initial_focus.clone();
    let title = options.title.clone();
    let footer = cx.new(|cx| FormDialogFooter {
        _observe: cx.observe(&view, |_, _, cx| cx.notify()),
        view: view.clone(),
        options,
    });
    let dialog_view = view.clone();
    window.open_dialog(cx, move |dialog, window, cx| {
        let confirm = dialog_view.clone();
        let cancel = dialog_view.clone();
        let close = dialog_view.clone();
        dialog
            .title(title.clone())
            .w(V::width(window, cx).min(window.viewport_size().width - px(24.)))
            .text_size(crate::ui::compact::rem(crate::ui::compact::BODY))
            .overlay_closable(false)
            .child(dialog_view.clone())
            .footer(footer.clone())
            .on_ok(move |_, window, cx| confirm.update(cx, |v, cx| submit(v, window, cx)))
            .on_cancel(move |_, _, cx| cancel.read(cx).dialog().phase == Phase::Editing)
            .on_close(move |_, _, cx| {
                close.update(cx, |v, cx| {
                    v.closing();
                    v.dialog_mut().close();
                    cx.notify();
                })
            })
    });
    window.defer(cx, move |window, cx| {
        if !view.read(cx).dialog().closed() {
            window.focus(&initial_focus, cx);
        }
    });
}

struct FormDialogFooter<V: FormDialogView> {
    view: Entity<V>,
    options: FormDialogOptions,
    _observe: Subscription,
}

impl<V: FormDialogView> Render for FormDialogFooter<V> {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.view.read(cx).dialog();
        let cancel = state.focus.clone();
        let cancel_enter = state.focus.clone();
        let confirm = state.focus.clone();
        div().id(self.options.footer_id).test_support().child(
            DialogFooter::new()
                .child(
                    // Kit's styled Button owns an internal focus handle. Use its
                    // base primitive here so deletion can actually focus Cancel.
                    BaseButton::new(self.options.cancel_id)
                        .track_focus(&state.cancel_focus)
                        .accessibility_label(t!("project.cancel"))
                        .h(crate::ui::compact::rem(crate::ui::compact::CONTROL))
                        .px_2p5()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().input)
                        .bg(cx.theme().input_background())
                        .text_color(cx.theme().button_foreground)
                        .text_size(crate::ui::compact::rem(crate::ui::compact::BODY))
                        .line_height(relative(1.25))
                        .hover(|b| b.bg(cx.theme().button_hover))
                        .when(state.cancel_focus.is_focused(window), |b| {
                            b.border_color(cx.theme().ring)
                        })
                        .disabled(state.phase != Phase::Editing)
                        // Dialog binds Enter to Confirm even when a button has
                        // focus. Keep the focused Cancel button authoritative.
                        .on_action(move |_: &Confirm, window, cx| {
                            cancel_enter.dispatch_action(&Cancel, window, cx)
                        })
                        .child(t!("project.cancel").into_owned())
                        .on_click(move |_, window, cx| cancel.dispatch_action(&Cancel, window, cx)),
                )
                .child(
                    crate::ui::compact::button(Button::new(self.options.submit_id))
                        .primary()
                        .label(self.options.submit_label.clone())
                        .disabled(!state.can_submit())
                        .on_click(move |_, window, cx| {
                            confirm.dispatch_action(&Confirm { secondary: false }, window, cx)
                        }),
                ),
        )
    }
}

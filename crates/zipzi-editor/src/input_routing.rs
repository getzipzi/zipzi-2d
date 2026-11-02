//! Window input orchestration: collect facts, decide without mutation, then execute.
mod facts;
mod pointer;
mod policy;
mod region;
mod target;

use crate::*;
use policy::{KeyDecision, decide};
pub(crate) use region::{CaptureCancelKey, CapturedInput, CoreCleanup, InputRegion};
use std::{cell::Cell, rc::Rc};
pub(crate) use target::command_at;
pub(crate) use target::{InputTarget, LocalTarget};

#[derive(Default)]
pub(crate) struct InputRouter {
    inside: Rc<Cell<bool>>,
    pub menu_target: Rc<Cell<Option<LocalTarget>>>,
}
impl InputRouter {
    pub fn deactivate(&self) {
        self.inside.set(false);
        self.menu_target.set(None);
    }
}

impl EditorView {
    pub(crate) fn route_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.collect_key_facts(event, window, cx);
        // Canvas nudges must not consume text caret movement, tree navigation,
        // timeline stepping, modal input or an active pointer capture.
        if !snapshot.facts.focus.blocked
            && !snapshot.facts.focus.text_focused
            && snapshot.facts.focus.editor_focused
            && snapshot.capture.is_none()
            && !event.prefer_character_input
        {
            let target = snapshot.facts.focused.or(snapshot.facts.hovered);
            if let Some(LocalTarget::Viewport(binding)) = target.map(|t| t.local) {
                let modifiers = event.keystroke.modifiers;
                if binding.kind == zipzi_editor_core::ViewportKind::Modeling
                    && !modifiers.control
                    && !modifiers.platform
                    && !self.editor.manual_for_viewport(binding.viewport)
                {
                    let direction = match event.keystroke.key.as_str() {
                        "left" => Some((-1., 0.)),
                        "right" => Some((1., 0.)),
                        "up" => Some((0., 1.)),
                        "down" => Some((0., -1.)),
                        _ => None,
                    };
                    if let Some((x, y)) = direction {
                        let step = if modifiers.alt {
                            0.1
                        } else if modifiers.shift {
                            10.
                        } else {
                            1.
                        };
                        let result = self.editor.nudge_selection(
                            binding,
                            zipzi_editor_core::ModelVector {
                                x: x * step,
                                y: y * step,
                            },
                        );
                        self.complete_edit(result, cx);
                        cx.stop_propagation();
                        return;
                    }
                }
            }
        }
        let decision = decide(
            &snapshot.facts,
            &cx.global::<settings::Settings>().bindings,
            |binding| {
                event
                    .keystroke
                    .should_match(&KeybindingKeystroke::new_with_mapper(
                        Keystroke::parse(&binding.key).expect("validated binding"),
                        false,
                        cx.keyboard_mapper().as_ref(),
                    ))
            },
        );
        self.execute_key_decision(decision, snapshot.capture, window, cx);
    }

    fn execute_key_decision(
        &mut self,
        decision: KeyDecision,
        capture: Option<CapturedInput>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match decision {
            KeyDecision::Ignore => {}
            KeyDecision::Consume => cx.stop_propagation(),
            KeyDecision::CancelCapture => {
                capture
                    .expect("cancel decision requires a capture")
                    .cancel(self, window, cx);
                self.redraw(cx);
                cx.stop_propagation();
            }
            KeyDecision::Invoke { id, target: local } => {
                let command = target::command_at(&self.editor, id, local);
                // Disabled local commands still consume their key.
                cx.stop_propagation();
                self.run_invocation(command, window, cx);
            }
        }
    }
    pub(crate) fn cancel_input_captures(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Include hidden instances during lifecycle cleanup; their core bindings
        // may already be invalid, but their local pointer state still needs release.
        let regions = self.all_input_regions(cx);
        let captures: Vec<_> = regions
            .iter()
            .filter_map(|region| region.captured(&self.editor, cx))
            .collect();
        for capture in captures {
            capture.cancel(self, window, cx);
        }
    }
}

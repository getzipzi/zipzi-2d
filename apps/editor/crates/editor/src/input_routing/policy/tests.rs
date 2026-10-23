use super::*;
use editor_core::{
    Editor, PanelKind, TimelineBinding, TimelineId, ViewportBinding, ViewportId, ViewportKind,
};
use std::cell::Cell;

fn facts() -> KeyFacts {
    KeyFacts {
        focus: FocusFacts {
            editor_focused: true,
            ..Default::default()
        },
        prefer_character_input: false,
        held: false,
        escape: false,
        unmodified: true,
        capture: None,
        hovered: None,
        focused: None,
    }
}
fn set_keyboard(facts: &mut KeyFacts, keyboard: KeyboardOwner) {
    facts.focus = FocusFacts {
        blocked: keyboard == KeyboardOwner::Control,
        text_focused: keyboard == KeyboardOwner::Text,
        editor_focused: keyboard == KeyboardOwner::Editor,
    };
}
fn bindings(ids: &[CommandId]) -> Vec<EffectiveBinding> {
    ids.iter()
        .map(|id| EffectiveBinding {
            command: *id,
            key: "match".into(),
        })
        .collect()
}
fn targets() -> [InputTarget; 3] {
    let document = Editor::new().unwrap().active_id().unwrap();
    [
        InputTarget::new(LocalTarget::Viewport(ViewportBinding {
            viewport: ViewportId::new(),
            document,
            kind: ViewportKind::Modeling,
            generation: 1,
        })),
        InputTarget::new(LocalTarget::Timeline(TimelineBinding {
            panel: TimelineId::new(),
            document,
            generation: 1,
        })),
        InputTarget::new(LocalTarget::Panel(PanelKind::Properties)),
    ]
}

#[test]
fn capture_precedes_hover_then_focus_and_never_reuses_an_absent_target() {
    let [focused, hovered, captured] = targets();
    let mut facts = facts();
    facts.focused = Some(focused);
    facts.hovered = Some(hovered);
    facts.capture = Some(CaptureFacts {
        target: captured.local,
        cancel_key: CaptureCancelKey::CommandBinding,
    });
    let bindings = bindings(&[CommandId::Undo]);
    for expected in [
        Some(captured.local),
        Some(hovered.local),
        Some(focused.local),
        None,
    ] {
        assert_eq!(
            decide(&facts, &bindings, |_| true),
            KeyDecision::Invoke {
                id: CommandId::Undo,
                target: expected
            }
        );
        if facts.capture.is_some() {
            facts.capture = None;
        } else if facts.hovered.is_some() {
            facts.hovered = None;
        } else {
            facts.focused = None;
        }
    }
}

#[test]
fn focused_palette_binding_precedes_disjoint_hovered_viewport_binding() {
    let mut facts = facts();
    facts.hovered = Some(targets()[0]);
    facts.focused = Some(InputTarget::new(LocalTarget::Panel(PanelKind::MeshTree)));
    let bindings = bindings(&[CommandId::ZoomIn, CommandId::PaletteMoveUp]);
    assert_eq!(decide(&facts, &bindings, |_| true), KeyDecision::Ignore);
    facts.focused = None;
    assert_eq!(
        decide(&facts, &bindings, |_| true),
        KeyDecision::Invoke {
            id: CommandId::ZoomIn,
            target: facts.hovered.map(|target| target.local)
        }
    );
}

#[test]
fn scopes_are_filtered_before_matching_and_text_only_keeps_window_commands() {
    let bindings = bindings(&[
        CommandId::CommitProperty,
        CommandId::ZoomIn,
        CommandId::TimelineNextFrame,
        CommandId::Undo,
        CommandId::Save,
    ]);
    let [viewport, timeline, panel] = targets();
    for (target, expected) in [
        (Some(viewport), CommandId::ZoomIn),
        (Some(timeline), CommandId::TimelineNextFrame),
        (Some(panel), CommandId::Undo),
        (None, CommandId::Undo),
    ] {
        let mut facts = facts();
        facts.hovered = target;
        let matched = Cell::new(0);
        assert_eq!(
            decide(&facts, &bindings, |binding| {
                matched.set(matched.get() + 1);
                assert_eq!(binding.command, expected);
                true
            }),
            KeyDecision::Invoke {
                id: expected,
                target: target.map(|target| target.local)
            }
        );
        assert_eq!(matched.get(), 1);
        set_keyboard(&mut facts, KeyboardOwner::Text);
        assert_eq!(
            decide(&facts, &bindings, |binding| {
                assert_eq!(binding.command, CommandId::Save);
                true
            }),
            KeyDecision::Invoke {
                id: CommandId::Save,
                target: facts.target().map(|target| target.local)
            }
        );
    }
}

#[test]
fn ime_and_control_ownership_block_matching_binding_and_capture_cancellation() {
    for (keyboard, ime) in [
        (KeyboardOwner::Control, false),
        (KeyboardOwner::Editor, true),
        (KeyboardOwner::Text, true),
    ] {
        let mut facts = facts();
        set_keyboard(&mut facts, keyboard);
        facts.prefer_character_input = ime;
        facts.escape = true;
        facts.capture = Some(CaptureFacts {
            target: targets()[1].local,
            cancel_key: CaptureCancelKey::Escape,
        });
        assert_eq!(
            decide(&facts, &bindings(&[CommandId::Save]), |_| panic!(
                "blocked matching"
            )),
            KeyDecision::Ignore
        );
    }
}

#[test]
fn captured_escape_keeps_priority_over_text_modifiers_and_repeat_but_configured_cancel_does_not() {
    let mut facts = facts();
    facts.escape = true;
    facts.held = true;
    facts.unmodified = false;
    set_keyboard(&mut facts, KeyboardOwner::Text);
    facts.capture = Some(CaptureFacts {
        target: targets()[1].local,
        cancel_key: CaptureCancelKey::Escape,
    });
    assert_eq!(
        decide(&facts, &[], |_| panic!("reserved escape matched")),
        KeyDecision::CancelCapture
    );
    facts.capture.as_mut().unwrap().cancel_key = CaptureCancelKey::CommandBinding;
    assert_eq!(decide(&facts, &[], |_| true), KeyDecision::Ignore);
    set_keyboard(&mut facts, KeyboardOwner::Editor);
    facts.held = false;
    facts.escape = false;
    facts.capture.as_mut().unwrap().target = targets()[0].local;
    assert_eq!(
        decide(&facts, &bindings(&[CommandId::CancelGesture]), |_| true),
        KeyDecision::Invoke {
            id: CommandId::CancelGesture,
            target: facts.target().map(|target| target.local)
        }
    );
}

#[test]
fn declared_idle_escape_only_overrides_bindings_for_unmodified_editor_input() {
    let mut facts = facts();
    facts.escape = true;
    let mut target = targets()[0];
    target.idle_escape = Some(CommandId::Deselect);
    facts.hovered = Some(target);
    let bindings = bindings(&[CommandId::Save]);
    assert_eq!(
        decide(&facts, &bindings, |_| panic!("idle escape must win")),
        KeyDecision::Invoke {
            id: CommandId::Deselect,
            target: facts.target().map(|target| target.local)
        }
    );
    facts.unmodified = false;
    assert_eq!(
        decide(&facts, &bindings, |_| true),
        KeyDecision::Invoke {
            id: CommandId::Save,
            target: facts.target().map(|target| target.local)
        }
    );
    facts.unmodified = true;
    set_keyboard(&mut facts, KeyboardOwner::Text);
    assert_eq!(
        decide(&facts, &bindings, |_| true),
        KeyDecision::Invoke {
            id: CommandId::Save,
            target: facts.target().map(|target| target.local)
        }
    );
    set_keyboard(&mut facts, KeyboardOwner::Editor);
    facts.capture = Some(CaptureFacts {
        target: target.local,
        cancel_key: CaptureCancelKey::CommandBinding,
    });
    assert_eq!(decide(&facts, &[], |_| true), KeyDecision::Ignore);
}

#[test]
fn held_nonrepeat_binding_is_consumed_without_falling_through_or_executing() {
    let mut facts = facts();
    facts.hovered = Some(targets()[0]);
    facts.held = true;
    let bindings = bindings(&[CommandId::SelectTool, CommandId::ZoomIn]);
    assert_eq!(decide(&facts, &bindings, |_| true), KeyDecision::Consume);
    assert_eq!(
        decide(&facts, &bindings[1..], |_| true),
        KeyDecision::Invoke {
            id: CommandId::ZoomIn,
            target: facts.target().map(|target| target.local)
        }
    );
    facts.held = false;
    assert_eq!(
        decide(&facts, &bindings, |_| true),
        KeyDecision::Invoke {
            id: CommandId::SelectTool,
            target: facts.target().map(|target| target.local)
        }
    );
}

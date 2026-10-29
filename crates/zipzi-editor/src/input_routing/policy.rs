//! Routing policy over an immutable snapshot. No window/entity mutation or command execution.
use super::{CaptureCancelKey, InputTarget, LocalTarget};
use zipzi_editor_core::{CommandId, CommandScope, EffectiveBinding};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeyboardOwner {
    Editor,
    Text,
    Control,
}

#[derive(Clone, Copy, Default)]
pub(super) struct FocusFacts {
    pub blocked: bool,
    pub text_focused: bool,
    pub editor_focused: bool,
}
impl FocusFacts {
    fn owner(self) -> KeyboardOwner {
        if self.blocked {
            KeyboardOwner::Control
        } else if self.text_focused {
            KeyboardOwner::Text
        } else if self.editor_focused {
            KeyboardOwner::Editor
        } else {
            KeyboardOwner::Control
        }
    }
}

#[derive(Clone, Copy)]
pub(super) struct CaptureFacts {
    pub target: LocalTarget,
    pub cancel_key: CaptureCancelKey,
}

pub(super) struct KeyFacts {
    pub focus: FocusFacts,
    pub prefer_character_input: bool,
    pub held: bool,
    pub escape: bool,
    pub unmodified: bool,
    pub capture: Option<CaptureFacts>,
    pub hovered: Option<InputTarget>,
    pub focused: Option<InputTarget>,
}
impl KeyFacts {
    fn target(&self) -> Option<InputTarget> {
        self.capture
            .map(|capture| InputTarget::new(capture.target))
            .or(self.hovered)
            .or(self.focused)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum KeyDecision {
    Ignore,
    CancelCapture,
    Consume,
    Invoke {
        id: CommandId,
        target: Option<LocalTarget>,
    },
}

/// Key matching uses the host mapper. Scope filtering precedes matching;
/// the first applicable binding wins, including disabled commands and held non-repeat keys.
pub(super) fn decide(
    facts: &KeyFacts,
    bindings: &[EffectiveBinding],
    mut matches: impl FnMut(&EffectiveBinding) -> bool,
) -> KeyDecision {
    let keyboard = facts.focus.owner();
    if facts.prefer_character_input || keyboard == KeyboardOwner::Control {
        return KeyDecision::Ignore;
    }
    if facts.escape {
        if let Some(capture) = facts.capture {
            if capture.cancel_key == CaptureCancelKey::Escape {
                return KeyDecision::CancelCapture;
            }
        }
    }
    let target = facts.target();
    let text = keyboard == KeyboardOwner::Text;
    let object_target = facts
        .capture
        .map(|c| InputTarget::new(c.target))
        .or(facts.focused)
        .or(facts.hovered);
    // Palette bindings are dispatched by GPUI at list focus. Reserve a matching
    // local key before the hovered viewport can take its disjoint binding.
    if !text
        && facts.capture.is_none()
        && matches!(
            facts.focused.map(|target| target.local),
            Some(LocalTarget::Panel(
                zipzi_editor_core::PanelKind::PartsTree
                    | zipzi_editor_core::PanelKind::Deformers
                    | zipzi_editor_core::PanelKind::Parameters
                    | zipzi_editor_core::PanelKind::AnimationMotions
                    | zipzi_editor_core::PanelKind::AnimationExpressions
                    | zipzi_editor_core::PanelKind::AnimationPhysics
                    | zipzi_editor_core::PanelKind::AnimationPose
            ))
        )
        && bindings.iter().any(|binding| {
            binding.command.spec().scope == CommandScope::Palette && matches(binding)
        })
    {
        return KeyDecision::Ignore;
    }
    let idle_escape = (facts.escape && facts.unmodified && !text)
        .then(|| target.and_then(|target| target.idle_escape))
        .flatten();
    let command = idle_escape.or_else(|| {
        bindings
            .iter()
            .find(|binding| {
                let applicable = match binding.command.spec().scope {
                    CommandScope::Window => true,
                    CommandScope::Document => !text,
                    CommandScope::Viewport => {
                        !text
                            && matches!(
                                target.map(|target| target.local),
                                Some(LocalTarget::Viewport(_))
                            )
                    }
                    CommandScope::Timeline => {
                        !text
                            && matches!(
                                target.map(|target| target.local),
                                Some(LocalTarget::Timeline(_))
                            )
                    }
                    CommandScope::Objects => {
                        !text
                            && matches!(
                                object_target.map(|t| t.local),
                                Some(LocalTarget::Viewport(_))
                                    | Some(LocalTarget::Panel(
                                        zipzi_editor_core::PanelKind::PartsTree
                                            | zipzi_editor_core::PanelKind::Deformers
                                    ))
                            )
                    }
                    CommandScope::Property | CommandScope::Palette => false,
                };
                applicable && matches(binding)
            })
            .map(|binding| binding.command)
    });
    match command {
        None => KeyDecision::Ignore,
        Some(id) if facts.held && !id.spec().repeat => KeyDecision::Consume,
        Some(id) => KeyDecision::Invoke {
            id,
            target: if id.spec().scope == CommandScope::Objects {
                object_target
            } else {
                target
            }
            .map(|target| target.local),
        },
    }
}

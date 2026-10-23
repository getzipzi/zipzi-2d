//! Hand tool and captured navigation share their operation vocabulary.
use super::hints::*;
use crate::CommandId;

pub(super) const IDLE_HINTS: &[InteractionHint] = &[
    InteractionHint::Primary(HintAction::Pan),
    InteractionHint::Command(CommandId::SelectTool),
];
pub(super) const CAPTURE_HINTS: &[InteractionHint] = &[
    InteractionHint::Drag(HintAction::Pan),
    InteractionHint::Command(CommandId::CancelGesture),
];

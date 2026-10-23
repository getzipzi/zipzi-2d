//! Semantic interaction hints. Tools declare them; UI resolves keys and translates labels.
use crate::CommandId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HintAction {
    Pan,
    Transform,
    LassoSelect,
    AddSelection,
    RemoveSelection,
    GlueBind,
    ScrubTime,
    BoxSelect,
    MoveKeys,
    AdjustCurve,
    MoveEvent,
    Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InteractionHint {
    Drag(HintAction),
    Primary(HintAction),
    Shift(HintAction),
    Alt(HintAction),
    Command(CommandId),
    Escape(CommandId),
}

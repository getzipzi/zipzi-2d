//! Glue uses the shared click/region gesture and its own idle guidance.
use super::hints::*;
use crate::CommandId;

pub(super) const HINTS: &[InteractionHint] = &[
    InteractionHint::Primary(HintAction::GlueBind),
    InteractionHint::Command(CommandId::SelectTool),
];

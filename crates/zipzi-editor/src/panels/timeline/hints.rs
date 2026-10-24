//! Timeline operation guidance, consumed by the shared status bar.
use super::TimelineOperation;
use std::{cell::Cell, rc::Rc};
use zipzi_editor_core::{CommandId, HintAction, InteractionHint};

/// Timeline-owned publication for readers that run while the Timeline entity is borrowed.
/// The dock keeps a read handle; routing has no Timeline state.
#[derive(Clone, Default)]
pub(crate) struct TimelineHints(Rc<Cell<HintSnapshot>>);

#[derive(Clone, Copy, Default)]
struct HintSnapshot {
    capture: Option<(TimelineOperation, TimelineCaptureKind)>,
    pan_tool: bool,
}

impl TimelineHints {
    pub(super) fn publish(
        &self,
        capture: Option<(TimelineOperation, TimelineCaptureKind)>,
        pan_tool: bool,
    ) {
        self.0.set(HintSnapshot { capture, pan_tool });
    }
    pub(super) fn clear(&self) {
        self.0.set(HintSnapshot::default());
    }
    pub(crate) fn captured(
        &self,
        editor: &zipzi_editor_core::Editor,
    ) -> Option<&'static [InteractionHint]> {
        self.0
            .get()
            .capture
            .filter(|(operation, _)| operation.valid(editor))
            .map(|(_, kind)| kind.hints())
    }
    pub(crate) fn idle(&self) -> &'static [InteractionHint] {
        if self.0.get().pan_tool {
            PAN_HINTS
        } else {
            &[]
        }
    }
}

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

pub(crate) const PAN_HINTS: &[InteractionHint] = &[InteractionHint::Primary(HintAction::Pan)];

impl TimelineCaptureKind {
    pub(crate) fn hints(self) -> &'static [InteractionHint] {
        use HintAction::*;
        use InteractionHint::{Drag, Escape};
        match self {
            Self::Pan => &[Drag(Pan)],
            Self::Seek => &[Drag(ScrubTime), Escape(CommandId::CancelGesture)],
            Self::Marquee => &[Drag(BoxSelect), Escape(CommandId::CancelGesture)],
            Self::Keys => &[Drag(MoveKeys), Escape(CommandId::CancelGesture)],
            Self::Handle => &[Drag(AdjustCurve), Escape(CommandId::CancelGesture)],
            Self::Event => &[Drag(MoveEvent), Escape(CommandId::CancelGesture)],
            Self::Number => &[Drag(Value), Escape(CommandId::CancelGesture)],
        }
    }
}

mod numbers;

use super::*;
use crate::ui::slide_number_input::{
    SlideNumberChange, SlideNumberInput, SlideNumberOptions, format_one_decimal,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum Field {
    RowValue(TimelineRowId),
    FrameStep,
    RetimeDuration,
}
impl Field {
    fn label(&self) -> String {
        t!(match self {
            Self::FrameStep => "timeline.frame_step_n",
            Self::RetimeDuration => "timeline.retime_target",
            _ => "timeline.value",
        })
        .into_owned()
    }
    fn value_format(&self) -> fn(f64) -> String {
        match self {
            Self::RowValue(_) => format_one_decimal,
            Self::FrameStep => |value| format!("{value:.0}"),
            Self::RetimeDuration => axis::number,
        }
    }
    fn number_options(
        &self,
        value: f64,
        range: (f64, f64),
        _fps: f32,
        disabled: bool,
    ) -> SlideNumberOptions {
        SlideNumberOptions {
            value,
            min: range.0,
            max: range.1,
            step: if matches!(self, Self::FrameStep) {
                1.
            } else {
                0.001
            },
            drag_distance: (matches!(self, Self::RowValue(_)) && range.1 - range.0 < 1e6)
                .then_some(400.),
            disabled,
        }
    }
    pub(super) fn exists(&self, projection: &TimelineProjection) -> bool {
        match self {
            Self::RowValue(id) => projection.rows.iter().any(|row| &row.id == id),
            Self::FrameStep | Self::RetimeDuration => true,
        }
    }
}

pub(super) struct FieldControl {
    pub(super) input: Entity<InputState>,
    displayed: String,
    pub(super) number: Entity<SlideNumberInput>,
    _events: Vec<Subscription>,
}
pub(super) struct NumberEdit {
    pub token: TimelineCandidateIdentity,
    pub(super) field: Field,
    command: TimelineCommand,
    pub sliding: bool,
    pub drag: Option<gpui_kit::ActiveDragToken>,
}
impl Timeline {
    pub(super) fn add_event(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let time = owner
            .read(cx)
            .editor
            .animation()
            .map_or(0., |a| a.edit_time as f64);
        let before = owner
            .read(cx)
            .editor
            .timeline_projection()
            .map(|p| p.events.iter().map(|e| e.id.clone()).collect::<Vec<_>>())
            .unwrap_or_default();
        self.execute(
            TimelineCommand::Event {
                id: None,
                time,
                value: String::new(),
            },
            window,
            cx,
        );
        let added = owner.read(cx).editor.timeline_projection().and_then(|p| {
            p.events
                .iter()
                .find(|e| !before.contains(&e.id))
                .map(|e| e.id.clone())
        });
        if let Some(id) = added {
            self.change_view(
                move |view| {
                    view.keys.clear();
                    view.events.clear();
                    view.events.insert(id);
                    view.channels.clear();
                    view.channels.insert(TimelineRowId::Events);
                    view.reveal_row = Some(TimelineRowId::Events);
                },
                cx,
            );
        }
    }
}

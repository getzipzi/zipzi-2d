//! Playhead scrubbing from pointer input and timeline navigation.
use super::{drawing::x_time, input::Gesture, *};
use zipzi_editor_core::{Editor, TimelineScrubToken};

#[derive(Clone, Copy)]
struct SeekMapping {
    left: Pixels,
    width: Pixels,
    range: (f64, f64),
    end: f64,
    fps: f32,
    snap: bool,
}

impl SeekMapping {
    fn time_at(self, position: Point<Pixels>) -> Option<f32> {
        let x = (position.x - self.left).as_f32();
        if !x.is_finite() || self.width <= px(0.) {
            return None;
        }
        let time = x_time(x, self.range, self.width.as_f32()).clamp(0., self.end);
        timeline_time(time, self.fps, self.snap).ok()
    }
}

#[derive(Default)]
pub(super) struct SeekInput {
    model_revision: u64,
    pub(super) pointer_trace: u64,
}

impl Timeline {
    pub(super) fn seek_token(&self) -> Option<TimelineScrubToken> {
        match self.gesture {
            Some(Gesture::Seek(token)) => Some(token),
            _ => None,
        }
    }

    pub(super) fn start_seek(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.allowed(window, cx) || crate::benchmark::ignore_native_pointer_event() {
            return;
        }
        self.finish_number(cx);
        let Some(target) = self.target(cx) else {
            return;
        };
        self.edge_scroll = None;
        let start = self
            .owner
            .update(cx, |owner, cx| {
                let time = self.seek_mapping(&owner.editor)?.time_at(event.position)?;
                let result = owner.editor.begin_timeline_scrub(target.binding, time);
                let started = result
                    .as_ref()
                    .ok()
                    .map(|token| (*token, owner.editor.document().unwrap().model_generation()));
                owner.animation_anchor = None;
                owner.complete_edit(result.map(|_| ()), cx);
                started
            })
            .ok()
            .flatten();
        if let Some((token, revision)) = start {
            window.focus(&self.focus, cx);
            self.gesture = Some(Gesture::Seek(token));
            self.pointer = event.position;
            self.seek.pointer_trace = frame_trace::current_input();
            self.seek.model_revision = revision;
            self.start_edge_scroll(window, cx);
            self.sync_status_bar(cx);
            cx.notify();
        }
        cx.stop_propagation();
    }

    fn seek_mapping(&self, editor: &Editor) -> Option<SeekMapping> {
        let state = editor.timeline_view()?;
        let projection = editor.timeline_projection()?;
        let animation = editor.animation()?;
        let end = if animation.preview_mode == AnimationPreviewMode::Result {
            animation
                .result_playback_position()
                .map_or(projection.display_end, |p| {
                    f64::from(p.period).max(projection.display_end)
                })
        } else {
            projection.display_end
        };
        let bounds = self.ruler.get();
        Some(SeekMapping {
            left: bounds.left(),
            width: bounds.size.width,
            range: state.time_range,
            end,
            fps: projection.fps,
            snap: state.snap,
        })
    }

    pub(super) fn validate_seek(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<TimelineScrubToken> {
        let token = self.seek_token()?;
        if self.allowed(window, cx)
            && !window.has_active_sheet(cx)
            && window.is_visible()
            && self.focus.is_focused(window)
            && self.owner.upgrade().is_some_and(|owner| {
                let editor = &owner.read(cx).editor;
                editor.timeline_scrub_valid(token)
                    && editor
                        .document()
                        .is_some_and(|doc| doc.model_generation() == self.seek.model_revision)
            })
        {
            Some(token)
        } else {
            self.cancel(window, cx);
            None
        }
    }

    pub(super) fn seek_to_pointer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(token) = self.validate_seek(window, cx) else {
            return;
        };
        let input = frame_trace::current_input();
        let trace = if input != 0 {
            input
        } else {
            self.seek.pointer_trace
        };
        let _ = self.owner.update(cx, |owner, cx| {
            if let Some(time) = self
                .seek_mapping(&owner.editor)
                .and_then(|mapping| mapping.time_at(self.pointer))
            {
                frame_trace::emit(trace, "timeline_seek_target", &[]);
                frame_trace::emit(trace, "timeline_seek_apply_begin", &[]);
                let result = owner.editor.update_timeline_scrub(token, time);
                owner.complete_edit_for_input(result, trace, cx);
                frame_trace::emit(trace, "timeline_seek_apply_end", &[]);
            }
        });
    }

    pub(super) fn release_seek(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(token) = self.validate_seek(window, cx) else {
            return;
        };
        self.edge_scroll = None;
        self.gesture = None;
        let _ = self.owner.update(cx, |owner, cx| {
            let result = self
                .seek_mapping(&owner.editor)
                .and_then(|mapping| {
                    mapping
                        .time_at(position)
                        .or_else(|| mapping.time_at(self.pointer))
                })
                .map_or_else(
                    || Err(anyhow::anyhow!("Invalid timeline release position")),
                    |time| owner.editor.finish_timeline_scrub(token, time),
                );
            if result.is_err() {
                owner.editor.cancel_timeline_scrub(token);
            }
            owner.complete_edit(result, cx);
        });
        self.sync_status_bar(cx);
        cx.notify();
    }
}

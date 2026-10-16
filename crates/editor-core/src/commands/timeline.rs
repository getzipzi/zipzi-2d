//! User commands share the same execution path across shortcuts and controls.
//! Parameterized candidate operations remain in the Timeline editing module.
use crate::{CommandId, DocumentId, EditingContext, Editor, TimelinePasteTarget};
use anyhow::{Result, bail};

impl Editor {
    pub(super) fn timeline_command_enabled(&self, id: CommandId) -> bool {
        use CommandId::*;
        if self.editing_context() != Some(EditingContext::Animation)
            || self.is_busy()
            || self.exiting()
        {
            return false;
        }
        let Some(projection) = self.timeline_projection() else {
            return false;
        };
        let Some(view) = self.timeline_view() else {
            return false;
        };
        let editable = projection.editable && self.animation().is_some_and(|a| !a.playing);
        match id {
            TimelineCopy => !view.keys.is_empty(),
            TimelineDelete => {
                editable
                    && (!view.keys.is_empty() || !view.events.is_empty())
                    && view.keys.iter().all(|key| {
                        projection
                            .tracks
                            .get(&key.track_id)
                            .is_some_and(|track| track.editable)
                    })
            }
            TimelinePaste => editable && self.timeline_clipboard_available(),
            TimelinePreviousFrame
            | TimelineNextFrame
            | TimelinePreviousKey
            | TimelineNextKey
            | TimelineStart
            | TimelineEnd => self.animation_playable(),
            _ => false,
        }
    }

    pub(super) fn dispatch_timeline_command(
        &mut self,
        document: DocumentId,
        id: CommandId,
    ) -> Result<()> {
        use CommandId::*;
        match id {
            TimelineDelete => return self.delete_timeline_selection(document),
            TimelineCopy => return self.copy_timeline_selection(document).map(|_| ()),
            TimelinePaste => {
                let selected = self.timeline_view().and_then(|view| {
                    (self.timeline_clipboard_can_retarget() && view.channels.len() == 1)
                        .then(|| view.channels.first().unwrap())
                });
                let paste = selected
                    .and_then(|id| self.timeline_rows()?.iter().find(|row| &row.id == id))
                    .and_then(|row| {
                        row.channel.clone().map(|channel| TimelinePasteTarget {
                            channel,
                            track_id: row.track_id.clone(),
                        })
                    });
                return self.paste_timeline(document, paste);
            }
            _ => {}
        }
        let projection = self.timeline_projection().unwrap();
        let time = f64::from(self.animation().unwrap().edit_time);
        let keys = || {
            projection
                .rows
                .iter()
                .filter_map(|row| row.track_id.as_ref())
                .flat_map(|id| &projection.tracks[id].keys)
        };
        let destination = match id {
            TimelinePreviousKey => keys()
                .filter(|key| key.time < time)
                .max_by(|a, b| a.time.total_cmp(&b.time)),
            TimelineNextKey => keys()
                .filter(|key| key.time > time)
                .min_by(|a, b| a.time.total_cmp(&b.time)),
            _ => None,
        };
        let next = match id {
            TimelineStart => 0.,
            TimelineEnd => projection.duration,
            TimelinePreviousFrame => f64::from(time as f32 - 1. / projection.fps),
            TimelineNextFrame => f64::from(time as f32 + 1. / projection.fps),
            TimelinePreviousKey => destination.map_or(0., |key| key.time),
            TimelineNextKey => destination.map_or(projection.duration, |key| key.time),
            _ => bail!("Not a Timeline command"),
        }
        .clamp(0., projection.display_end);
        self.seek_timeline(document, next as f32)?;
        if let Some(key) = destination {
            self.reveal_timeline_keys(document, &[key.reference.clone()])
        } else {
            self.update_timeline_view(document, |view| view.reveal_time(next))
        }
    }
}

//! Payload refreshes for Motion library fields with unchanged relationships.
use super::*;
use serde_json::{Value, json};
use zipzi_core::document::{MotionClip, MotionTrack};

pub(super) fn motion_properties(clip: &MotionClip) -> Value {
    json!({"name":clip.name,"duration":clip.duration,"fps":clip.fps,"looping":clip.looping,
        "fade_in":clip.fade_in,"fade_out":clip.fade_out,"events":clip.events,"extensions":clip.extensions})
}

pub(super) fn motion_track_properties(track: &MotionTrack) -> Value {
    json!({"target":track.target,"initial":track.initial,"segments":track.segments.len(),
        "fade_in":track.fade_in,"fade_out":track.fade_out,"extensions":track.extensions})
}

impl ObjectIndex {
    /// Preserve the index only while all indexed children and endpoints keep their slots.
    pub(crate) fn refresh_motion_clip(&mut self, model: &Model, before: &MotionClip) -> bool {
        let Some(after) = model.motion(&before.id) else {
            return false;
        };
        if !std::ptr::eq(&*before.tracks, &*after.tracks) {
            if before.tracks.len() != after.tracks.len() {
                return false;
            }
            for (old, new) in before.tracks.iter().zip(&after.tracks) {
                if old.id != new.id || old.target != new.target || old.initial.id != new.initial.id
                {
                    return false;
                }
                let same_segments = Arc::ptr_eq(&old.segments, &new.segments);
                if !same_segments
                    && (old.segments.len() != new.segments.len()
                        || !old
                            .segments
                            .iter()
                            .zip(new.segments.iter())
                            .all(|(a, b)| a.end.id == b.end.id))
                {
                    return false;
                }
                if old.initial != new.initial
                    || old.fade_in != new.fade_in
                    || old.fade_out != new.fade_out
                    || old.extensions != new.extensions
                {
                    let track = ObjectRef::new(ObjectKind::MotionTrack, &new.id);
                    let Some(&slot) = self.structure.slots.get(&track) else {
                        return false;
                    };
                    self.objects[slot].properties = motion_track_properties(new);
                }
                if !self.refresh_motion_key(&old.initial, &new.initial) {
                    return false;
                }
                if !same_segments
                    && !old
                        .segments
                        .iter()
                        .zip(new.segments.iter())
                        .all(|(a, b)| self.refresh_motion_key(&a.end, &b.end))
                {
                    return false;
                }
            }
        }
        if !std::ptr::eq(&*before.events, &*after.events) {
            if before.events.len() != after.events.len() {
                return false;
            }
            for (old, new) in before.events.iter().zip(&after.events) {
                if old.id != new.id {
                    return false;
                }
                if old != new {
                    let object = ObjectRef::new(ObjectKind::MotionEvent, &new.id);
                    let Some(&slot) = self.structure.slots.get(&object) else {
                        return false;
                    };
                    let row = &mut self.objects[slot];
                    row.name = if new.value.is_empty() {
                        new.id.clone()
                    } else {
                        new.value.clone()
                    };
                    row.properties = json!(new);
                }
            }
        }
        self.refresh_motion_properties(model, &ObjectRef::new(ObjectKind::Motion, &after.id))
    }

    fn refresh_motion_key(
        &mut self,
        before: &zipzi_core::document::MotionKey,
        after: &zipzi_core::document::MotionKey,
    ) -> bool {
        if before != after {
            let object = ObjectRef::new(ObjectKind::MotionKey, &after.id);
            let Some(&slot) = self.structure.slots.get(&object) else {
                return false;
            };
            let row = &mut self.objects[slot];
            row.name = format!("Key {}", after.time);
            row.properties = json!(after);
        }
        true
    }

    pub(crate) fn refresh_motion_properties(&mut self, model: &Model, object: &ObjectRef) -> bool {
        let (name, properties) = match object.kind {
            ObjectKind::Motion => {
                let Some(clip) = model.motion(&object.id) else {
                    return false;
                };
                (clip.name.clone(), motion_properties(clip))
            }
            ObjectKind::MotionGroup => {
                let Some(group) = model.motion_group(&object.id) else {
                    return false;
                };
                (group.name.clone(), json!(group))
            }
            ObjectKind::MotionRegistration => {
                let Some((_, entry)) = model.motion_registration(&object.id) else {
                    return false;
                };
                (
                    model.motion(&entry.clip_id).unwrap().name.clone(),
                    json!(entry),
                )
            }
            _ => return false,
        };
        let Some(&slot) = self.structure.slots.get(object) else {
            return false;
        };
        let name = if name.is_empty() {
            object.id.clone()
        } else {
            name
        };
        let renamed = self.objects[slot].name != name;
        if renamed || self.objects[slot].properties != properties {
            let row = &mut self.objects[slot];
            row.name = name;
            row.properties = properties;
        }
        if object.kind == ObjectKind::Motion && renamed {
            // Registration labels follow their shared Clip, independent of ownership.
            for entry in model
                .motion_registrations()
                .filter(|e| e.clip_id == object.id)
            {
                if !self.refresh_motion_properties(
                    model,
                    &ObjectRef::new(ObjectKind::MotionRegistration, &entry.id),
                ) {
                    return false;
                }
            }
        }
        true
    }
}

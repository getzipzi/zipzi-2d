//! Payload refreshes for Motion library fields with unchanged relationships.
use super::*;
use serde_json::{Value, json};

pub(super) fn motion_properties(clip: &zipzi_core::document::MotionClip) -> Value {
    json!({"name":clip.name,"duration":clip.duration,"fps":clip.fps,"looping":clip.looping,
        "fade_in":clip.fade_in,"fade_out":clip.fade_out,"events":clip.events,"extensions":clip.extensions})
}

impl ObjectIndex {
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
        if self.objects[slot].name != name || self.objects[slot].properties != properties {
            self.objects[slot].name = name;
            self.objects[slot].properties = properties;
        }
        if object.kind == ObjectKind::Motion {
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

//! Domain operations on an isolated candidate. Record writes own all tracking;
//! model rules borrow the candidate without constructing a legacy Document.
use super::*;
use crate::model_validation::binding_coordinates::coordinates;
use crate::SceneTrackEvaluation;

fn checked(status: Status) -> Result<(), Status> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(status)
    }
}

impl DocumentBuilder {
    /// Check the object, its organizational owners and owned binding identities.
    pub fn authoring_unlocked(&self, id: &str) -> Result<(), Status> {
        self.content.model().authoring_unlocked(id)
    }

    /// Prepare a complete binding against the current candidate. Existing axes
    /// and samples retain their identities when numeric input is resubmitted.
    pub fn canonicalize_binding(&self, binding: &mut MeshBinding) -> Result<(), Status> {
        checked(self.content.model().canonicalize_binding(binding))
    }

    pub fn canonicalize_scene_binding(&self, binding: &mut SceneBinding) -> Result<(), Status> {
        checked(self.content.model().canonicalize_scene_binding(binding))
    }

    pub fn set_mesh_keyform(&mut self, id: &str, mut form: MeshKeyform) -> Result<(), Status> {
        self.write(|this| {
            this.authoring_unlocked(id)?;
            let mut binding = this
                .binding(id)
                .cloned()
                .ok_or_else(|| Status::error("MISSING_BINDING", id))?;
            coordinates(&binding.axes, &mut form.keys, &mut form.samples)?;
            let existing = binding
                .keyforms
                .iter_mut()
                .find(|f| f.samples == form.samples)
                .ok_or_else(|| Status::error("INVALID_KEY_COMBINATION", id))?;
            *existing = form;
            this.canonicalize_binding(&mut binding)?;
            this.replace_binding(binding)
        })
    }

    pub fn set_scene_keyform(&mut self, id: &str, mut form: SceneKeyform) -> Result<(), Status> {
        self.write(|this| {
            this.authoring_unlocked(id)?;
            let mut binding = this
                .scene_binding(id)
                .cloned()
                .ok_or_else(|| Status::error("MISSING_BINDING", id))?;
            let (keys, samples) = match &mut form {
                SceneKeyform::Warp(f) => (&mut f.keys, &mut f.samples),
                SceneKeyform::Rotation(f) => (&mut f.keys, &mut f.samples),
                SceneKeyform::Part(f) => (&mut f.keys, &mut f.samples),
            };
            coordinates(&binding.axes, keys, samples)?;
            checked(binding.track.replace(form))?;
            this.canonicalize_scene_binding(&mut binding)?;
            this.replace_scene_binding(binding)
        })
    }
}

impl DocumentState {
    pub fn contains_id(&self, id: &str) -> bool {
        self.content.model().contains_id(id)
    }
}
impl DocumentBuilder {
    pub fn validate_motion(&self, clip: &MotionClip) -> Status {
        self.content.model().validate_motion(clip)
    }
}

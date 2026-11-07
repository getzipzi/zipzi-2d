use super::*;
use zipzi_core::document::MotionClip;
impl Commands<'_> {
    pub fn create_motion(&mut self, clip: MotionClip) -> Result<(), Status> {
        self.run(|this| this.builder.create_authored_motion(clip))
    }
    pub fn replace_motion(&mut self, clip: MotionClip) -> Result<(), Status> {
        self.run(|this| this.builder.replace_authored_motion(clip))
    }
    pub fn edit_motion(&mut self, edit: MotionEdit) -> Result<(), Status> {
        self.run(|this| this.builder.edit_motion(edit))
    }
}

impl Commands<'_> {
    /// Apply one curve operation to a clip and install it through the same tracked
    /// Builder path as other motion writes. Any error aborts the enclosing Draft.
    pub fn edit_motion_curve(
        &mut self,
        id: &str,
        intent: crate::MotionCurveEdit,
    ) -> Result<(), Status> {
        use crate::{motion_curve::CurveWorkspace, MotionCurveEdit as E};
        self.run(|this| {
            let document = &*this.builder;
            let original = document
                .motion(id)
                .cloned()
                .ok_or_else(|| Status::error("MISSING_MOTION", id))?;
            let mut workspace = CurveWorkspace::new(document, original)?;
            match intent {
                E::MoveKeys(moves) => workspace.move_keys(document, &moves)?,
                E::InsertKey {
                    track,
                    id,
                    time,
                    value,
                } => workspace.insert_key(document, &track, &id, time, value)?,
                E::DeleteKeys(keys) => workspace.delete_keys(document, &keys)?,
                E::Interpolation { keys, kind } => {
                    workspace.set_interpolation(document, &keys, kind)?
                }
                E::BezierHandle { end, handle, point } => {
                    workspace.set_handle(document, &end, handle, point)?
                }
                E::ScaleTime { duration } => workspace.scale_time(document, duration)?,
            }
            this.builder.replace_authored_motion(workspace.into_clip())
        })
    }
}

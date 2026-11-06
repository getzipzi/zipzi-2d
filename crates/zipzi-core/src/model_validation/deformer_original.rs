use super::*;
impl Model<'_> {
    pub(crate) fn validate_deformer_original(
        &self,
        record: &DeformerOriginal,
    ) -> Result<(), Status> {
        if let Some(snapshot) = &record.snapshot {
            match &snapshot.shape {
                DeformerOriginalShape::Warp { warp, .. } => {
                    if warp.rows == 0
                        || warp.columns == 0
                        || warp.rows > 1024
                        || warp.columns > 1024
                        || warp.points.len() != ((warp.rows + 1) * (warp.columns + 1)) as usize
                    {
                        return Err(Status::error(
                            "INVALID_DEFORMER_ORIGINAL",
                            "Invalid saved lattice",
                        ));
                    }
                    checked(crate::geometry::validate_positions(&warp.points))?;
                    if let Some(controller) = &warp.bezier {
                        checked(controller.validate())?;
                    }
                }
                DeformerOriginalShape::Rotation { pose } => {
                    checked(pose_valid(pose, "original shape"))?
                }
            }
        }
        Ok(())
    }
}

fn checked(status: Status) -> Result<(), Status> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(status)
    }
}

use crate::AnimationError;
use zipzi_core::DrawableFrame;

/// Host Model opacity policy shared with the SDK observer: multiply each
/// drawable once, before masking/Offscreen composition. This is deliberately
/// different from scaling the final flattened image's alpha.
pub fn apply_model_opacity(frame: &mut DrawableFrame, opacity: f32) -> Result<(), AnimationError> {
    if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
        return Err(AnimationError::Evaluation(
            "Model opacity must be finite and between zero and one".into(),
        ));
    }
    for drawable in &mut frame.drawables {
        drawable.opacity *= opacity;
    }
    Ok(())
}

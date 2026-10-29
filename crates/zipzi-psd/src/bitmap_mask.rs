use ag_psd::{psd::LayerMaskData, PixelData};

use crate::{coordinate, ImportError};

/// Bake a user bitmap mask into straight RGBA alpha without changing RGB,
/// layer bounds, or the separately stored layer opacity. Returns false for a
/// disabled mask, whose pixels and parameters must have no effect on the layer.
pub(super) fn apply(
    image: &mut PixelData,
    mask: &LayerMaskData,
    layer_left: f32,
    layer_top: f32,
    name: &str,
) -> Result<bool, ImportError> {
    if mask.disabled == Some(true) {
        return Ok(false);
    }
    let invalid = |reason| ImportError::new("INVALID_PSD", format!("{name}: {reason}"));
    let density = mask.user_mask_density.unwrap_or(1.0);
    if !density.is_finite() || !(0.0..=1.0).contains(&density) {
        return Err(invalid("invalid bitmap mask density"));
    }
    let feather = mask.user_mask_feather.unwrap_or(0.0);
    if !feather.is_finite() || feather < 0.0 {
        return Err(invalid("invalid bitmap mask feather"));
    }
    if feather != 0.0 {
        return Err(ImportError::new(
            "UNSUPPORTED_LAYER",
            format!("{name}: bitmap mask feather is not supported"),
        ));
    }
    if mask.vector_mask_density.is_some_and(|value| value != 1.0)
        || mask.vector_mask_feather.is_some_and(|value| value != 0.0)
    {
        return Err(ImportError::new(
            "UNSUPPORTED_LAYER",
            format!("{name}: vector mask parameters cannot be represented"),
        ));
    }

    let left = coordinate(mask.left, "mask left")? as i64;
    let top = coordinate(mask.top, "mask top")? as i64;
    let right = coordinate(mask.right, "mask right")? as i64;
    let bottom = coordinate(mask.bottom, "mask bottom")? as i64;
    let width = right - left;
    let height = bottom - top;
    if width < 0 || height < 0 {
        return Err(invalid("invalid bitmap mask bounds"));
    }
    let pixels = mask.image_data.as_ref().or(mask.canvas.as_ref());
    if let Some(pixels) = pixels {
        if i64::from(pixels.width) != width
            || i64::from(pixels.height) != height
            || pixels.data.len() as u64 != pixels.width as u64 * pixels.height as u64 * 4
        {
            return Err(invalid("bitmap mask bounds and pixels differ"));
        }
    } else if width != 0 && height != 0 {
        // An empty rectangle is a valid constant mask. A nonempty rectangle
        // without its channel cannot safely be replaced with the default color.
        return Err(invalid("bitmap mask has no raster pixels"));
    }
    let default_color = mask.default_color.unwrap_or(0.0);
    if !default_color.is_finite()
        || default_color.fract() != 0.0
        || !(0.0..=255.0).contains(&default_color)
    {
        return Err(invalid("invalid bitmap mask default color"));
    }

    let (left, top) = if mask.position_relative_to_layer == Some(true) {
        (left + layer_left as i64, top + layer_top as i64)
    } else {
        (left, top)
    };
    // PSD stores density in one byte. Restore that integer to round only once
    // when computing alpha * (1 - density * (1 - grayscale / 255)).
    let density = (density * 255.0).round() as u32;
    for (index, rgba) in image.data.chunks_exact_mut(4).enumerate() {
        let x = layer_left as i64 + (index % image.width as usize) as i64 - left;
        let y = layer_top as i64 + (index / image.width as usize) as i64 - top;
        let gray = if x >= 0 && x < width && y >= 0 && y < height {
            // The reader expands the grayscale channel into RGB and sets A to
            // 255. Coverage comes from R, never from that synthesized alpha.
            pixels.unwrap().data[((y * width + x) * 4) as usize]
        } else {
            default_color as u8
        };
        let coverage = 255 * 255 - density * (255 - u32::from(gray));
        rgba[3] = ((u32::from(rgba[3]) * coverage + 32_512) / 65_025) as u8;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask() -> LayerMaskData {
        LayerMaskData {
            left: Some(0.0),
            top: Some(0.0),
            right: Some(1.0),
            bottom: Some(1.0),
            image_data: Some(PixelData {
                width: 1,
                height: 1,
                data: vec![128, 128, 128, 255],
            }),
            ..Default::default()
        }
    }

    fn rejects(mask: LayerMaskData, reason: &str) {
        let mut image = PixelData {
            width: 1,
            height: 1,
            data: vec![80, 120, 200, 128],
        };
        let before = image.data.clone();
        let error = apply(&mut image, &mask, 0.0, 0.0, "masked").unwrap_err();
        assert_eq!(error.code, "INVALID_PSD");
        assert!(error.message.contains(reason), "{error}");
        assert_eq!(image.data, before);
    }

    // These malformed decoded records cannot be built through the PSD writer,
    // which derives bounds from pixels and clamps byte-valued parameters.
    #[test]
    fn invalid_mask_pixels_and_bounds_fail_before_modifying_alpha() {
        let mut missing = mask();
        missing.image_data = None;
        rejects(missing, "no raster pixels");
        let mut short = mask();
        short.image_data.as_mut().unwrap().data.pop();
        rejects(short, "bounds and pixels differ");
        let mut mismatched = mask();
        mismatched.right = Some(2.0);
        rejects(mismatched, "bounds and pixels differ");
        let mut inverted = mask();
        inverted.right = Some(-1.0);
        rejects(inverted, "invalid bitmap mask bounds");
    }

    #[test]
    fn invalid_mask_parameters_fail_before_modifying_alpha() {
        for density in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
            let mut bitmap = mask();
            bitmap.user_mask_density = Some(density);
            rejects(bitmap, "invalid bitmap mask density");
        }
        for feather in [f64::NAN, f64::INFINITY, -1.0] {
            let mut bitmap = mask();
            bitmap.user_mask_feather = Some(feather);
            rejects(bitmap, "invalid bitmap mask feather");
        }
        for color in [f64::NAN, -1.0, 256.0, 0.5] {
            let mut bitmap = mask();
            bitmap.default_color = Some(color);
            rejects(bitmap, "invalid bitmap mask default color");
        }
    }
}

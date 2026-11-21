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
    let bitmap = Bitmap::read(mask, layer_left, layer_top, name)?;
    for (index, rgba) in image.data.chunks_exact_mut(4).enumerate() {
        let x = layer_left as i64 + (index % image.width as usize) as i64;
        let y = layer_top as i64 + (index / image.width as usize) as i64;
        rgba[3] = bitmap.alpha(bitmap.gray(x, y), rgba[3]);
    }
    Ok(true)
}

pub(super) struct Bitmap<'a> {
    left: i64,
    top: i64,
    width: i64,
    height: i64,
    pixels: Option<&'a PixelData>,
    default: u8,
    density: u32,
}

impl<'a> Bitmap<'a> {
    pub(super) fn read(
        mask: &'a LayerMaskData,
        layer_left: f32,
        layer_top: f32,
        name: &str,
    ) -> Result<Self, ImportError> {
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
        Ok(Self {
            left,
            top,
            width,
            height,
            pixels,
            default: default_color as u8,
            density,
        })
    }
    fn gray(&self, x: i64, y: i64) -> u8 {
        let (x, y) = (x - self.left, y - self.top);
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            self.pixels.unwrap().data[((y * self.width + x) * 4) as usize]
        } else {
            self.default
        }
    }
    fn alpha(&self, gray: u8, alpha: u8) -> u8 {
        let coverage = 65_025 - self.density * (255 - u32::from(gray));
        ((u32::from(alpha) * coverage + 32_512) / 65_025) as u8
    }
}

impl crate::Builder {
    /// The renderer uses raw source alpha for clipping even when its color draw
    /// has zero opacity. An inverted white-background mask thus extends to infinity.
    pub(super) fn create_bitmap_mask(
        &mut self,
        bitmap: &Bitmap<'_>,
        name: &str,
        part: &str,
        layer: [f32; 4],
    ) -> Result<(String, bool), ImportError> {
        use zipzi_core::types::{Appearance, Mesh, Vec2};
        let outside = bitmap.alpha(bitmap.default, 255);
        let inverted = outside == 255;
        let width = bitmap.width.max(1) as u32;
        let height = bitmap.height.max(1) as u32;
        // A one-texel ring holds the PSD default gray, also used by the four
        // outer faces when density gives the outside a fractional alpha.
        let texture_width = width + 2;
        let texture_height = height + 2;
        if u64::from(texture_width) * u64::from(texture_height) > crate::MAX_PIXELS {
            return Err(ImportError::new(
                "PSD_LIMIT",
                format!("{name}: editable bitmap mask exceeds texture limit"),
            ));
        }
        let mut data = Vec::with_capacity(texture_width as usize * texture_height as usize * 4);
        for row in 0..texture_height {
            for col in 0..texture_width {
                let gray = bitmap.gray(
                    bitmap.left + i64::from(col) - 1,
                    bitmap.top + i64::from(row) - 1,
                );
                let alpha = bitmap.alpha(gray, 255);
                data.extend_from_slice(&[
                    gray,
                    gray,
                    gray,
                    if inverted { 255 - alpha } else { alpha },
                ]);
            }
        }
        let label = format!("{name} (Bitmap Mask)");
        let asset = self.store_image(
            &label,
            PixelData {
                width: texture_width,
                height: texture_height,
                data,
            },
        )?;
        let id = self.id("mask");
        let mut mesh = Mesh {
            id: id.clone(),
            runtime_id: self.mesh_runtime_id(&label, &id),
            name: label,
            part_id: part.into(),
            texture_asset_id: asset.into(),
            texture_repeat: Some(false),
            appearance: Appearance {
                opacity: 0.,
                ..Default::default()
            },
            ..Default::default()
        };
        let origin = self.document.setup().source_canvas.unwrap().origin;
        let left = bitmap.left as f32;
        let top = bitmap.top as f32;
        let right = left + width as f32;
        let bottom = top + height as f32;
        let mut quad = |l: f32, t: f32, r: f32, b: f32, uv: [f32; 4]| {
            if l >= r || t >= b {
                return;
            }
            let first = mesh.vertex_ids.len() as u32 + 1;
            mesh.vertex_ids.extend(first..first + 4);
            for (x, y) in [(l, b), (r, b), (r, t), (l, t)] {
                mesh.base_positions
                    .push(Vec2::new(x - origin.x, origin.y - y));
            }
            let [u0, v0, u1, v1] = uv;
            mesh.uvs.extend([
                Vec2::new(u0, v0),
                Vec2::new(u1, v0),
                Vec2::new(u1, v1),
                Vec2::new(u0, v1),
            ]);
            mesh.triangles
                .extend([[first, first + 1, first + 2], [first, first + 2, first + 3]]);
        };
        let u = 1. / texture_width as f32;
        let v = 1. / texture_height as f32;
        quad(left, top, right, bottom, [u, v, 1. - u, 1. - v]);
        if outside != 0 && !inverted {
            let l = layer[0].min(left).min(0.);
            let t = layer[1].min(top).min(0.);
            let r = layer[2].max(right).max(self.report.width as f32);
            let b = layer[3].max(bottom).max(self.report.height as f32);
            let constant = [u * 0.5, v * 0.5, u * 0.5, v * 0.5];
            quad(l, t, r, top, constant);
            quad(l, bottom, r, b, constant);
            quad(l, top, left, bottom, constant);
            quad(right, top, r, bottom, constant);
        }
        crate::check(self.document.create_mesh(mesh))?;
        Ok((id, inverted))
    }
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

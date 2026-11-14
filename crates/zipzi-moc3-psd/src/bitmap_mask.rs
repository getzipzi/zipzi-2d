//! Freeze raw ArtMesh clipping into editable PSD user-mask pixels.
use crate::{raster, Error, Layer};
use std::collections::{HashMap, HashSet};
use zipzi_core::{
    evaluation::{Drawable, DrawableFrame},
    image::DecodedImage,
    Canvas,
};

pub(crate) struct MaskSources<'a> {
    drawables: HashMap<&'a str, &'a Drawable>,
    textures: &'a HashMap<String, DecodedImage>,
    cached: HashMap<String, Option<Layer>>,
    canvas: Canvas,
    width: u32,
    height: u32,
}

impl<'a> MaskSources<'a> {
    pub fn new(
        frame: &'a DrawableFrame,
        textures: &'a HashMap<String, DecodedImage>,
        canvas: Canvas,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            drawables: frame.drawables.iter().map(|d| (d.id.as_str(), d)).collect(),
            textures,
            cached: HashMap::new(),
            canvas,
            width,
            height,
        }
    }

    pub fn for_layer(
        &mut self,
        drawable: &Drawable,
        layer: &Layer,
    ) -> Result<Option<Vec<u8>>, Error> {
        if drawable.masks.is_empty() {
            return Ok(None);
        }
        let mut coverage = vec![0u8; layer.width as usize * layer.height as usize];
        let mut seen = HashSet::new();
        for id in &drawable.masks {
            if !seen.insert(id) {
                continue;
            }
            if !self.cached.contains_key(id) {
                let source = self
                    .drawables
                    .get(id.as_str())
                    .ok_or_else(|| Error::Model(format!("missing mask source {id}")))?;
                let texture = self.textures.get(&source.texture_asset_id).ok_or_else(|| {
                    Error::Model(format!("missing mask texture {}", source.texture_asset_id))
                })?;
                self.cached.insert(
                    id.clone(),
                    raster::rasterize_mask(source, texture, self.canvas, self.width, self.height),
                );
            }
            let Some(source) = self.cached[id].as_ref() else {
                continue;
            };
            let left = layer.left.max(source.left);
            let top = layer.top.max(source.top);
            let right = (layer.left + layer.width).min(source.left + source.width);
            let bottom = (layer.top + layer.height).min(source.top + source.height);
            for y in top..bottom {
                for x in left..right {
                    let dst = ((y - layer.top) * layer.width + x - layer.left) as usize;
                    let src =
                        (((y - source.top) * source.width + x - source.left) * 4 + 3) as usize;
                    let alpha = u32::from(source.rgba[src]);
                    coverage[dst] =
                        (alpha + (u32::from(coverage[dst]) * (255 - alpha) + 127) / 255) as u8;
                }
            }
        }
        if drawable.inverted_mask {
            coverage.iter_mut().for_each(|alpha| *alpha = 255 - *alpha);
        }
        Ok(Some(coverage))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ag_psd::{psd::ReadOptions, read_psd};
    use zipzi_core::Vec2;

    fn quad(id: &str, x: f32, size: f32) -> Drawable {
        Drawable {
            id: id.into(),
            texture_asset_id: "texture".into(),
            positions: vec![
                Vec2::new(x, 0.),
                Vec2::new(x + size, 0.),
                Vec2::new(x + size, -size),
                Vec2::new(x, -size),
            ],
            uvs: vec![Vec2::new(0., 0.); 4].into(),
            indices: vec![0, 1, 2, 0, 2, 3].into(),
            ..Default::default()
        }
    }
    fn textures() -> HashMap<String, DecodedImage> {
        HashMap::from([(
            "texture".into(),
            DecodedImage {
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 128],
            },
        )])
    }
    fn canvas() -> Canvas {
        Canvas::new(4., 4., Vec2::new(0., 0.), 1.)
    }

    #[test]
    fn multiple_raw_sources_union_once_and_ignore_appearance_and_source_masks() {
        let mut first = quad("first", 0., 4.);
        first.opacity = 0.;
        first.visible = false;
        first.enabled = false;
        first.multiply_color = [0.; 4];
        first.masks = vec!["missing-nested-source".into()];
        let second = quad("second", 0., 4.);
        let mut target = quad("target", 0., 4.);
        target.masks = vec!["first".into(), "second".into(), "first".into()];
        let frame = DrawableFrame {
            drawables: vec![first, second, target.clone()],
            ..Default::default()
        };
        let textures = textures();
        let mut sources = MaskSources::new(&frame, &textures, canvas(), 4, 4);
        let layer = raster::rasterize(&target, &textures["texture"], canvas(), 4, 4).unwrap();
        assert_eq!(
            sources.for_layer(&target, &layer).unwrap().unwrap(),
            vec![192; 16]
        );
        target.inverted_mask = true;
        assert_eq!(
            sources.for_layer(&target, &layer).unwrap().unwrap(),
            vec![63; 16]
        );
        // Reversed winding must cover the shared diagonal exactly once too.
        let mut source = quad("reversed", 0., 4.);
        source.indices = vec![2, 1, 0, 3, 2, 0].into();
        let raw = raster::rasterize_mask(&source, &textures["texture"], canvas(), 4, 4).unwrap();
        assert!(raw.rgba.chunks_exact(4).all(|p| p[3] == 128));
    }

    #[test]
    fn cropped_mask_alignment_inversion_and_empty_sources() {
        let source = quad("source", 2., 2.);
        let mut target = quad("target", 1., 3.);
        target.masks = vec!["source".into()];
        let frame = DrawableFrame {
            drawables: vec![source, target.clone(), quad("outside", 20., 2.)],
            ..Default::default()
        };
        let textures = textures();
        let mut sources = MaskSources::new(&frame, &textures, canvas(), 4, 4);
        let layer = raster::rasterize(&target, &textures["texture"], canvas(), 4, 4).unwrap();
        assert_eq!(
            sources.for_layer(&target, &layer).unwrap().unwrap(),
            vec![0, 128, 128, 0, 128, 128, 0, 0, 0]
        );
        target.inverted_mask = true;
        assert_eq!(
            sources.for_layer(&target, &layer).unwrap().unwrap(),
            vec![255, 127, 127, 255, 127, 127, 255, 255, 255]
        );
        target.masks = vec!["outside".into()];
        assert_eq!(
            sources.for_layer(&target, &layer).unwrap().unwrap(),
            vec![255; 9]
        );
        target.inverted_mask = false;
        assert_eq!(
            sources.for_layer(&target, &layer).unwrap().unwrap(),
            vec![0; 9]
        );
        target.masks = vec!["missing".into()];
        assert!(sources.for_layer(&target, &layer).is_err());
    }

    #[test]
    fn psd_keeps_editable_mask_and_original_pixels_and_masks_merged_preview() {
        let textures = textures();
        let target = quad("target", 1., 2.);
        let mut layer = raster::rasterize(&target, &textures["texture"], canvas(), 4, 4).unwrap();
        layer.mask = Some(vec![0, 128, 255, 64]);
        let bytes = crate::psd::encode(4, 4, vec![layer]).unwrap();
        let parsed = read_psd(
            &bytes,
            &ReadOptions {
                use_image_data: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        let layer = &parsed.children.as_ref().unwrap()[0];
        assert!(layer
            .image_data
            .as_ref()
            .unwrap()
            .data
            .chunks_exact(4)
            .all(|p| p[3] == 128));
        let mask = layer.additional_info.mask.as_ref().unwrap();
        assert_eq!(
            (mask.left, mask.top, mask.right, mask.bottom),
            (Some(1.), Some(0.), Some(3.), Some(2.))
        );
        assert_ne!(mask.disabled, Some(true));
        assert_eq!(
            mask.image_data
                .as_ref()
                .unwrap()
                .data
                .chunks_exact(4)
                .map(|p| p[0])
                .collect::<Vec<_>>(),
            vec![0, 128, 255, 64]
        );
        let composite = &parsed.image_data.as_ref().unwrap().data;
        let alpha = |x: usize, y: usize| composite[(y * 4 + x) * 4 + 3];
        assert_eq!(
            [alpha(1, 0), alpha(2, 0), alpha(1, 1), alpha(2, 1)],
            [0, 64, 128, 32]
        );
    }
}

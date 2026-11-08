//! Immutable cursor artwork, registered once and requested by a small identifier.
use crate::{CursorStyle, Point, Size};
use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

/// A process-local resource identifier. Serialized IDs are not portable between runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct CustomCursorId(u64);

/// One resolution of a cursor, in top-down, premultiplied RGBA8 pixels.
#[derive(Clone, Debug)]
pub struct CustomCursorImage {
    size: Size<u32>,
    rgba: Arc<[u8]>,
}

impl CustomCursorImage {
    /// Creates an image. Each dimension must be in 1..=512.
    pub fn new(size: Size<u32>, rgba: impl Into<Arc<[u8]>>) -> Result<Self> {
        ensure!(
            (1..=512).contains(&size.width) && (1..=512).contains(&size.height),
            "invalid cursor image size"
        );
        let rgba = rgba.into();
        ensure!(
            rgba.len() == size.width as usize * size.height as usize * 4,
            "invalid cursor RGBA length"
        );
        ensure!(
            rgba.chunks_exact(4)
                .all(|p| p[..3].iter().all(|c| *c <= p[3])),
            "cursor pixels must be premultiplied"
        );
        Ok(Self { size, rgba })
    }

    /// The bitmap dimensions in physical pixels.
    pub fn size(&self) -> Size<u32> {
        self.size
    }

    /// Top-down, premultiplied RGBA8 bytes.
    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

/// Cursor artwork with a logical size, a stable hotspot and a standard fallback.
///
/// Register during application initialization. Platforms retain registered resources
/// for their lifetime; do not register per frame. Native caches may also be held by
/// windows until those windows are dropped.
#[derive(Clone, Debug)]
pub struct CustomCursor {
    id: CustomCursorId,
    size: Size<u32>,
    hotspot: Point<u32>,
    images: Vec<CustomCursorImage>,
    fallback: CursorStyle,
}

impl CustomCursor {
    /// Creates immutable artwork. All representations must have the logical aspect ratio.
    pub fn new(
        size: Size<u32>,
        hotspot: Point<u32>,
        mut images: Vec<CustomCursorImage>,
        fallback: CursorStyle,
    ) -> Result<Self> {
        ensure!(
            (1..=128).contains(&size.width) && (1..=128).contains(&size.height),
            "invalid logical cursor size"
        );
        ensure!(
            hotspot.x < size.width && hotspot.y < size.height,
            "cursor hotspot outside image"
        );
        ensure!(
            !matches!(fallback, CursorStyle::Custom(_)),
            "cursor fallback must be a standard style"
        );
        ensure!(
            !images.is_empty() && images.len() <= 8,
            "cursor requires 1..=8 representations"
        );
        ensure!(
            images
                .iter()
                .all(|image| image.size.width * size.height == image.size.height * size.width),
            "cursor representation aspect ratio differs"
        );
        images.sort_by_key(|image| image.size.width);
        images.dedup_by_key(|image| image.size.width);
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Ok(Self {
            id: CustomCursorId(NEXT_ID.fetch_add(1, Ordering::Relaxed)),
            size,
            hotspot,
            images,
            fallback,
        })
    }

    /// Rasterizes a path-only SVG at 1x, 2x, 3x and 4x before registration.
    pub fn from_svg(
        svg: &[u8],
        size: Size<u32>,
        hotspot: Point<u32>,
        fallback: CursorStyle,
    ) -> Result<Self> {
        ensure!(
            (1..=128).contains(&size.width) && (1..=128).contains(&size.height),
            "invalid logical cursor size"
        );
        let tree = usvg::Tree::from_data(svg, &usvg::Options::default())?;
        let mut images = Vec::new();
        for scale in 1..=4 {
            let width = size.width * scale;
            let height = size.height * scale;
            let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
                .ok_or_else(|| anyhow::anyhow!("allocating cursor bitmap"))?;
            let transform = resvg::tiny_skia::Transform::from_scale(
                width as f32 / tree.size().width(),
                height as f32 / tree.size().height(),
            );
            resvg::render(&tree, transform, &mut pixmap.as_mut());
            images.push(CustomCursorImage::new(
                crate::size(width, height),
                pixmap.take(),
            )?);
        }
        Self::new(size, hotspot, images, fallback)
    }

    /// The resource identifier, preserved by cloning this artwork.
    pub fn id(&self) -> CustomCursorId {
        self.id
    }
    /// The logical dimensions, independent of document zoom.
    pub fn size(&self) -> Size<u32> {
        self.size
    }
    /// The click location, measured from the top-left in logical pixels.
    pub fn hotspot(&self) -> Point<u32> {
        self.hotspot
    }
    /// The standard cursor to use when registration or native creation fails.
    pub fn fallback(&self) -> CursorStyle {
        self.fallback
    }
    /// All registered bitmap resolutions.
    pub fn images(&self) -> &[CustomCursorImage] {
        &self.images
    }

    /// Selects or resamples artwork and scales the hotspot for a physical DPI.
    /// Output dimensions are bounded to 512 pixels, even for unusual OS scaling.
    pub fn image_for_scale(&self, scale: f32) -> (CustomCursorImage, Point<u32>) {
        let scale = if scale.is_finite() {
            scale.clamp(0.5, 4.)
        } else {
            1.
        };
        let width = (self.size.width as f32 * scale).round().max(1.) as u32;
        let height = (self.size.height as f32 * scale).round().max(1.) as u32;
        let source = self
            .images
            .iter()
            .find(|image| image.size.width >= width)
            .unwrap_or_else(|| self.images.last().unwrap());
        let image = if source.size == crate::size(width, height) {
            source.clone()
        } else {
            let source = image::RgbaImage::from_raw(
                source.size.width,
                source.size.height,
                source.rgba.to_vec(),
            )
            .unwrap();
            let resized = image::imageops::resize(
                &source,
                width,
                height,
                image::imageops::FilterType::Triangle,
            );
            CustomCursorImage {
                size: crate::size(width, height),
                rgba: resized.into_raw().into(),
            }
        };
        let hotspot = crate::point(
            ((self.hotspot.x as f32 * width as f32 / self.size.width as f32).round() as u32)
                .min(width - 1),
            ((self.hotspot.y as f32 * height as f32 / self.size.height as f32).round() as u32)
                .min(height - 1),
        );
        (image, hotspot)
    }
}

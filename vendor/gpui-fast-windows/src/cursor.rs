use gpui::{CursorStyle, CustomCursor, CustomCursorId};
use std::{
    collections::{HashMap, VecDeque},
    rc::Rc,
};
use windows::Win32::{Foundation::*, Graphics::Gdi::*, UI::WindowsAndMessaging::*};

/// Only handles created here are destroyed. LoadImage(..., LR_SHARED) handles
/// remain owned by the OS and never enter this wrapper.
pub(crate) struct OwnedCursor(HCURSOR);
impl Drop for OwnedCursor {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyCursor(self.0);
        }
    }
}

pub(crate) struct ResolvedCursor {
    pub(crate) handle: Option<HCURSOR>,
    // Windows keep their selected resource alive even after cache eviction.
    _owned: Option<Rc<OwnedCursor>>,
}
impl ResolvedCursor {
    fn system(style: CursorStyle) -> Self {
        Self {
            handle: crate::load_cursor(style),
            _owned: None,
        }
    }
}

struct Resource {
    artwork: CustomCursor,
    // Both failures and successes are cached. No allocation on repeated requests.
    variants: VecDeque<(gpui::Size<u32>, Option<Rc<OwnedCursor>>)>,
}

pub(crate) struct CursorCache {
    resources: HashMap<CustomCursorId, Resource>,
    open_hand: Option<CustomCursorId>,
    closed_hand: Option<CustomCursorId>,
}

impl CursorCache {
    pub(crate) fn new() -> Self {
        let mut cache = Self {
            resources: HashMap::new(),
            open_hand: None,
            closed_hand: None,
        };
        cache.open_hand = cache.hand(
            include_bytes!("../resources/cursors/open-hand.svg"),
            gpui::point(10, 10),
        );
        cache.closed_hand = cache.hand(
            include_bytes!("../resources/cursors/closed-hand.svg"),
            gpui::point(10, 10),
        );
        cache
    }

    fn hand(&mut self, svg: &[u8], hotspot: gpui::Point<u32>) -> Option<CustomCursorId> {
        let cursor =
            CustomCursor::from_svg(svg, gpui::size(24, 24), hotspot, CursorStyle::Arrow).ok()?;
        match self.register(cursor) {
            CursorStyle::Custom(id) => Some(id),
            _ => None,
        }
    }

    pub(crate) fn register(&mut self, artwork: CustomCursor) -> CursorStyle {
        let id = artwork.id();
        let fallback = artwork.fallback();
        let size = artwork.size();
        self.resources.entry(id).or_insert_with(|| Resource {
            artwork,
            variants: VecDeque::new(),
        });
        // Test native creation now so callers get a usable standard style on failure.
        self.resolve(CursorStyle::Custom(id), 1.);
        if self.resources[&id]
            .variants
            .iter()
            .any(|(key, cursor)| *key == size && cursor.is_some())
        {
            CursorStyle::Custom(id)
        } else {
            fallback
        }
    }

    pub(crate) fn resolve(&mut self, style: CursorStyle, scale: f32) -> ResolvedCursor {
        let id = match style {
            CursorStyle::Custom(id) => Some(id),
            CursorStyle::OpenHand => self.open_hand,
            CursorStyle::ClosedHand => self.closed_hand,
            _ => None,
        };
        let Some(resource) = id.and_then(|id| self.resources.get_mut(&id)) else {
            return ResolvedCursor::system(style);
        };
        // Compute the cache key without rasterizing or allocating an image.
        let scale = if scale.is_finite() {
            scale.clamp(0.5, 4.)
        } else {
            1.
        };
        let size = gpui::size(
            (resource.artwork.size().width as f32 * scale)
                .round()
                .max(1.) as u32,
            (resource.artwork.size().height as f32 * scale)
                .round()
                .max(1.) as u32,
        );
        let native = if let Some(index) = resource.variants.iter().position(|(key, _)| *key == size)
        {
            let variant = resource.variants.remove(index).unwrap();
            let native = variant.1.clone();
            resource.variants.push_back(variant);
            native
        } else {
            let (image, hotspot) = resource.artwork.image_for_scale(scale);
            let native = create(&image, hotspot).map(Rc::new);
            // At most 16 DPI variants per registered resource. Window-held Rc's
            // keep evicted, displayed cursors alive until their replacement is set.
            if resource.variants.len() == 16 {
                resource.variants.pop_front();
            }
            resource.variants.push_back((size, native.clone()));
            native
        };
        match native {
            Some(native) => ResolvedCursor {
                handle: Some(native.0),
                _owned: Some(native),
            },
            None => {
                let fallback = resource.artwork.fallback();
                self.resolve(fallback, scale)
            }
        }
    }
}

struct Bitmap(HBITMAP);
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0.into());
        }
    }
}

fn create(image: &gpui::CustomCursorImage, hotspot: gpui::Point<u32>) -> Option<OwnedCursor> {
    let size = image.size();
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size.width as i32,
            biHeight: -(size.height as i32), // top-down DIB
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = std::ptr::null_mut();
    // The OS owns DIB storage until DeleteObject; CreateIconIndirect copies it.
    let color =
        Bitmap(unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }.ok()?);
    if bits.is_null() {
        return None;
    }
    let destination =
        unsafe { std::slice::from_raw_parts_mut(bits as *mut u8, image.rgba().len()) };
    for (source, pixel) in image
        .rgba()
        .chunks_exact(4)
        .zip(destination.chunks_exact_mut(4))
    {
        pixel.copy_from_slice(&[source[2], source[1], source[0], source[3]]);
    }
    // The monochrome AND mask uses word-aligned rows. Transparent pixels are
    // masked out for systems which fall back from the alpha color bitmap.
    let stride = size.width.div_ceil(16) as usize * 2;
    let mut mask_bytes = vec![0u8; stride * size.height as usize];
    for y in 0..size.height as usize {
        for x in 0..size.width as usize {
            if image.rgba()[(y * size.width as usize + x) * 4 + 3] == 0 {
                mask_bytes[y * stride + x / 8] |= 0x80 >> (x % 8);
            }
        }
    }
    let mask = Bitmap(unsafe {
        CreateBitmap(
            size.width as i32,
            size.height as i32,
            1,
            1,
            Some(mask_bytes.as_ptr().cast()),
        )
    });
    if mask.0.is_invalid() {
        return None;
    }
    let info = ICONINFO {
        fIcon: FALSE,
        xHotspot: hotspot.x,
        yHotspot: hotspot.y,
        hbmMask: mask.0,
        hbmColor: color.0,
    };
    unsafe { CreateIconIndirect(&info) }
        .ok()
        .map(|icon| OwnedCursor(HCURSOR(icon.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artwork() -> CustomCursor {
        CustomCursor::from_svg(
            include_bytes!("../resources/cursors/open-hand.svg"),
            gpui::size(24, 24),
            gpui::point(10, 10),
            CursorStyle::Crosshair,
        )
        .unwrap()
    }

    fn native_info(cursor: &ResolvedCursor) -> (gpui::Size<u32>, gpui::Point<u32>) {
        let mut info = ICONINFO::default();
        unsafe { GetIconInfo(HICON(cursor.handle.unwrap().0), &mut info) }.unwrap();
        let color = Bitmap(info.hbmColor);
        let _mask = Bitmap(info.hbmMask);
        let mut bitmap = BITMAP::default();
        let bytes = unsafe {
            GetObjectW(
                color.0.into(),
                std::mem::size_of::<BITMAP>() as i32,
                Some((&mut bitmap as *mut BITMAP).cast()),
            )
        };
        assert_eq!(bytes, std::mem::size_of::<BITMAP>() as i32);
        assert!(!info.fIcon.as_bool());
        (
            gpui::size(bitmap.bmWidth as u32, bitmap.bmHeight.unsigned_abs()),
            gpui::point(info.xHotspot, info.yHotspot),
        )
    }

    #[test]
    fn native_hotspot_and_size_follow_fractional_dpi() {
        let mut cache = CursorCache::new();
        let cursor = cache.register(artwork());
        assert!(matches!(cursor, CursorStyle::Custom(_)));
        for (scale, pixels, hotspot) in [(1., 24, 10), (1.25, 30, 13), (1.5, 36, 15), (2., 48, 20)]
        {
            assert_eq!(
                native_info(&cache.resolve(cursor, scale)),
                (gpui::size(pixels, pixels), gpui::point(hotspot, hotspot))
            );
        }
    }

    #[test]
    fn cache_reuses_handles_and_preserves_selected_handles_after_eviction() {
        let mut cache = CursorCache::new();
        let artwork = artwork();
        let id = artwork.id();
        let style = cache.register(artwork);
        let first = cache.resolve(style, 1.);
        let again = cache.resolve(style, 1.);
        assert!(Rc::ptr_eq(
            first._owned.as_ref().unwrap(),
            again._owned.as_ref().unwrap()
        ));
        for step in 1..=48 {
            cache.resolve(style, 1. + step as f32 / 24.);
        }
        assert_eq!(cache.resources[&id].variants.len(), 16);
        assert!(
            !cache.resources[&id]
                .variants
                .iter()
                .any(|(size, _)| *size == gpui::size(24, 24))
        );
        assert_eq!(
            native_info(&first),
            (gpui::size(24, 24), gpui::point(10, 10))
        );
    }

    #[test]
    fn standard_hands_resolve_to_distinct_owned_cursors() {
        let mut cache = CursorCache::new();
        let open = cache.resolve(CursorStyle::OpenHand, 1.);
        let closed = cache.resolve(CursorStyle::ClosedHand, 1.);
        assert!(open._owned.is_some() && closed._owned.is_some());
        assert_ne!(open.handle.unwrap().0, closed.handle.unwrap().0);
        let unregistered = artwork();
        let unknown = cache.resolve(CursorStyle::Custom(unregistered.id()), 1.);
        assert!(unknown._owned.is_none());
        assert_eq!(
            unknown.handle.unwrap().0,
            crate::load_cursor(CursorStyle::Arrow).unwrap().0
        );
    }

    #[test]
    fn failed_custom_creation_can_fall_back_to_a_native_standard_hand() {
        let mut cache = CursorCache::new();
        let artwork = CustomCursor::from_svg(
            include_bytes!("../resources/cursors/open-hand.svg"),
            gpui::size(24, 24),
            gpui::point(10, 10),
            CursorStyle::ClosedHand,
        )
        .unwrap();
        let id = artwork.id();
        let style = cache.register(artwork.clone());
        // Model a cached native allocation failure without exhausting OS resources.
        cache.resources.get_mut(&id).unwrap().variants[0].1 = None;
        let fallback = cache.resolve(style, 1.);
        let hand = cache.resolve(CursorStyle::ClosedHand, 1.);
        assert!(fallback._owned.is_some());
        assert_eq!(fallback.handle.unwrap().0, hand.handle.unwrap().0);
        assert_eq!(cache.register(artwork), CursorStyle::ClosedHand);
    }
}

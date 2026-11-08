use gpui::{CursorStyle, CustomCursor, CustomCursorId};
use objc2::{AnyThread, MainThreadMarker, rc::Retained};
use objc2_app_kit::{NSBitmapImageRep, NSCursor, NSDeviceRGBColorSpace, NSImage};
use objc2_foundation::{NSPoint, NSSize};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

/// Shared by the platform and its windows; all access stays on the AppKit thread.
pub(crate) type CustomCursors = Rc<RefCell<HashMap<CustomCursorId, Retained<NSCursor>>>>;

pub(crate) fn register(
    cursors: &CustomCursors,
    cursor: CustomCursor,
    marker: MainThreadMarker,
) -> CursorStyle {
    if cursors.borrow().contains_key(&cursor.id()) {
        return CursorStyle::Custom(cursor.id());
    }
    match create(&cursor, marker) {
        Some(native) => {
            cursors.borrow_mut().insert(cursor.id(), native);
            CursorStyle::Custom(cursor.id())
        }
        None => cursor.fallback(),
    }
}

fn create(cursor: &CustomCursor, _marker: MainThreadMarker) -> Option<Retained<NSCursor>> {
    let logical_size = NSSize::new(cursor.size().width as f64, cursor.size().height as f64);
    let image = NSImage::initWithSize(NSImage::alloc(), logical_size);
    for bitmap in cursor.images() {
        // A null planes pointer asks AppKit to own the backing storage. The default
        // bitmap format is premultiplied RGBA, matching CustomCursorImage.
        let representation = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(), std::ptr::null_mut(),
                bitmap.size().width as isize, bitmap.size().height as isize,
                8, 4, true, false, NSDeviceRGBColorSpace,
                bitmap.size().width as isize * 4, 32,
            )
        }?;
        let destination = representation.bitmapData();
        if destination.is_null() {
            return None;
        }
        // AppKit allocated height * bytesPerRow bytes; the validated source has
        // exactly that length and does not overlap the new representation.
        unsafe {
            std::ptr::copy_nonoverlapping(bitmap.rgba().as_ptr(), destination, bitmap.rgba().len());
        }
        representation.setSize(logical_size);
        image.addRepresentation(&representation);
    }
    Some(NSCursor::initWithImage_hotSpot(
        NSCursor::alloc(),
        &image,
        NSPoint::new(cursor.hotspot().x as f64, cursor.hotspot().y as f64),
    ))
}

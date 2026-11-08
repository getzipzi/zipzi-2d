//! Cursor contracts live here so they use the workspace lockfile and normal GPUI features.
use gpui_kit::{CursorStyle, CustomCursor, CustomCursorImage, point, size};
#[path = "support/cursor_artwork.rs"]
mod cursor_artwork;

#[test]
fn first_batch_cursor_artwork_shares_a_stable_hotspot_at_every_dpi() {
    for (name, svg) in cursor_artwork::ARTWORK {
        let cursor =
            CustomCursor::from_svg(svg, size(24, 24), point(7, 7), CursorStyle::Crosshair).unwrap();
        for scale in [1., 1.25, 1.5, 2.] {
            let (image, hotspot) = cursor.image_for_scale(scale);
            assert_eq!(
                hotspot,
                point((7. * scale).round() as u32, (7. * scale).round() as u32),
                "{name}"
            );
            assert!(
                image.rgba().chunks_exact(4).any(|pixel| pixel[3] > 0),
                "{name}"
            );
        }
    }
}

fn mesh_add() -> CustomCursor {
    CustomCursor::from_svg(
        include_bytes!("../assets/cursors/mesh-add.svg"),
        size(24, 24),
        point(7, 7),
        CursorStyle::Crosshair,
    )
    .unwrap()
}

#[test]
fn cursor_hotspot_tracks_fractional_dpi_without_changing_logical_size() {
    let cursor = mesh_add();
    for (scale, pixels, hotspot) in [
        (1., 24, 7),
        (1.25, 30, 9),
        (1.5, 36, 11),
        (2., 48, 14),
        (4., 96, 28),
    ] {
        let (image, actual) = cursor.image_for_scale(scale);
        assert_eq!(image.size(), size(pixels, pixels));
        assert_eq!(actual, point(hotspot, hotspot));
        assert_eq!(image.rgba().len(), (pixels * pixels * 4) as usize);
        assert!(image.rgba().chunks_exact(4).any(|pixel| pixel[3] > 0));
        assert!(image.rgba().chunks_exact(4).any(|pixel| pixel[3] == 0));
    }
    assert_eq!(cursor.size(), size(24, 24));
    assert_eq!(cursor.hotspot(), point(7, 7));
}

#[test]
fn cursor_limits_bad_scale_and_keeps_edge_hotspots_inside_bitmap() {
    let image = CustomCursorImage::new(size(24, 24), vec![0; 24 * 24 * 4]).unwrap();
    let cursor =
        CustomCursor::new(size(24, 24), point(23, 23), vec![image], CursorStyle::Arrow).unwrap();
    for scale in [f32::NAN, f32::INFINITY, -1., 0., 0.5, 1.25, 1000.] {
        let (image, hotspot) = cursor.image_for_scale(scale);
        assert!(image.size().width <= 96 && image.size().height <= 96);
        assert!(hotspot.x < image.size().width && hotspot.y < image.size().height);
    }
}

#[test]
fn registration_ids_are_shared_by_clones_and_distinct_between_resources() {
    let first = mesh_add();
    assert_eq!(first.id(), first.clone().id());
    assert_ne!(first.id(), mesh_add().id());
    let style = CursorStyle::Custom(first.id());
    assert_eq!(
        serde_json::from_str::<CursorStyle>(&serde_json::to_string(&style).unwrap()).unwrap(),
        style
    );
}

#[test]
fn malformed_cursor_data_is_rejected_before_native_registration() {
    assert!(CustomCursorImage::new(size(0, 24), vec![]).is_err());
    assert!(CustomCursorImage::new(size(513, 1), vec![0; 513 * 4]).is_err());
    assert!(CustomCursorImage::new(size(1, 1), vec![0; 3]).is_err());
    assert!(CustomCursorImage::new(size(1, 1), vec![255, 0, 0, 0]).is_err());
    let image = || CustomCursorImage::new(size(24, 24), vec![0; 24 * 24 * 4]).unwrap();
    assert!(
        CustomCursor::new(
            size(24, 24),
            point(24, 7),
            vec![image()],
            CursorStyle::Arrow
        )
        .is_err()
    );
    assert!(CustomCursor::new(size(24, 24), point(7, 7), vec![], CursorStyle::Arrow).is_err());
    assert!(
        CustomCursor::new(size(24, 12), point(7, 7), vec![image()], CursorStyle::Arrow).is_err()
    );
    assert!(
        CustomCursor::new(
            size(24, 24),
            point(7, 7),
            vec![image()],
            CursorStyle::Custom(mesh_add().id())
        )
        .is_err()
    );
    assert!(
        CustomCursor::from_svg(b"not an SVG", size(24, 24), point(7, 7), CursorStyle::Arrow)
            .is_err()
    );
}

#[test]
fn windows_hand_artwork_uses_the_same_dpi_and_hotspot_contract() {
    for svg in [
        include_bytes!("../../../vendor/gpui-fast-windows/resources/cursors/open-hand.svg")
            .as_slice(),
        include_bytes!("../../../vendor/gpui-fast-windows/resources/cursors/closed-hand.svg")
            .as_slice(),
    ] {
        let cursor =
            CustomCursor::from_svg(svg, size(24, 24), point(10, 10), CursorStyle::Arrow).unwrap();
        assert_eq!(cursor.images().len(), 4);
        let (image, hotspot) = cursor.image_for_scale(1.5);
        assert_eq!(image.size(), size(36, 36));
        assert_eq!(hotspot, point(15, 15));
        assert!(image.rgba().chunks_exact(4).any(|pixel| pixel[3] == 255));
    }
}

#[gpui_kit::test]
fn unsupported_platform_returns_the_standard_fallback(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| {
        assert_eq!(
            cx.register_custom_cursor(mesh_add()),
            CursorStyle::Crosshair
        );
    });
}

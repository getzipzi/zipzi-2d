use super::*;

fn png(width: u32, height: u32, color: ColorType, pixels: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(color);
        encoder.set_depth(BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(pixels)
            .unwrap();
    }
    bytes
}

#[test]
fn bounded_decode_checks_workspace_and_dimensions_before_decoding_pixels() {
    let mut bytes = png(3, 1, ColorType::Rgba, &[7; 12]);
    // Keep IHDR but remove IDAT. Dimension/workspace errors must precede any
    // attempt to decode or validate the now-missing compressed image data.
    bytes.truncate(33);
    assert_eq!(
        decode_png_bounded(&bytes, (2, 1), PngDecodeLimits::default())
            .unwrap_err()
            .code,
        "RESOURCE_DIMENSIONS"
    );
    let required = png_decode_workspace_bytes(3, 1, bytes.len()).unwrap();
    assert_eq!(
        decode_png_bounded(
            &bytes,
            (3, 1),
            PngDecodeLimits {
                max_workspace_bytes: required - 1,
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        "DECODE_WORKSPACE_LIMIT"
    );
    assert_eq!(
        decode_png_bounded(
            &bytes,
            (3, 1),
            PngDecodeLimits {
                max_workspace_bytes: required,
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        "INVALID_PNG"
    );
}

#[test]
fn bounded_decode_preserves_rgba_and_normalizes_other_color_types() {
    for (color, pixels, rgba) in [
        (
            ColorType::Rgba,
            vec![250, 10, 99, 0, 7, 8, 9, 128],
            vec![250, 10, 99, 0, 7, 8, 9, 128],
        ),
        (
            ColorType::Rgb,
            vec![1, 2, 3, 4, 5, 6],
            vec![1, 2, 3, 255, 4, 5, 6, 255],
        ),
        (
            ColorType::Grayscale,
            vec![7, 8],
            vec![7, 7, 7, 255, 8, 8, 8, 255],
        ),
        (
            ColorType::GrayscaleAlpha,
            vec![7, 0, 8, 17],
            vec![7, 7, 7, 0, 8, 8, 8, 17],
        ),
    ] {
        let bytes = png(2, 1, color, &pixels);
        let image = decode_png_bounded(
            &bytes,
            (2, 1),
            PngDecodeLimits {
                max_encoded_bytes: bytes.len(),
                max_workspace_bytes: png_decode_workspace_bytes(2, 1, bytes.len()).unwrap(),
            },
        )
        .unwrap();
        assert_eq!(image.rgba, rgba);
        assert_eq!((image.width, image.height), (2, 1));
    }
}

#[test]
fn bounded_decode_rejects_encoded_limit_and_corrupt_complete_stream() {
    let mut bytes = png(2, 1, ColorType::Rgba, &[255; 8]);
    assert_eq!(
        decode_png_bounded(
            &bytes,
            (2, 1),
            PngDecodeLimits {
                max_encoded_bytes: bytes.len() - 1,
                ..Default::default()
            }
        )
        .unwrap_err()
        .code,
        "ENCODED_READ_LIMIT"
    );
    *bytes.last_mut().unwrap() ^= 1;
    assert_eq!(
        decode_png_bounded(&bytes, (2, 1), PngDecodeLimits::default())
            .unwrap_err()
            .code,
        "INVALID_PNG"
    );
}

#[test]
fn bounded_decode_does_not_expand_auxiliary_compressed_text() {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        encoder
            .add_ztxt_chunk("Unused".into(), "x".repeat(2 << 20))
            .unwrap();
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[7, 8, 9, 255])
            .unwrap();
    }
    assert!(bytes.len() < 16 << 10);
    let limits = PngDecodeLimits {
        max_encoded_bytes: bytes.len(),
        max_workspace_bytes: png_decode_workspace_bytes(1, 1, bytes.len()).unwrap(),
    };
    assert_eq!(
        decode_png_bounded(&bytes, (1, 1), limits).unwrap().rgba,
        [7, 8, 9, 255]
    );
}

#[test]
fn workspace_size_uses_checked_metadata_without_pixel_allocation() {
    for (w, h) in [(0, 1), (1, 0), (u32::MAX, u32::MAX)] {
        assert_eq!(
            png_decode_workspace_bytes(w, h, 0).unwrap_err().code,
            "RESOURCE_DIMENSIONS"
        );
    }
    assert_eq!(
        png_decode_workspace_bytes(1, 1, usize::MAX)
            .unwrap_err()
            .code,
        "RESOURCE_DIMENSIONS"
    );
    // Existing prepared-image maxima: 256 MiB RGBA, 64 MiB encoded, 16K axis.
    // Their conservative decode/conversion bound fits the default 1 GiB.
    let worst = png_decode_workspace_bytes(16384, 4096, 64 << 20).unwrap();
    assert!(worst < PngDecodeLimits::default().max_workspace_bytes);
    assert!(worst > 256 << 20);
}

//! Shared PNG validation and normalization for import and project resources.
use crate::types::Status;
use png::{BitDepth, ColorType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Limits for one bounded PNG acquisition, including encoded input, conversion
/// buffers, decoder metadata and scanline/inflate workspace. These are
/// conservative buffer bounds, not measured process memory.
#[derive(Debug, Clone, Copy)]
pub struct PngDecodeLimits {
    pub max_encoded_bytes: usize,
    pub max_workspace_bytes: usize,
}

impl Default for PngDecodeLimits {
    fn default() -> Self {
        Self {
            max_encoded_bytes: 64 << 20,
            max_workspace_bytes: 1 << 30,
        }
    }
}

fn dimensions_error() -> Status {
    Status::error(
        "RESOURCE_DIMENSIONS",
        "Invalid or overflowing PNG dimensions",
    )
}

/// Conservative encoded capacity for an RGBA8 PNG with one filter byte per
/// scanline, zlib expansion and PNG framing. Shared by admission and encoding.
pub fn rgba_png_capacity(width: u32, height: u32) -> Result<usize, Status> {
    if width == 0 || height == 0 {
        return Err(dimensions_error());
    }
    let raw = (width as usize)
        .checked_mul(4)
        .and_then(|n| n.checked_add(1))
        .and_then(|n| n.checked_mul(height as usize))
        .ok_or_else(dimensions_error)?;
    raw.checked_add(raw / 100)
        .and_then(|n| n.checked_add(1024))
        .ok_or_else(dimensions_error)
}

/// Conservative simultaneous storage bound for png 0.17: encoded source plus
/// bounded decoder chunk storage, decoder output plus RGBA conversion/Arc
/// conversion, scanline buffers (including RGBA16 input), and fixed inflate
/// workspace. Text and ICC decompression are disabled in the bounded decoder.
pub fn png_decode_workspace_bytes(
    width: u32,
    height: u32,
    encoded_bytes: usize,
) -> Result<usize, Status> {
    if width == 0 || height == 0 {
        return Err(dimensions_error());
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(8))
        .and_then(|n| n.checked_add(encoded_bytes.checked_mul(5)?))
        .and_then(|n| n.checked_add((width as usize).checked_mul(64)?))
        .and_then(|n| n.checked_add(4 << 20))
        .ok_or_else(dimensions_error)
}

/// Validate the fixed header before constructing a decoder or allocating its
/// output. CRCs, actual decoded layout and the complete stream are checked later.
pub fn png_dimensions(bytes: &[u8]) -> Result<(u32, u32), Status> {
    if bytes.len() < 33
        || bytes.get(..8) != Some(b"\x89PNG\r\n\x1a\n")
        || bytes[8..12] != 13u32.to_be_bytes()
        || &bytes[12..16] != b"IHDR"
    {
        return Err(Status::error("INVALID_PNG", "Invalid PNG header"));
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    png_decode_workspace_bytes(width, height, 0)?;
    Ok((width, height))
}

/// Check all large output/conversion allocations before entering PNG decoding.
pub fn decode_png_bounded(
    bytes: &[u8],
    expected: (u32, u32),
    limits: PngDecodeLimits,
) -> Result<DecodedImage, Status> {
    if bytes.len() > limits.max_encoded_bytes {
        return Err(Status::error(
            "ENCODED_READ_LIMIT",
            format!(
                "PNG requires {} encoded bytes, limit {}",
                bytes.len(),
                limits.max_encoded_bytes
            ),
        ));
    }
    let actual = png_dimensions(bytes)?;
    if actual != expected {
        return Err(Status::error(
            "RESOURCE_DIMENSIONS",
            format!(
                "PNG is {}x{}, expected {}x{}",
                actual.0, actual.1, expected.0, expected.1
            ),
        ));
    }
    let required = png_decode_workspace_bytes(actual.0, actual.1, bytes.len())?;
    if required > limits.max_workspace_bytes {
        return Err(Status::error(
            "DECODE_WORKSPACE_LIMIT",
            format!(
                "PNG decode requires {required} workspace bytes, limit {}",
                limits.max_workspace_bytes
            ),
        ));
    }
    decode_png_impl(bytes, Some((expected, limits)))
}

fn convert_to_rgba8(
    color_type: ColorType,
    bit_depth: BitDepth,
    mut data: Vec<u8>,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, Status> {
    let total_pixels = (width as usize) * (height as usize);
    let expected_rgba_len = total_pixels * 4;

    if bit_depth != BitDepth::Eight {
        return Err(Status::error(
            "INVALID_PNG",
            "Unsupported PNG bit depth (requires 8-bit)",
        ));
    }

    match color_type {
        ColorType::Rgba => {
            if data.len() < expected_rgba_len {
                return Err(Status::error("INVALID_PNG", "Incomplete RGBA PNG data"));
            }
            data.truncate(expected_rgba_len);
            Ok(data)
        }
        ColorType::Rgb => {
            let mut out = Vec::with_capacity(expected_rgba_len);
            for chunk in data.as_chunks::<3>().0.iter().take(total_pixels) {
                out.push(chunk[0]);
                out.push(chunk[1]);
                out.push(chunk[2]);
                out.push(255);
            }
            if out.len() != expected_rgba_len {
                return Err(Status::error("INVALID_PNG", "Incomplete RGB PNG data"));
            }
            Ok(out)
        }
        ColorType::Grayscale => {
            let mut out = Vec::with_capacity(expected_rgba_len);
            for &g in data.iter().take(total_pixels) {
                out.push(g);
                out.push(g);
                out.push(g);
                out.push(255);
            }
            if out.len() != expected_rgba_len {
                return Err(Status::error(
                    "INVALID_PNG",
                    "Incomplete Grayscale PNG data",
                ));
            }
            Ok(out)
        }
        ColorType::GrayscaleAlpha => {
            let mut out = Vec::with_capacity(expected_rgba_len);
            for chunk in data.as_chunks::<2>().0.iter().take(total_pixels) {
                out.push(chunk[0]);
                out.push(chunk[0]);
                out.push(chunk[0]);
                out.push(chunk[1]);
            }
            if out.len() != expected_rgba_len {
                return Err(Status::error(
                    "INVALID_PNG",
                    "Incomplete GrayscaleAlpha PNG data",
                ));
            }
            Ok(out)
        }
        _ => Err(Status::error(
            "INVALID_PNG",
            "Unsupported PNG color type (requires RGB, RGBA, or Grayscale)",
        )),
    }
}

pub fn decode_png(bytes: &[u8]) -> Result<DecodedImage, Status> {
    decode_png_impl(bytes, None)
}

fn decode_png_impl(
    bytes: &[u8],
    bounded: Option<((u32, u32), PngDecodeLimits)>,
) -> Result<DecodedImage, Status> {
    let cursor = std::io::Cursor::new(bytes);
    let mut decoder = png::Decoder::new(cursor);
    if let Some((expected, _)) = bounded {
        // Chunk storage is bounded independently from output and conversion
        // storage. No unbounded auxiliary compressed metadata is needed for RGBA.
        let decoder_bytes = bytes
            .len()
            .checked_mul(4)
            .and_then(|n| n.checked_add((expected.0 as usize).checked_mul(32)?))
            .and_then(|n| n.checked_add(1 << 20))
            .ok_or_else(dimensions_error)?;
        decoder.set_limits(png::Limits {
            bytes: decoder_bytes,
        });
        decoder.set_ignore_text_chunk(true);
        decoder.set_ignore_iccp_chunk(true);
        decoder.ignore_checksums(false);
    }
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(decode_error)?;

    let info = reader.info();
    let width = info.width;
    let height = info.height;

    if bounded.is_some_and(|(expected, _)| expected != (width, height)) {
        return Err(Status::error(
            "RESOURCE_DIMENSIONS",
            "Decoded PNG header differs from its source descriptor",
        ));
    }

    if bounded.is_none() && (width as u64) * (height as u64) > 268435456 {
        return Err(Status::error("CAPACITY", "PNG exceeds 1 GiB decoded"));
    }

    let output_size = reader.output_buffer_size();
    if let Some((_, limits)) = bounded {
        let required = png_decode_workspace_bytes(width, height, bytes.len())?;
        let rgba_size = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(dimensions_error)?;
        if output_size > rgba_size || required > limits.max_workspace_bytes {
            return Err(Status::error(
                "DECODE_WORKSPACE_LIMIT",
                "Decoder output exceeds the admitted PNG workspace",
            ));
        }
    }
    let mut buf = vec![0; output_size];
    let output_info = reader.next_frame(&mut buf).map_err(decode_error)?;
    buf.truncate(output_info.buffer_size());

    let rgba = convert_to_rgba8(
        output_info.color_type,
        output_info.bit_depth,
        buf,
        width,
        height,
    )?;

    reader.finish().map_err(decode_error)?;
    Ok(DecodedImage {
        rgba,
        width,
        height,
    })
}

fn decode_error(error: png::DecodingError) -> Status {
    if matches!(error, png::DecodingError::LimitsExceeded) {
        Status::error(
            "DECODE_WORKSPACE_LIMIT",
            "PNG decoder allocation limit exceeded",
        )
    } else {
        Status::error("INVALID_PNG", error.to_string())
    }
}

#[cfg(test)]
mod tests;

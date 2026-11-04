use std::io::{Cursor, Write};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};
use zipzi_project_preview::*;

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut e = png::Encoder::new(&mut bytes, width, height);
        e.set_color(png::ColorType::Rgba);
        e.write_header()
            .unwrap()
            .write_image_data(&vec![127; (width * height * 4) as usize])
            .unwrap();
    }
    bytes
}
fn archive(
    preview: Option<&ProjectPreview>,
    mutate: impl FnOnce(&mut serde_json::Value),
    extra: Option<(&str, &[u8])>,
) -> Vec<u8> {
    let mut format = serde_json::json!({"format":"zipzi-project-package","container_version":1,
        "document":"project.json","document_sha256":"unused-for-advisory-preview","required_features":[]});
    if let Some(p) = preview {
        format["preview"] = serde_json::to_value(p.descriptor()).unwrap();
    }
    mutate(&mut format);
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let deflate = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .large_file(true);
    zip.start_file("format.json", deflate).unwrap();
    zip.write_all(&serde_json::to_vec(&format).unwrap())
        .unwrap();
    zip.start_file("project.json", deflate).unwrap();
    zip.write_all(b"document not decoded by preview reader")
        .unwrap();
    if let Some(p) = preview {
        zip.start_file(
            PREVIEW_PATH,
            SimpleFileOptions::default()
                .compression_method(CompressionMethod::Stored)
                .large_file(true),
        )
        .unwrap();
        zip.write_all(p.png()).unwrap();
    }
    if let Some((name, bytes)) = extra {
        zip.start_file(name, deflate).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn bounded_preview_roundtrip_and_absence_without_document_decode() {
    let image = ProjectPreview::from_png(png(8, 4)).unwrap();
    let bytes = archive(Some(&image), |_| {}, None);
    let result = read_preview(Cursor::new(bytes)).unwrap().unwrap();
    assert_eq!(image.png(), result.png());
    assert_eq!(image.descriptor(), result.descriptor());
    assert!(read_preview(Cursor::new(archive(None, |_| {}, None)))
        .unwrap()
        .is_none());
}

#[test]
fn rejects_corrupt_hash_wrong_dimensions_unknown_version_and_undeclared_preview() {
    let image = ProjectPreview::from_png(png(8, 4)).unwrap();
    for change in [0, 1, 2, 3, 4] {
        let bytes = archive(
            Some(&image),
            |f| match change {
                0 => f["preview"]["sha256"] = "incorrect".into(),
                1 => f["preview"]["width"] = 9.into(),
                2 => f["container_version"] = 2.into(),
                3 => f["required_features"] = serde_json::json!(["unknown"]),
                _ => {
                    f.as_object_mut().unwrap().remove("preview");
                }
            },
            None,
        );
        assert!(read_preview(Cursor::new(bytes)).is_err(), "change {change}");
    }
    assert!(ProjectPreview::from_png(png(1025, 1)).is_err());
    assert!(ProjectPreview::from_png(vec![0; MAX_PREVIEW_BYTES + 1]).is_err());
    let mut truncated = png(8, 4);
    truncated.truncate(truncated.len() / 2);
    assert!(ProjectPreview::from_png(truncated).is_err());
}

#[test]
fn rejects_index_bombs_bad_paths_and_crc() {
    let image = ProjectPreview::from_png(png(8, 4)).unwrap();
    for name in [
        "../escape.png",
        "Preview.png",
        "other.bin",
        "assets/../bad.png",
    ] {
        assert!(read_preview(Cursor::new(archive(
            Some(&image),
            |_| {},
            Some((name, b"x"))
        )))
        .is_err());
    }
    let mut bytes = archive(Some(&image), |_| {}, None);
    let end = bytes.len() - 22;
    bytes[end + 12..end + 16].copy_from_slice(&(9u32 << 20).to_le_bytes());
    assert!(read_preview(Cursor::new(bytes)).is_err());
    let mut bytes = archive(Some(&image), |_| {}, None);
    let start = {
        let mut z = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
        let offset = z.by_name(PREVIEW_PATH).unwrap().data_start().unwrap() as usize;
        offset
    };
    bytes[start + 40] ^= 1;
    assert!(read_preview(Cursor::new(bytes)).is_err());
}

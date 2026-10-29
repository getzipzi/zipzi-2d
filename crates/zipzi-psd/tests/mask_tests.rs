use ag_psd::psd::{ColorMode, Layer, LayerMaskData, LayerVectorMask, PixelData, Psd, WriteOptions};
use zipzi_core::{image::decode_png, Vec2};
use zipzi_psd::{import_psd, import_psd_pixels, import_psd_pixels_with_limit, ImportBundle};

fn raster(left: f64, top: f64, alphas: [u8; 4]) -> Layer {
    let mut layer = Layer::default();
    layer.additional_info.name = Some("蒙版图层".into());
    layer.left = Some(left);
    layer.top = Some(top);
    layer.image_data = Some(PixelData {
        width: 2,
        height: 2,
        data: rgba(alphas),
    });
    layer
}

fn rgba(alphas: [u8; 4]) -> Vec<u8> {
    alphas.into_iter().flat_map(|a| [80, 120, 200, a]).collect()
}

fn mask(left: f64, top: f64, width: u32, gray: &[u8]) -> LayerMaskData {
    LayerMaskData {
        left: Some(left),
        top: Some(top),
        image_data: Some(PixelData {
            width,
            height: gray.len() as u32 / width,
            data: gray.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        }),
        ..Default::default()
    }
}

fn psd(layers: Vec<Layer>) -> Vec<u8> {
    ag_psd::write_psd(
        &Psd {
            width: 8.0,
            height: 8.0,
            color_mode: Some(ColorMode::Rgb),
            bits_per_channel: Some(8.0),
            children: Some(layers),
            ..Default::default()
        },
        &WriteOptions::default(),
    )
}

/// Check independently specified RGBA bytes in both publication paths, including
/// RGB under zero alpha. Comparing only the two paths could hide a shared bug.
fn assert_pixels(bytes: &[u8], alphas: [u8; 4]) -> ImportBundle {
    let pngs = import_psd(bytes).unwrap();
    let pixels = import_psd_pixels(bytes).unwrap();
    assert_eq!(pngs.report, pixels.report);
    assert_eq!(pngs.report.raster_layers, 1);
    assert_eq!(pngs.assets.len(), 1);
    assert_eq!(pixels.assets.len(), 1);
    let png = decode_png(&pngs.assets[0].bytes).unwrap();
    assert_eq!((png.width, png.height), (2, 2));
    assert_eq!((pixels.assets[0].width, pixels.assets[0].height), (2, 2));
    assert_eq!(png.rgba, rgba(alphas));
    assert_eq!(pixels.assets[0].rgba, rgba(alphas));
    assert_eq!(pixels.assets[0].id, pngs.assets[0].id);
    assert!(pngs.document.validate_structure().is_empty());
    assert!(pixels.document.validate_structure().is_empty());
    assert_eq!(pngs.document.mesh_order(), pixels.document.mesh_order());
    for id in pngs.document.mesh_order() {
        assert_eq!(pngs.document.get_mesh(id), pixels.document.get_mesh(id));
    }
    pngs
}

fn assert_rejected(layer: Layer, code: &str, reason: &str) {
    let bytes = psd(vec![layer]);
    let png_error = import_psd(&bytes).err().expect("PNG import must fail");
    let pixel_error = import_psd_pixels(&bytes)
        .err()
        .expect("RGBA import must fail");
    assert_eq!(png_error, pixel_error);
    assert_eq!(png_error.code, code);
    assert!(png_error.message.contains("蒙版图层"), "{png_error}");
    assert!(png_error.message.contains(reason), "{png_error}");
}

#[test]
fn bitmap_mask_multiplies_existing_alpha_and_preserves_layer_properties() {
    let mut layer = raster(1.0, 2.0, [255, 128, 64, 0]);
    layer.opacity = Some(128.0 / 255.0);
    layer.hidden = Some(true);
    layer.additional_info.mask = Some(mask(1.0, 2.0, 2, &[0, 64, 128, 255]));
    let bundle = assert_pixels(&psd(vec![layer]), [0, 32, 32, 0]);
    assert_eq!(bundle.report.warnings.len(), 1);
    assert!(bundle.report.warnings[0].contains("蒙版图层"));
    assert!(bundle.report.warnings[0].contains("baked into texture alpha"));
    let mesh = bundle
        .document
        .get_mesh(&bundle.document.mesh_order()[0])
        .unwrap();
    assert_eq!(mesh.name, "蒙版图层");
    assert!(!mesh.enabled);
    assert!((mesh.appearance.opacity - 128.0 / 255.0).abs() < 1e-6);
    assert_eq!(
        mesh.base_positions,
        vec![
            Vec2::new(-3., 0.),
            Vec2::new(-1., 0.),
            Vec2::new(-1., 2.),
            Vec2::new(-3., 2.),
        ]
    );
    assert!(mesh.masks.is_empty());
    assert!(bundle.document.offscreen_order().is_empty());
}

#[test]
fn bitmap_mask_uses_its_offset_and_default_color_outside_its_bounds() {
    for (default_color, outside) in [(0.0, 0), (255.0, 128)] {
        let mut layer = raster(3.0, 4.0, [128; 4]);
        let mut bitmap = mask(4.0, 5.0, 1, &[64]);
        bitmap.default_color = Some(default_color);
        layer.additional_info.mask = Some(bitmap);
        assert_pixels(&psd(vec![layer]), [outside, outside, outside, 32]);
    }
}

#[test]
fn bitmap_mask_can_be_larger_than_the_layer_with_negative_coordinates() {
    let mut layer = raster(-1.0, -2.0, [255; 4]);
    layer.additional_info.mask = Some(mask(
        -2.0,
        -3.0,
        4,
        &[0, 0, 0, 0, 0, 30, 40, 0, 0, 50, 60, 0, 0, 0, 0, 0],
    ));
    assert_pixels(&psd(vec![layer]), [30, 40, 50, 60]);
}

#[test]
fn bitmap_mask_coordinates_can_be_relative_to_the_layer() {
    let mut layer = raster(3.0, 4.0, [255; 4]);
    let mut bitmap = mask(1.0, 0.0, 1, &[64, 128]);
    bitmap.position_relative_to_layer = Some(true);
    bitmap.default_color = Some(255.0);
    layer.additional_info.mask = Some(bitmap);
    assert_pixels(&psd(vec![layer]), [255, 64, 255, 128]);
}

#[test]
fn bitmap_mask_density_restores_visibility_instead_of_multiplying_opacity() {
    for (density, expected) in [
        (0.0, [128; 4]),
        (128.0 / 255.0, [64, 80, 96, 128]),
        (1.0, [0, 32, 64, 128]),
    ] {
        let mut layer = raster(1.0, 2.0, [128; 4]);
        let mut bitmap = mask(1.0, 2.0, 2, &[0, 64, 128, 255]);
        bitmap.user_mask_density = Some(density);
        bitmap.user_mask_feather = Some(0.0);
        layer.additional_info.mask = Some(bitmap);
        assert_pixels(&psd(vec![layer]), expected);
    }
}

#[test]
fn disabled_bitmap_mask_does_not_affect_pixels_even_with_feather() {
    let mut layer = raster(1.0, 2.0, [255, 128, 64, 0]);
    let mut bitmap = mask(1.0, 2.0, 2, &[0; 4]);
    bitmap.disabled = Some(true);
    bitmap.user_mask_feather = Some(3.0);
    layer.additional_info.mask = Some(bitmap);
    let bundle = assert_pixels(&psd(vec![layer]), [255, 128, 64, 0]);
    assert!(bundle.report.warnings.is_empty());
}

#[test]
fn empty_bitmap_mask_uses_its_constant_default_color() {
    for (default_color, expected) in [(0.0, [0; 4]), (255.0, [128; 4])] {
        let mut layer = raster(1.0, 2.0, [128; 4]);
        layer.additional_info.mask = Some(LayerMaskData {
            default_color: Some(default_color),
            ..Default::default()
        });
        assert_pixels(&psd(vec![layer]), expected);
    }
}

#[test]
fn nonoverlapping_bitmap_mask_uses_default_color() {
    let mut layer = raster(1.0, 2.0, [128; 4]);
    layer.additional_info.mask = Some(mask(100.0, 200.0, 1, &[255]));
    assert_pixels(&psd(vec![layer]), [0; 4]);
}

#[test]
fn bitmap_masks_compose_with_existing_clipping_groups() {
    let mut base = raster(1.0, 2.0, [255; 4]);
    base.additional_info.mask = Some(mask(1.0, 2.0, 2, &[0, 64, 128, 255]));
    let mut clipped = raster(1.0, 2.0, [128; 4]);
    clipped.clipping = Some(true);
    clipped.additional_info.mask = Some(mask(1.0, 2.0, 2, &[255, 128, 64, 0]));
    let bytes = psd(vec![base, clipped]);
    let pngs = import_psd(&bytes).unwrap();
    let pixels = import_psd_pixels(&bytes).unwrap();
    assert_eq!(pngs.report, pixels.report);
    assert_eq!(pngs.report.raster_layers, 2);
    assert_eq!(pngs.report.warnings.len(), 2);
    assert_eq!(pngs.document.offscreen_order().len(), 1);
    for (index, expected) in [[0, 64, 128, 255], [128, 64, 32, 0]]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            decode_png(&pngs.assets[index].bytes).unwrap().rgba,
            rgba(expected)
        );
        assert_eq!(pixels.assets[index].rgba, rgba(expected));
        let mesh = pngs
            .document
            .get_mesh(&pngs.document.mesh_order()[index])
            .unwrap();
        assert_eq!(mesh.raw_blend_mode, (index == 1).then_some(256));
        assert!(mesh.masks.is_empty());
    }
}

#[test]
fn mask_decode_is_included_in_the_strict_host_memory_budget() {
    let mut layer = raster(1.0, 2.0, [128; 4]);
    layer.additional_info.mask = Some(mask(0.0, 0.0, 64, &vec![255; 64 * 64]));
    let bytes = psd(vec![layer]);
    let error = import_psd_pixels_with_limit(&bytes, 1024).err().unwrap();
    assert_eq!(error.code, "RESOURCE_BUDGET_EXCEEDED");
    assert!(import_psd_pixels_with_limit(&bytes, 1 << 20).is_ok());
    assert!(import_psd_pixels(&bytes).is_ok());
}

#[test]
fn unsupported_mask_kinds_still_fail_with_specific_layer_errors() {
    let mut group = Layer::default();
    group.additional_info.name = Some("蒙版图层".into());
    group.children = Some(vec![raster(1.0, 2.0, [255; 4])]);
    group.additional_info.mask = Some(mask(1.0, 2.0, 2, &[128; 4]));
    assert_rejected(group, "UNSUPPORTED_LAYER", "group masks");

    let mut vector = raster(1.0, 2.0, [255; 4]);
    vector.additional_info.vector_mask = Some(LayerVectorMask::default());
    assert_rejected(
        vector.clone(),
        "UNSUPPORTED_LAYER",
        "vector and combined masks",
    );

    // Both canonical vmsk and its vsms alias must be rejected. The reader's
    // alias dispatch drops vmsk in 0.3.0, so the metadata preflight is essential.
    vector.additional_info.mask = Some(mask(1.0, 2.0, 2, &[128; 4]));
    assert_rejected(
        vector.clone(),
        "UNSUPPORTED_LAYER",
        "vector and combined masks",
    );
    let mut alias = psd(vec![vector]);
    let key = alias
        .windows(8)
        .position(|bytes| bytes == b"8BIMvmsk")
        .unwrap()
        + 4;
    alias[key..key + 4].copy_from_slice(b"vsms");
    assert_eq!(import_psd(&alias).err().unwrap().code, "UNSUPPORTED_LAYER");
    assert_eq!(
        import_psd_pixels(&alias).err().unwrap().code,
        "UNSUPPORTED_LAYER"
    );

    let mut combined = raster(1.0, 2.0, [255; 4]);
    combined.additional_info.mask = Some(mask(1.0, 2.0, 2, &[128; 4]));
    combined.additional_info.real_mask = Some(mask(1.0, 2.0, 2, &[128; 4]));
    assert_rejected(combined, "UNSUPPORTED_LAYER", "vector and combined masks");

    let mut derived = raster(1.0, 2.0, [255; 4]);
    let mut bitmap = mask(1.0, 2.0, 2, &[128; 4]);
    bitmap.from_vector_data = Some(true);
    derived.additional_info.mask = Some(bitmap);
    assert_rejected(derived, "UNSUPPORTED_LAYER", "vector and combined masks");
}

#[test]
fn bitmap_mask_feather_is_rejected_instead_of_silently_ignored() {
    let mut layer = raster(1.0, 2.0, [255; 4]);
    let mut bitmap = mask(1.0, 2.0, 2, &[128; 4]);
    bitmap.user_mask_feather = Some(3.0);
    layer.additional_info.mask = Some(bitmap);
    assert_rejected(layer, "UNSUPPORTED_LAYER", "bitmap mask feather");
}

#[test]
fn mask_preflight_does_not_treat_layer_names_as_metadata() {
    let mut layer = raster(1.0, 2.0, [128; 4]);
    layer.additional_info.name = Some("8BIMvmsk".into());
    layer.additional_info.mask = Some(mask(1.0, 2.0, 2, &[255; 4]));
    assert_pixels(&psd(vec![layer]), [128; 4]);
}

#[test]
fn obsolete_invert_flag_is_rejected_but_has_no_effect_on_disabled_masks() {
    let mut layer = raster(1.0, 2.0, [128; 4]);
    layer.additional_info.mask = Some(mask(10.0, 20.0, 2, &[128; 4]));
    let mut bytes = psd(vec![layer]);
    // Use a unique mask rectangle to locate its flags in this generated file.
    let bounds: Vec<_> = [20_i32, 10, 22, 12]
        .into_iter()
        .flat_map(i32::to_be_bytes)
        .collect();
    let flags = bytes.windows(16).position(|value| value == bounds).unwrap() + 17;
    bytes[flags] |= 4;
    for error in [
        import_psd(&bytes).err().unwrap(),
        import_psd_pixels(&bytes).err().unwrap(),
    ] {
        assert_eq!(error.code, "UNSUPPORTED_LAYER");
        assert!(error.message.contains("obsolete inverted bitmap masks"));
    }
    bytes[flags] |= 2;
    assert!(assert_pixels(&bytes, [128; 4]).report.warnings.is_empty());
}

#[test]
fn truncated_mask_psd_never_panics() {
    let mut layer = raster(1.0, 2.0, [128; 4]);
    layer.additional_info.mask = Some(mask(1.0, 2.0, 2, &[128; 4]));
    let bytes = psd(vec![layer]);
    for length in 26..bytes.len() {
        let result = std::panic::catch_unwind(|| import_psd(&bytes[..length]));
        assert!(result.is_ok(), "length {length}");
    }
}

/// Keep private artwork out of the repository; run this acceptance check with
/// ZIPZI_PSD_TEST_SOURCE=/absolute/path/to/art.psd and --ignored --nocapture.
#[test]
#[ignore = "requires local artwork supplied via ZIPZI_PSD_TEST_SOURCE"]
fn local_psd_imports_with_bitmap_masks() {
    use ag_psd::{get_layer_image_data, get_layer_mask_image_data, psd::ReadOptions};

    fn flatten<'a>(layers: &'a [Layer], output: &mut Vec<&'a Layer>) {
        for layer in layers {
            if let Some(children) = &layer.children {
                flatten(children, output);
            } else {
                output.push(layer);
            }
        }
    }

    let source = std::env::var_os("ZIPZI_PSD_TEST_SOURCE").expect("set ZIPZI_PSD_TEST_SOURCE");
    let bytes = std::fs::read(&source).unwrap();
    let parsed = ag_psd::read_psd(
        &bytes,
        &ReadOptions {
            use_raw_data: Some(true),
            skip_composite_image_data: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    let mut layers = Vec::new();
    flatten(parsed.children.as_deref().unwrap_or_default(), &mut layers);
    eprintln!(
        "source={source:?}, bytes={}, canvas={}x{}, raster_layers={}",
        bytes.len(),
        parsed.width,
        parsed.height,
        layers.len()
    );
    let mut active_masks = 0;
    for layer in &layers {
        if let Some(mask) = &layer.additional_info.mask {
            active_masks += usize::from(mask.disabled != Some(true));
            eprintln!("mask layer={:?}, bounds={:?}, disabled={:?}, relative={:?}, density={:?}, feather={:?}",
                layer.additional_info.name,
                (mask.left, mask.top, mask.right, mask.bottom),
                mask.disabled, mask.position_relative_to_layer, mask.user_mask_density, mask.user_mask_feather);
        }
    }

    let pngs = import_psd(&bytes).unwrap_or_else(|error| panic!("PNG import: {error}"));
    let pixels = import_psd_pixels(&bytes).unwrap_or_else(|error| panic!("RGBA import: {error}"));
    eprintln!(
        "imported: {}x{}, {} layers, {} groups, warnings={:#?}",
        pngs.report.width,
        pngs.report.height,
        pngs.report.raster_layers,
        pngs.report.groups,
        pngs.report.warnings
    );
    assert_eq!(pngs.report, pixels.report);
    assert_eq!(pngs.assets.len(), layers.len());
    assert_eq!(pixels.assets.len(), layers.len());
    assert!(pngs.document.validate_structure().is_empty());
    assert!(pixels.document.validate_structure().is_empty());
    assert_eq!(pngs.document.mesh_order(), pixels.document.mesh_order());
    assert!(
        active_masks > 0,
        "acceptance artwork must exercise an active bitmap mask"
    );
    for (index, layer) in layers.iter().enumerate() {
        let decoded = decode_png(&pngs.assets[index].bytes).unwrap();
        assert_eq!(decoded.rgba, pixels.assets[index].rgba);
        assert_eq!(pngs.assets[index].id, pixels.assets[index].id);
        let mesh_id = &pngs.document.mesh_order()[index];
        assert_eq!(
            pngs.document.get_mesh(mesh_id),
            pixels.document.get_mesh(mesh_id)
        );
        let original = get_layer_image_data(layer).unwrap().unwrap();
        assert_eq!(
            (decoded.width, decoded.height),
            (original.width, original.height)
        );
        let Some(mask) = layer
            .additional_info
            .mask
            .as_ref()
            .filter(|mask| mask.disabled != Some(true))
        else {
            assert_eq!(decoded.rgba, original.data);
            continue;
        };
        let bitmap = get_layer_mask_image_data(layer).unwrap();
        let layer_left = layer.left.unwrap() as i64;
        let layer_top = layer.top.unwrap() as i64;
        let relative = mask.position_relative_to_layer == Some(true);
        let left = mask.left.unwrap() as i64 + if relative { layer_left } else { 0 };
        let top = mask.top.unwrap() as i64 + if relative { layer_top } else { 0 };
        let density = mask.user_mask_density.unwrap_or(1.0);
        let mut changed = 0;
        for (pixel, (before, after)) in original
            .data
            .chunks_exact(4)
            .zip(decoded.rgba.chunks_exact(4))
            .enumerate()
        {
            assert_eq!(&before[..3], &after[..3]);
            let x = layer_left + (pixel % original.width as usize) as i64 - left;
            let y = layer_top + (pixel / original.width as usize) as i64 - top;
            let gray = bitmap
                .as_ref()
                .filter(|data| {
                    x >= 0 && y >= 0 && x < i64::from(data.width) && y < i64::from(data.height)
                })
                .map(|data| f64::from(data.data[((y as u32 * data.width + x as u32) * 4) as usize]))
                .unwrap_or(mask.default_color.unwrap_or(0.0));
            let expected =
                (f64::from(before[3]) * (1.0 - density + density * gray / 255.0)).round() as u8;
            assert_eq!(
                after[3], expected,
                "layer {:?}, pixel {pixel}",
                layer.additional_info.name
            );
            changed += usize::from(before[3] != after[3]);
        }
        eprintln!(
            "verified mask {:?}: {} pixels, {} alpha values changed",
            layer.additional_info.name,
            original.width as u64 * original.height as u64,
            changed
        );
    }
}

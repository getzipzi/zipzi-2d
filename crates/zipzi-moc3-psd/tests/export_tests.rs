use ag_psd::psd::ReadOptions;
use ag_psd::read_psd;
use std::collections::HashMap;
use std::path::PathBuf;
use zipzi_moc3_psd::{from_moc3_file, from_model3_file};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

fn u32be(bytes: &[u8], pos: usize) -> u32 {
    u32::from_be_bytes(bytes[pos..pos + 4].try_into().unwrap())
}

#[test]
fn exports_real_model_with_layers_and_composite() {
    let (psd, report) = from_model3_file(&fixture("gpu/gpu-package/model.model3.json")).unwrap();
    assert_eq!((report.width, report.height, report.layers), (640, 480, 19));
    assert!(!report.warnings.iter().any(|w| w.contains("masks")));
    assert_eq!(&psd[..4], b"8BPS");
    assert_eq!(u32be(&psd, 14), 480);
    assert_eq!(u32be(&psd, 18), 640);
    let layer_section = 26 + 4 + 4;
    let layer_info = layer_section + 4;
    assert!(u32be(&psd, layer_section) > 0);
    let parsed = read_psd(
        &psd,
        &ReadOptions {
            skip_composite_image_data: Some(true),
            use_image_data: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    let tree = parsed.children.unwrap();
    fn flatten<'a>(
        tree: &'a [ag_psd::psd::Layer],
        groups: &mut usize,
    ) -> Vec<&'a ag_psd::psd::Layer> {
        let mut layers = Vec::new();
        for layer in tree {
            if let Some(children) = &layer.children {
                *groups += 1;
                layers.extend(flatten(children, groups));
            } else {
                layers.push(layer);
            }
        }
        layers
    }
    let mut groups = 0;
    let children = flatten(&tree, &mut groups);
    assert_eq!(children.len(), report.layers);
    // Folder start/end records participate in PSD's signed layer count.
    assert_eq!(
        i16::from_be_bytes(psd[layer_info + 4..layer_info + 6].try_into().unwrap()).unsigned_abs()
            as usize,
        report.layers + 2 * groups
    );
    // Linear filtering adds intermediate colors; compare compression with the
    // decoded payload instead of a size tuned to nearest-neighbor output.
    let raw_bytes: usize = children
        .iter()
        .map(|layer| {
            layer
                .image_data
                .as_ref()
                .map_or(0, |image| image.data.len())
                + layer
                    .additional_info
                    .mask
                    .as_ref()
                    .and_then(|mask| mask.image_data.as_ref())
                    .map_or(0, |image| image.data.len())
        })
        .sum();
    assert!(psd.len() < raw_bytes / 2, "PSD layers should be compressed");
    assert!(children.iter().any(|layer| layer
        .additional_info
        .mask
        .as_ref()
        .is_some_and(|mask| mask.image_data.is_some())));
    let hidden: Vec<_> = children
        .iter()
        .filter(|layer| layer.hidden == Some(true))
        .collect();
    assert_eq!(hidden.len(), 9);
    assert!(hidden.iter().all(|layer| layer
        .image_data
        .as_ref()
        .unwrap()
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .any(|pixel| pixel[3] != 0)));
    assert!(psd.windows(4).any(|bytes| bytes == b"luni"));
    let composite = layer_section + 4 + u32be(&psd, layer_section) as usize;
    assert_eq!(&psd[composite..composite + 2], &[0, 1]);
    assert!(psd[composite + 2..].iter().any(|&value| value != 0));
}

#[test]
fn bare_moc3_needs_explicit_textures() {
    let moc3 = fixture("external_v50/model.moc3");
    let error = from_moc3_file(&moc3, &HashMap::new()).unwrap_err();
    assert!(error.to_string().contains("texture"));
    let texture = fixture("external_v50/texture_00.png");
    let (psd, report) = from_moc3_file(&moc3, &HashMap::from([(0, texture)])).unwrap();
    assert_eq!((report.width, report.height, report.layers), (400, 400, 1));
    assert_eq!(&psd[..4], b"8BPS");
}

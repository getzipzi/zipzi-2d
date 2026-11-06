use zipzi_core::{state::*, types::*};
pub const DOC: &str = "00000000-0000-4000-8000-000000000001";
pub const ASSET: &str = "00000000-0000-4000-8000-000000000002";
pub const MESH: &str = "00000000-0000-4000-8000-000000000003";
pub const PARAM: &str = "00000000-0000-4000-8000-000000000004";
pub fn fixture() -> (DocumentState, Vec<u8>) {
    let mut d = DocumentState::new(DOC, Canvas::new(128., 64., Vec2::new(20., 30.), 32.).into())
        .unwrap()
        .builder();
    d.create_asset(AssetRecord {
        id: ASSET.into(),
        name: "Texture".into(),
        content: AssetContent::Image {
            resource: ResourceKey::new(),
            width: 2,
            height: 2,
        },
    })
    .unwrap();
    let positions = vec![Vec2::new(8., 12.), Vec2::new(104., 12.), Vec2::new(8., 60.)];
    assert!(d
        .create_mesh(Mesh {
            id: MESH.into(),
            runtime_id: "ArtMesh".into(),
            name: "日本語 & <中文> \"图层\"\n".into(),
            texture_asset_id: ASSET.into(),
            vertex_ids: vec![100, 7, 23],
            base_positions: positions.clone(),
            uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
            triangles: vec![[100, 7, 23]],
            ..Default::default()
        })
        .is_ok());
    assert!(d
        .create_parameter(Parameter {
            id: PARAM.into(),
            runtime_id: "ParamX".into(),
            name: "X".into(),
            repeat: true,
            ..Default::default()
        })
        .is_ok());
    assert!(d
        .create_binding(MeshBinding {
            id: "00000000-0000-4000-8000-000000000005".into(),
            mesh_id: MESH.into(),
            axes: vec![BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: PARAM.into(),
                keys: vec![-1., 0., 1.]
            }],
            keyforms: [-1., 0., 1.]
                .into_iter()
                .map(|v| MeshKeyform {
                    keys: vec![v],
                    positions: positions
                        .iter()
                        .map(|p| Vec2::new(p.x + v * 4., p.y))
                        .collect(),
                    appearance: Appearance {
                        opacity: 1. - v.abs() * 0.25,
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .collect()
        })
        .is_ok());
    let mut png = Vec::new();
    {
        let mut e = png::Encoder::new(&mut png, 2, 2);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()
            .unwrap()
            .write_image_data(&[255; 16])
            .unwrap();
    }
    (d.freeze().unwrap().state, png)
}

use std::io::{Cursor, Read};
// Independent CAFF reader in the contract test: check offsets, varints, XOR
// widths, archive guards and the ZIP payload, rather than calling writer internals.
#[allow(dead_code)]
pub fn archive(bytes: &[u8]) -> Vec<(String, String, u8, Vec<u8>)> {
    assert_eq!(&bytes[..4], b"CAFF");
    assert_eq!(&bytes[bytes.len() - 2..], b"bc");
    let key = u32::from_be_bytes(bytes[14..18].try_into().unwrap());
    let mut cursor = 54;
    fn read32(bytes: &[u8], cursor: &mut usize, key: u32) -> u32 {
        let result = u32::from_be_bytes(bytes[*cursor..*cursor + 4].try_into().unwrap()) ^ key;
        *cursor += 4;
        result
    }
    fn string(bytes: &[u8], cursor: &mut usize, key: u32) -> String {
        let mut len = 0;
        loop {
            let b = bytes[*cursor] ^ key as u8;
            *cursor += 1;
            len = (len << 7) | (b & 127) as usize;
            if b & 128 == 0 {
                break;
            }
        }
        let decoded = bytes[*cursor..*cursor + len]
            .iter()
            .map(|v| v ^ key as u8)
            .collect();
        *cursor += len;
        String::from_utf8(decoded).unwrap()
    }
    let count = read32(bytes, &mut cursor, key);
    let mut entries = Vec::new();
    for _ in 0..count {
        let path = string(bytes, &mut cursor, key);
        let tag = string(bytes, &mut cursor, key);
        let mask = (((key as i32) as i64) << 32) | ((key as i32) as i64);
        let offset = (u64::from_be_bytes(bytes[cursor..cursor + 8].try_into().unwrap())
            ^ mask as u64) as usize;
        cursor += 8;
        let size = read32(bytes, &mut cursor, key) as usize;
        assert_eq!(bytes[cursor] ^ key as u8, 1);
        cursor += 1;
        let compression = bytes[cursor] ^ key as u8;
        cursor += 9;
        assert!(offset >= cursor);
        assert!(offset + size <= bytes.len() - 2);
        let stored: Vec<_> = bytes[offset..offset + size]
            .iter()
            .map(|v| v ^ key as u8)
            .collect();
        let content = if compression == 16 {
            stored
        } else {
            assert_eq!(compression, 33);
            let mut zip = zip::ZipArchive::new(Cursor::new(stored)).unwrap();
            assert_eq!(zip.len(), 1);
            let mut contents = zip.by_name("contents").unwrap();
            let mut decoded = Vec::new();
            contents.read_to_end(&mut decoded).unwrap();
            decoded
        };
        entries.push((path, tag, compression, content));
    }
    entries
}

/// Construct a validated fixture candidate without mutating the source snapshot.
#[allow(dead_code)]
pub fn prepare(
    state: &DocumentState,
    write: impl FnOnce(&mut DocumentBuilder) -> Result<(), zipzi_core::Status>,
) -> DocumentState {
    let mut builder = state.builder();
    write(&mut builder).unwrap();
    builder.freeze().unwrap().state
}

/// Resolve the decoder's source descriptors against the PNGs owned by this
/// codec fixture. Production Project import adopts these bytes into ResourceStore.
#[allow(dead_code)]
pub fn resolve_images(imported: &zipzi_cmo3::ImportedModel) -> DocumentState {
    prepare(&imported.document, |builder| {
        for id in imported.document.asset_order() {
            let mut asset = imported.document.asset(id).unwrap().clone();
            let AssetContent::Unresolved { source } = &asset.content else {
                continue;
            };
            let decoder = png::Decoder::new(Cursor::new(&imported.images[source]));
            let mut reader = decoder.read_info().unwrap();
            let mut pixels = vec![0; reader.output_buffer_size()];
            let image = reader.next_frame(&mut pixels).unwrap();
            asset.content = AssetContent::Image {
                resource: ResourceKey::new(),
                width: image.width,
                height: image.height,
            };
            builder.replace_asset(asset)?;
        }
        Ok(())
    })
}

use super::helpers::{id, TestDirectory};
use std::{collections::HashMap, fs};
use zipzi_core::{Canvas, Document, ImageAsset, ImageContent, Mesh, Vec2};
use zipzi_project::{content_sha256, DocumentSession, ImageMemoryBudget, TextureData};

fn fixture(tmp: &TestDirectory) -> (Document, Vec<u8>, TextureData) {
    let width = 24;
    let mut rgba = Vec::new();
    for y in 0..width {
        for x in 0..width {
            rgba.extend([x as u8 * 7, y as u8 * 9, 53, 255]);
        }
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, width, width);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&rgba)
            .unwrap();
    }
    let path = tmp.0.join("page.png");
    fs::write(&path, &png).unwrap();
    let mut doc = Document::new();
    assert!(doc
        .initialize(id(900), Canvas::new(256., 256., Vec2::default(), 1.))
        .is_ok());
    assert!(doc
        .add_asset(ImageAsset {
            id: id(901),
            name: "Atlas".into(),
            unresolved_source: None,
            content: Some(ImageContent {
                source: path.to_string_lossy().into_owned(),
                width,
                height: width,
                sha256: content_sha256(&png)
            }),
        })
        .status
        .is_ok());
    for (i, uvs) in [
        vec![
            Vec2::new(0.5, 0.2),
            Vec2::new(0.8, 0.5),
            Vec2::new(0.5, 0.8),
            Vec2::new(0.2, 0.5),
        ],
        vec![
            Vec2::new(0.22, 0.72),
            Vec2::new(0.28, 0.72),
            Vec2::new(0.28, 0.78),
            Vec2::new(0.22, 0.78),
        ],
    ]
    .into_iter()
    .enumerate()
    {
        assert!(doc
            .create_mesh(Mesh {
                id: id(910 + i as i32),
                runtime_id: format!("Piece{i}"),
                name: format!("Piece {i}"),
                texture_asset_id: id(901).into(),
                vertex_ids: vec![0, 1, 2, 3],
                base_positions: uvs
                    .iter()
                    .map(|p| Vec2::new(p.x * 100., p.y * 100.))
                    .collect(),
                uvs,
                triangles: vec![[0, 1, 2], [0, 2, 3]],
                ..Default::default()
            })
            .status
            .is_ok());
    }
    let parameter = zipzi_core::Parameter {
        id: id(920),
        runtime_id: "ParamX".into(),
        name: "X".into(),
        minimum: -1.,
        maximum: 1.,
        ..Default::default()
    };
    assert!(doc.create_parameter(parameter).status.is_ok());
    let base = doc.get_mesh(&id(910)).unwrap().base_positions.clone();
    assert!(doc
        .create_binding(zipzi_core::MeshBinding {
            id: id(921),
            mesh_id: id(910),
            axes: vec![zipzi_core::BindingAxis {
                parameter_id: id(920),
                keys: vec![-1., 1.],
                ..Default::default()
            }],
            keyforms: [-1., 1.]
                .into_iter()
                .map(|key| zipzi_core::MeshKeyform {
                    keys: vec![key],
                    positions: base
                        .iter()
                        .map(|p| Vec2::new(p.x + key * p.y * 0.2, p.y))
                        .collect(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        })
        .status
        .is_ok());
    (
        doc,
        png,
        TextureData {
            width,
            height: width,
            rgba: rgba.into(),
            cache_key: "atlas-fixture".into(),
        },
    )
}

fn sample(image: &TextureData, uv: [f64; 2]) -> [f64; 4] {
    let x = uv[0] * image.width as f64 - 0.5;
    let y = (1. - uv[1]) * image.height as f64 - 0.5;
    let pixel = |x: f64, y: f64| {
        let x = x.clamp(0., (image.width - 1) as f64) as usize;
        let y = y.clamp(0., (image.height - 1) as f64) as usize;
        let i = (y * image.width as usize + x) * 4;
        std::array::from_fn::<_, 4, _>(|c| image.rgba[i + c] as f64)
    };
    let a = pixel(x.floor(), y.floor());
    let b = pixel(x.floor() + 1., y.floor());
    let c = pixel(x.floor(), y.floor() + 1.);
    let d = pixel(x.floor() + 1., y.floor() + 1.);
    std::array::from_fn(|i| {
        (a[i] * (1. - x.fract().rem_euclid(1.)) + b[i] * x.fract().rem_euclid(1.))
            * (1. - y.fract().rem_euclid(1.))
            + (c[i] * (1. - x.fract().rem_euclid(1.)) + d[i] * x.fract().rem_euclid(1.))
                * y.fract().rem_euclid(1.)
    })
}
fn assert_appearance(before: &Document, atlas: &TextureData, after: &DocumentSession) {
    assert_eq!(
        after.document().asset_order().len(),
        after.document().mesh_order().len()
    );
    let mut assets = std::collections::HashSet::new();
    for mesh in before
        .mesh_order()
        .iter()
        .map(|id| before.get_mesh(id).unwrap())
    {
        let actual = after
            .document()
            .mesh_order()
            .iter()
            .map(|id| after.document().get_mesh(id).unwrap())
            .find(|m| m.runtime_id == mesh.runtime_id)
            .unwrap();
        assert!(assets.insert(actual.texture_asset_id.clone()));
        assert_eq!(mesh.base_positions, actual.base_positions);
        assert_eq!(mesh.vertex_ids, actual.vertex_ids);
        assert_eq!(mesh.triangles, actual.triangles);
        let image = after.read_texture(&actual.texture_asset_id).unwrap();
        assert!(image.width < atlas.width || image.height < atlas.height);
        let slots: HashMap<_, _> = mesh
            .vertex_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        for face in &mesh.triangles {
            for a in 0..=12 {
                for b in 0..=12 - a {
                    let weights = [a as f64 / 12., b as f64 / 12., (12 - a - b) as f64 / 12.];
                    let uv = |values: &[Vec2]| {
                        let mut p = [0.; 2];
                        for i in 0..3 {
                            p[0] += values[slots[&face[i]]].x as f64 * weights[i];
                            p[1] += values[slots[&face[i]]].y as f64 * weights[i];
                        }
                        p
                    };
                    let expected = sample(atlas, uv(&mesh.uvs));
                    let got = sample(&image, uv(&actual.uvs));
                    for c in 0..4 {
                        assert!(
                            (expected[c] - got[c]).abs() < 0.005,
                            "{}: {:?} != {:?}",
                            mesh.name,
                            expected,
                            got
                        );
                    }
                }
            }
        }
    }
}
fn runtime_package(tmp: &TestDirectory, doc: &Document) -> std::path::PathBuf {
    let moc = zipzi_moc3::encode_moc3(doc).unwrap();
    fs::write(tmp.0.join("model.moc3"), moc.bytes).unwrap();
    let path = tmp.0.join("model.model3.json");
    fs::write(
        &path,
        r#"{"Version":3,"FileReferences":{"Moc":"model.moc3","Textures":["page.png"]}}"#,
    )
    .unwrap();
    path
}
fn save_reopen(tmp: &TestDirectory, session: &mut DocumentSession) {
    let expected: Vec<_> = session
        .document()
        .mesh_order()
        .iter()
        .map(|id| session.document().get_mesh(id).unwrap().clone())
        .collect();
    let pixels: Vec<_> = expected
        .iter()
        .map(|m| session.read_texture(&m.texture_asset_id).unwrap())
        .collect();
    // Extracted drafts must no longer depend on the original page or model files.
    fs::remove_file(tmp.0.join("page.png")).unwrap();
    let captured = session.transfer_snapshot().unwrap();
    let refusal = captured
        .publish(zipzi_project::TransferFormat::Runtime, &tmp.0)
        .unwrap_err();
    assert_eq!(
        refusal.code, "INVALID_DESTINATION",
        "Extracted images must not remove source overwrite protection"
    );
    let exported = tmp.0.join("runtime");
    let result = session.export_package(&exported);
    assert!(
        result.status.is_ok(),
        "Export before save must resolve extracted images: {result:?}"
    );
    let mut reimported = DocumentSession::new();
    let (result, _) = reimported.import_model3_authoring(&exported.join("model.model3.json"));
    assert!(result.status.is_ok(), "{result:?}");
    assert_eq!(reimported.document().asset_order().len(), expected.len());
    let output = tmp.0.join("saved");
    let result = session.save(&output);
    assert!(result.status.is_ok(), "{result:?}");
    let mut reopened = DocumentSession::new();
    assert!(reopened.open(&output).status.is_ok());
    for (mesh, image) in expected.iter().zip(pixels) {
        assert_eq!(reopened.document().get_mesh(&mesh.id).unwrap(), mesh);
        let actual = reopened.read_texture(&mesh.texture_asset_id).unwrap();
        assert_eq!(
            (actual.width, actual.height, actual.rgba),
            (image.width, image.height, image.rgba)
        );
    }
}

#[test]
fn model3_import_unpacks_shared_atlas_and_survives_source_removal_and_save() {
    let tmp = TestDirectory::new();
    let (doc, png, atlas) = fixture(&tmp);
    let path = runtime_package(&tmp, &doc);
    let raw = zipzi_moc3::import_from_model3_file(&path).unwrap();
    let mut imported = DocumentSession::new();
    let (result, _) = imported.import_model3_authoring(&path);
    assert!(result.status.is_ok(), "{result:?}");
    assert_appearance(&raw.document, &atlas, &imported);
    // The page's top-left corner is inside the diamond's bounds, but belongs
    // to another mesh. It must be transparent in the diamond's independent image.
    let mesh = imported
        .document()
        .mesh_order()
        .iter()
        .map(|id| imported.document().get_mesh(id).unwrap())
        .find(|m| m.runtime_id == "Piece0")
        .unwrap();
    let image = imported.read_texture(&mesh.texture_asset_id).unwrap();
    assert_eq!(
        image.rgba[((6 - 2) * image.width as usize + (6 - 2)) * 4 + 3],
        0
    );
    assert_eq!(fs::read(tmp.0.join("page.png")).unwrap(), png);
    save_reopen(&tmp, &mut imported);
}

#[test]
fn bare_moc3_and_legacy_model3_imports_unpack_the_same_pages() {
    let tmp = TestDirectory::new();
    let (doc, _, atlas) = fixture(&tmp);
    let path = runtime_package(&tmp, &doc);
    let raw = zipzi_moc3::import_from_model3_file(&path).unwrap();
    for authoring in [false, true] {
        let mut session = DocumentSession::new();
        let paths = HashMap::from([(0, tmp.0.join("page.png"))]);
        let (result, _) = if authoring {
            session.import_bare_moc3_authoring(&tmp.0.join("model.moc3"), &paths)
        } else {
            session.import_bare_moc3(&tmp.0.join("model.moc3"), &paths)
        };
        assert!(result.status.is_ok(), "{result:?}");
        assert_appearance(&raw.document, &atlas, &session);
    }
    let mut session = DocumentSession::new();
    assert!(session.import_model3(&path).0.status.is_ok());
    assert_appearance(&raw.document, &atlas, &session);
}

#[test]
fn cmo3_import_unpacks_atlas_and_preserves_animated_geometry() {
    let tmp = TestDirectory::new();
    let (doc, png, atlas) = fixture(&tmp);
    let artifact =
        zipzi_cmo3::encode_cmo3(&doc, &|_: &ImageAsset| Ok(png.clone()), &Default::default())
            .unwrap();
    let raw = zipzi_cmo3::import_model(&artifact.bytes).unwrap();
    let path = tmp.0.join("model.cmo3");
    fs::write(&path, artifact.bytes).unwrap();
    let mut imported = DocumentSession::new();
    imported.import_cubism_authoring(&path).unwrap();
    assert_appearance(&raw.document, &atlas, &imported);
    for value in [-1., -0.3, 0., 0.6, 1.] {
        let mut expected = zipzi_core::DrawableFrame::default();
        let mut actual = zipzi_core::DrawableFrame::default();
        let preview = HashMap::from([(id(920), value)]);
        assert!(zipzi_core::evaluation::evaluate_frame(&doc, &preview, &mut expected).is_ok());
        assert!(
            zipzi_core::evaluation::evaluate_frame(imported.document(), &preview, &mut actual)
                .is_ok()
        );
        for (a, b) in expected.drawables.iter().zip(&actual.drawables) {
            assert_eq!(a.positions, b.positions);
            assert_eq!(a.opacity, b.opacity);
        }
    }
    save_reopen(&tmp, &mut imported);
}

#[test]
fn atlas_extraction_budget_failure_does_not_replace_the_current_session() {
    let tmp = TestDirectory::new();
    let (doc, _, _) = fixture(&tmp);
    let path = runtime_package(&tmp, &doc);
    let mut session = DocumentSession::from_authoring_document(doc.clone());
    let budget = ImageMemoryBudget::new(128);
    session.set_image_memory_budget(budget.clone()).unwrap();
    let (result, _) = session.import_model3_authoring(&path);
    assert!(!result.status.is_ok());
    assert!(session.document().same_content(&doc));
    assert_eq!(budget.usage().total_bytes(), 0);
}

#[test]
fn single_mesh_atlas_is_unpacked_for_runtime_and_cmo3_import() {
    let tmp = TestDirectory::new();
    let (mut doc, png, atlas) = fixture(&tmp);
    assert!(doc.erase_object(&id(911)).status.is_ok());
    let path = runtime_package(&tmp, &doc);
    let raw = zipzi_moc3::import_from_model3_file(&path).unwrap();
    let mut session = DocumentSession::new();
    assert!(session.import_model3_authoring(&path).0.status.is_ok());
    assert_appearance(&raw.document, &atlas, &session);
    let artifact =
        zipzi_cmo3::encode_cmo3(&doc, &|_: &ImageAsset| Ok(png.clone()), &Default::default())
            .unwrap();
    let raw = zipzi_cmo3::import_model(&artifact.bytes).unwrap();
    assert_eq!(
        raw.atlas_assets.len(),
        1,
        "The CMO3 atlas input must be distinguished from original artwork"
    );
    let path = tmp.0.join("single.cmo3");
    fs::write(&path, artifact.bytes).unwrap();
    session.import_cubism_authoring(&path).unwrap();
    assert_appearance(&raw.document, &atlas, &session);
}

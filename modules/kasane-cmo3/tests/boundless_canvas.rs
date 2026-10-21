mod common;
use common::*;
use kasane_cmo3::*;
use kasane_core::{types::*, Document, DrawableFrame};

fn field<'a, 'b>(n: roxmltree::Node<'a, 'b>, name: &str) -> roxmltree::Node<'a, 'b> {
    n.children()
        .find(|n| n.attribute("xs.n") == Some(name))
        .unwrap()
}
fn number(n: roxmltree::Node<'_, '_>, name: &str) -> f32 {
    field(n, name).text().unwrap().parse().unwrap()
}
fn canvas(bytes: &[u8]) -> Canvas {
    let entries = archive(bytes);
    let xml = roxmltree::Document::parse(std::str::from_utf8(&entries.last().unwrap().3).unwrap())
        .unwrap();
    let c = xml
        .descendants()
        .find(|n| n.tag_name().name() == "CImageCanvas")
        .unwrap();
    let mut result = Canvas::new(
        number(c, "pixelWidth"),
        number(c, "pixelHeight"),
        Vec2::default(),
        1.,
    );
    if let Some(info) = xml
        .descendants()
        .find(|n| n.tag_name().name() == "CModelInfo")
    {
        result.pixels_per_unit = number(info, "pixelsPerUnit");
        let origin = field(info, "originInPixels");
        result.origin = Vec2::new(number(origin, "x"), number(origin, "y"));
    }
    result
}
fn frame(doc: &Document, value: f32) -> DrawableFrame {
    let mut result = DrawableFrame::default();
    let pose = std::collections::HashMap::from([(PARAM.into(), value)]);
    assert!(
        kasane_core::evaluation::evaluate_frame_including_hidden(doc, &pose, &mut result).is_ok()
    );
    result
}
fn fixture_with_hierarchy(kind: u8, flag: u8) -> (Document, Vec<u8>) {
    let (mut doc, png) = fixture();
    let c = Canvas::with_flag(100., 100., Vec2::new(130.25, -77.5), 100., flag);
    assert!(doc.replace_model_setup(c).status.is_ok());
    let mut parameter = doc.get_parameter(PARAM).unwrap().clone();
    parameter.repeat = false;
    assert!(doc.replace_parameter(parameter).status.is_ok());
    if kind != 0 {
        let root = "00000000-0000-4000-8000-000000000020";
        let child = "00000000-0000-4000-8000-000000000021";
        let data = if kind == 1 {
            TransformData::Rotation(RotationTransform {
                base_angle: 10.,
                pose: RotationPose {
                    origin: PreciseVec2::new(-900., 600.),
                    angle: 25.,
                    scale: 2.,
                    ..Default::default()
                },
            })
        } else {
            TransformData::Warp(WarpTransform {
                rows: 1,
                columns: 1,
                quad: true,
                bezier: None,
                points: vec![
                    Vec2::new(-900., 500.),
                    Vec2::new(-300., 500.),
                    Vec2::new(-900., 900.),
                    Vec2::new(-300., 900.),
                ],
            })
        };
        assert!(doc
            .create_transform(Transform {
                id: root.into(),
                runtime_id: "Root".into(),
                data,
                ..Default::default()
            })
            .status
            .is_ok());
        assert!(doc
            .create_transform(Transform {
                id: child.into(),
                runtime_id: "Child".into(),
                parent_id: Some(root.into()),
                data: TransformData::Rotation(RotationTransform {
                    base_angle: 0.,
                    pose: RotationPose {
                        origin: PreciseVec2::new(0.5, 0.5),
                        angle: -15.,
                        scale: 0.5,
                        ..Default::default()
                    },
                }),
                ..Default::default()
            })
            .status
            .is_ok());
        let mut mesh = doc.get_mesh(MESH).unwrap().clone();
        mesh.deformer_id = child.into();
        mesh.base_positions = vec![
            Vec2::new(-0.2, -0.2),
            Vec2::new(0.2, -0.2),
            Vec2::new(-0.2, 0.2),
        ];
        assert!(doc.replace_mesh(mesh.clone()).status.is_ok());
        let mut binding = doc.binding_for_mesh(MESH).unwrap().clone();
        for form in &mut binding.keyforms {
            form.positions = mesh
                .base_positions
                .iter()
                .map(|p| Vec2::new(p.x + form.keys[0] * 0.05, p.y))
                .collect();
        }
        assert!(doc.replace_binding(binding).status.is_ok());
    }
    let driver = "00000000-0000-4000-8000-000000000030";
    let table = "00000000-0000-4000-8000-000000000031";
    assert!(doc
        .create_parameter(Parameter {
            id: driver.into(),
            runtime_id: "Blend".into(),
            kind: ParameterKind::BlendShape,
            minimum: 0.,
            maximum: 1.,
            default_value: 0.5,
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: table.into(),
            parameter_id: driver.into(),
            keys: vec![0., 1.],
            base_key_idx: 0,
        })
        .status
        .is_ok());
    assert!(doc
        .create_blend_binding(BlendShapeBinding {
            id: "00000000-0000-4000-8000-000000000032".into(),
            target_id: MESH.into(),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: table.into(),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Mesh(vec![
                DeltaMeshKeyform {
                    positions: vec![Vec2::default(); 3],
                    ..Default::default()
                },
                DeltaMeshKeyform {
                    positions: vec![Vec2::new(0.1, 0.2); 3],
                    ..Default::default()
                },
            ]),
        })
        .status
        .is_ok());
    (doc, png)
}

#[test]
fn boundless_pair_preserves_root_and_nested_geometry_for_both_canvas_directions() {
    for kind in 0..3 {
        for flag in [0, 1] {
            let (doc, png) = fixture_with_hierarchy(kind, flag);
            let mut doc = doc;
            // Imported reference dimensions must not determine the export rectangle.
            assert!(doc
                .replace_source_canvas(Some(Canvas::new(8192., 4096., Vec2::new(500., 300.), 100.)))
                .status
                .is_ok());
            let before = doc.clone();
            let pair = encode_cubism(
                &doc,
                &|_: &ImageAsset| Ok(png.clone()),
                &CubismOptions::default(),
            )
            .unwrap();
            assert!(!pair.report.has_loss());
            let c = canvas(&pair.cmo3.bytes);
            let a = canvas(&pair.can3.bytes);
            assert_eq!((c.width, c.height), (a.width, a.height));
            assert!(c.width >= 256. && c.height >= 256.);
            assert_eq!(
                c.pixels_per_unit,
                doc.source_canvas().map_or(1., |c| c.pixels_per_unit)
            );
            let bounds = frame(&doc, 0.);
            for p in &bounds.drawables[0].positions {
                let x = p.x + c.origin.x;
                let y = c.origin.y - p.y;
                assert!(x >= 49. && x <= c.width - 49., "{x} / {}", c.width);
                assert!(y >= 49. && y <= c.height - 49., "{y} / {}", c.height);
            }
            let standalone = encode_can3(
                &doc,
                &Can3Options {
                    animation_name: Cmo3Options::default().model_name,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(standalone.bytes, pair.can3.bytes);
            assert_eq!(
                encode_cmo3(
                    &doc,
                    &|_: &ImageAsset| Ok(png.clone()),
                    &Cmo3Options::default()
                )
                .unwrap()
                .bytes,
                pair.cmo3.bytes
            );
            assert_eq!(doc.revision(), before.revision());
            assert!(doc.same_content(&before));

            // Changing only imported framing must not move any exported forms.
            let mut finite = doc.clone();
            let _space = doc.export_settings();
            let old =
                Canvas::with_flag(1024., 1024., Vec2::new(130.25, -77.5), c.pixels_per_unit, 1);
            // Fractional authored origins are valid; generated export origins are integral.
            assert!(finite.replace_model_setup(old).status.is_ok());
            let fixed = encode_cmo3(
                &finite,
                &|_: &ImageAsset| Ok(png.clone()),
                &Cmo3Options::default(),
            )
            .unwrap();
            assert_eq!(canvas(&fixed.bytes), c);
            let entries = archive(&pair.cmo3.bytes);
            let original = archive(&fixed.bytes);
            let xml = roxmltree::Document::parse(
                std::str::from_utf8(&entries.last().unwrap().3).unwrap(),
            )
            .unwrap();
            let baseline = roxmltree::Document::parse(
                std::str::from_utf8(&original.last().unwrap().3).unwrap(),
            )
            .unwrap();
            let forms = |x: &roxmltree::Document<'_>| {
                x.descendants()
                    .filter(|n| {
                        n.has_attribute("xs.id")
                            && matches!(
                                n.tag_name().name(),
                                "CArtMeshForm" | "CWarpDeformerForm" | "CRotationDeformerForm"
                            )
                    })
                    .count()
            };
            assert_eq!(forms(&xml), forms(&baseline));
            for (current, previous) in
                xml.descendants()
                    .filter(|n| n.has_attribute("xs.id") && n.tag_name().name() == "CArtMeshForm")
                    .zip(baseline.descendants().filter(|n| {
                        n.has_attribute("xs.id") && n.tag_name().name() == "CArtMeshForm"
                    }))
            {
                let parse = |n| {
                    field(n, "positions")
                        .text()
                        .unwrap()
                        .split_whitespace()
                        .map(|v| v.parse::<f32>().unwrap())
                        .collect::<Vec<_>>()
                };
                assert_eq!(parse(current), parse(previous));
            }

            // Optional installed-Cubism probe consumes these fixtures and compares
            // all vertices at multiple parameter poses, including nested deformers.
            let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/cmo3-acceptance");
            std::fs::create_dir_all(&directory).unwrap();
            let name = format!("boundless-{kind}-{flag}");
            std::fs::write(directory.join(format!("{name}.cmo3")), pair.cmo3.bytes).unwrap();
            let cases: Vec<_> = [-1., 0., 1.].into_iter().map(|value| {
                let frame = frame(&doc, value);
                let positions: std::collections::BTreeMap<_, Vec<_>> = frame.drawables.iter().map(|d| (d.runtime_id.clone(), d.positions.iter().map(|p| [p.x + c.origin.x, c.origin.y - p.y]).collect())).collect();
                serde_json::json!({"parameters":{"ParamX":value,"Blend":0.5}, "positions":positions, "render_order":["ArtMesh"]})
            }).collect();
            std::fs::write(
                directory.join(format!("{name}.json")),
                serde_json::to_vec(&cases).unwrap(),
            )
            .unwrap();
            // Use the linked name recorded in CAN3 for the independent load test.
            let linked = directory.join(&name);
            std::fs::create_dir_all(&linked).unwrap();
            std::fs::write(
                linked.join("model.cmo3"),
                std::fs::read(directory.join(format!("{name}.cmo3"))).unwrap(),
            )
            .unwrap();
            std::fs::write(linked.join("model.can3"), pair.can3.bytes).unwrap();
        }
    }
}

#[test]
fn empty_model_spaces_and_invisible_meshes_get_finite_export_bounds() {
    for origin in [Vec2::default(), Vec2::new(17., -23.)] {
        let mut empty = Document::new();
        assert!(empty
            .initialize(DOC, Canvas::new(100., 100., origin, 1.))
            .is_ok());
        let pair = encode_cubism(
            &empty,
            &|_: &ImageAsset| panic!("empty document read an asset"),
            &CubismOptions::default(),
        )
        .unwrap();
        assert_eq!(
            (
                canvas(&pair.cmo3.bytes).width,
                canvas(&pair.can3.bytes).height
            ),
            (256., 256.)
        );
    }
    let (mut doc, png) = fixture_with_hierarchy(0, 1);
    let mut hidden = doc.get_mesh(MESH).unwrap().clone();
    hidden.id = "00000000-0000-4000-8000-000000000025".into();
    hidden.runtime_id = "Hidden".into();
    hidden.enabled = false;
    hidden.base_positions = hidden
        .base_positions
        .iter()
        .map(|p| Vec2::new(p.x + 2000., p.y + 1000.))
        .collect();
    assert!(doc.create_mesh(hidden).status.is_ok());
    let pair = encode_cubism(
        &doc,
        &|_: &ImageAsset| Ok(png.clone()),
        &CubismOptions::default(),
    )
    .unwrap();
    let c = canvas(&pair.cmo3.bytes);
    assert!(c.width > 2400. && c.height > 1200.);
    assert_eq!(c.width, canvas(&pair.can3.bytes).width);
}

#[test]
fn overflowing_canvas_is_rejected_before_reading_assets() {
    let (mut doc, _) = fixture_with_hierarchy(0, 1);
    let mut c = doc.export_settings();
    c.bounds = Some([-f32::MAX, 0., f32::MAX, 100.]);
    assert!(doc.replace_export_settings(c).status.is_ok());
    assert!(inspect_cubism(&doc).is_err());
    assert!(encode_cubism(
        &doc,
        &|_: &ImageAsset| panic!("invalid canvas read an asset"),
        &CubismOptions::default()
    )
    .is_err());
}

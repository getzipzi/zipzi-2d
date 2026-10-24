mod common;
use common::*;
use zipzi_cmo3::*;
use zipzi_core::{types::*, Document};

fn encode(doc: &Document, png: &[u8], key: i32) -> Cmo3Artifact {
    encode_cmo3(
        doc,
        &|_: &ImageAsset| Ok(png.to_vec()),
        &Cmo3Options {
            obfuscation_key: key,
            model_name: "模型".repeat(100),
            ..Default::default()
        },
    )
    .unwrap()
}
fn field<'a, 'input>(node: roxmltree::Node<'a, 'input>, name: &str) -> roxmltree::Node<'a, 'input> {
    node.children()
        .find(|n| n.attribute("xs.n") == Some(name))
        .unwrap()
}

#[test]
fn runtime_model_name_maps_to_native_label_but_unknown_metadata_stays_strict() {
    let (mut doc, png) = fixture();
    let mut settings = doc.model3_settings().clone();
    settings
        .extensions
        .insert("Name".into(), serde_json::json!("Original label"));
    assert!(doc.set_model3_settings(settings.clone()).status.is_ok());
    for explicit in [None, Some("Chosen label")] {
        let mut options = Cmo3Options::default();
        if let Some(name) = explicit {
            options.model_name = name.into();
        }
        let artifact = encode_cmo3(&doc, &|_: &ImageAsset| Ok(png.clone()), &options).unwrap();
        assert!(!artifact.report.has_loss());
        let imported = import_model(&artifact.bytes).unwrap();
        let expected = serde_json::json!(explicit.unwrap_or("Original label"));
        assert_eq!(
            imported.document.model3_settings().extensions.get("Name"),
            Some(&expected)
        );
        let reexported = encode_cmo3(
            &imported.document,
            &|asset: &ImageAsset| {
                Ok(imported.images[&asset.content.as_ref().unwrap().source].clone())
            },
            &Cmo3Options::default(),
        )
        .unwrap();
        let reopened = import_model(&reexported.bytes).unwrap();
        assert_eq!(
            reopened.document.model3_settings().extensions.get("Name"),
            Some(&expected)
        );
        let entries = archive(&artifact.bytes);
        let xml = String::from_utf8(
            entries
                .iter()
                .find(|e| e.0 == "main.xml")
                .unwrap()
                .3
                .clone(),
        )
        .unwrap();
        let parsed = roxmltree::Document::parse(&xml).unwrap();
        let model = parsed
            .descendants()
            .find(|n| n.has_tag_name("CModelSource") && n.attribute("xs.ref").is_none())
            .unwrap();
        assert_eq!(
            field(model, "name").text(),
            Some(explicit.unwrap_or("Original label"))
        );
    }
    settings
        .extensions
        .insert("Unknown".into(), serde_json::json!(true));
    assert!(doc.set_model3_settings(settings).status.is_ok());
    assert!(matches!(
        encode_cmo3(
            &doc,
            &|_: &ImageAsset| panic!("metadata rejection must precede image I/O"),
            &Cmo3Options::default()
        ),
        Err(Cmo3Error::Unsupported(_))
    ));
}

fn exported_origin(xml: &roxmltree::Document<'_>) -> Vec2 {
    let info = xml
        .descendants()
        .find(|n| n.tag_name().name() == "CModelInfo")
        .unwrap();
    let origin = field(info, "originInPixels");
    Vec2::new(
        field(origin, "x").text().unwrap().parse().unwrap(),
        field(origin, "y").text().unwrap().parse().unwrap(),
    )
}

fn assert_translated_positions(
    xml: &roxmltree::Document<'_>,
    node: roxmltree::Node<'_, '_>,
    expected: &[f32],
) {
    let origin = exported_origin(xml);
    let actual: Vec<f32> = node
        .text()
        .unwrap()
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    assert_eq!(actual.len(), expected.len());
    for (i, (&actual, &source)) in actual.iter().zip(expected).enumerate() {
        let expected = if i % 2 == 0 {
            origin.x + source
        } else {
            origin.y - source
        };
        assert!((actual - expected).abs() < 0.0001, "{actual} vs {expected}");
    }
}

#[test]
fn self_contained_graph_and_signed_xor_keys() {
    let (d, png) = fixture();
    let original = d.clone();
    let revision = d.revision();
    for key in [0, 0x4b617361, -1, i32::MIN + 91] {
        let artifact = encode(&d, &png, key);
        assert!(!artifact.report.has_loss());
        let entries = archive(&artifact.bytes);
        let (_, tag, compression, xml) = entries.last().unwrap();
        assert_eq!(tag, "main_xml");
        assert_eq!(*compression, 33);
        assert!(entries
            .iter()
            .take(entries.len() - 1)
            .all(|e| e.2 == 16 && e.1.is_empty()));
        assert!(entries.iter().any(|e| e.3 == png));
        let text = std::str::from_utf8(xml).unwrap();
        let xml = roxmltree::Document::parse(text).unwrap();
        assert_eq!(
            xml.root_element().attribute("fileFormatVersion"),
            Some("503000000")
        );
        let ids: std::collections::HashMap<_, _> = xml
            .descendants()
            .filter_map(|n| n.attribute("xs.id").map(|id| (id, n)))
            .collect();
        for node in xml.descendants() {
            if let Some(id) = node.attribute("xs.ref") {
                assert_eq!(ids[id].tag_name(), node.tag_name());
            }
            if let Some(count) = node.attribute("count") {
                let expected = count.parse::<usize>().unwrap();
                let actual = if node.tag_name().name().ends_with("-array") {
                    node.text().unwrap_or("").split_whitespace().count()
                } else {
                    node.children().filter(|n| n.is_element()).count()
                };
                assert_eq!(expected, actual);
            }
            if node.tag_name().name() == "file" {
                if let Some(path) = node.attribute("path") {
                    assert!(entries.iter().any(|e| e.0 == path));
                }
            }
        }
        let mesh = xml
            .descendants()
            .find(|n| n.tag_name().name() == "CArtMeshSource" && n.has_attribute("xs.id"))
            .unwrap();
        assert_eq!(field(mesh, "indices").text(), Some("0 2 1"));
        assert_eq!(field(mesh, "uvs").text(), Some("0 1 1 1 0 0"));
        let name = xml.descendants().find(|n| {
            n.attribute("xs.n") == Some("localName")
                && n.text() == Some(d.get_mesh(MESH).unwrap().name.as_str())
        });
        assert!(name.is_some());
        let forms: Vec<_> = xml
            .descendants()
            .filter(|n| n.tag_name().name() == "CArtMeshForm" && n.has_attribute("xs.id"))
            .collect();
        assert_eq!(forms.len(), 3);
        assert_translated_positions(
            &xml,
            field(forms[0], "positions"),
            &[4., 12., 100., 12., 4., 60.],
        );
        assert_eq!(
            xml.descendants()
                .filter(
                    |n| n.tag_name().name() == "KeyformBindingSource" && n.has_attribute("xs.id")
                )
                .count(),
            1
        );
    }
    assert_eq!(d.revision(), revision);
    assert!(original.same_content(&d));
}

#[test]
fn reproducible_and_resource_errors() {
    let (d, png) = fixture();
    assert_eq!(encode(&d, &png, 1).bytes, encode(&d, &png, 1).bytes);
    let error = encode_cmo3(
        &d,
        &|_: &ImageAsset| Ok(b"invalid PNG".to_vec()),
        &Default::default(),
    )
    .unwrap_err();
    assert!(matches!(error, Cmo3Error::Asset { .. }));
    let error = encode_cmo3(
        &d,
        &|_: &ImageAsset| {
            Err(Cmo3Error::Asset {
                asset_id: ASSET.into(),
                message: "missing".into(),
            })
        },
        &Default::default(),
    )
    .unwrap_err();
    assert!(matches!(error, Cmo3Error::Asset { .. }));
    assert!(matches!(
        inspect_export(&Document::new()),
        Err(Cmo3Error::InvalidDocument(_))
    ));
}

#[test]
fn loss_is_rejected_before_reading_assets() {
    let (mut d, png) = fixture();
    let mut m = d.get_mesh(MESH).unwrap().clone();
    m.draw_order = Some(12.25);
    assert!(d.replace_mesh(m).status.is_ok());
    let error = encode_cmo3(
        &d,
        &|_: &ImageAsset| panic!("Must inspect losses before resource IO"),
        &Default::default(),
    )
    .unwrap_err();
    let Cmo3Error::Unsupported(report) = error else {
        panic!("Unexpected error")
    };
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == "FRACTIONAL_DRAW_ORDER"));
    let artifact = encode_cmo3(
        &d,
        &|_: &ImageAsset| Ok(png.clone()),
        &Cmo3Options {
            loss_policy: LossPolicy::Allow,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(artifact.report.has_loss());
}

#[test]
fn morph_forms_use_the_ordinary_default_pose_and_keep_constraints() {
    let (mut d, png) = fixture();
    let driver = "00000000-0000-4000-8000-000000000010";
    let table = "00000000-0000-4000-8000-000000000011";
    let constraint = "00000000-0000-4000-8000-000000000012";
    assert!(d
        .create_parameter(Parameter {
            id: driver.into(),
            runtime_id: "Blend".into(),
            kind: ParameterKind::BlendShape,
            minimum: 0.,
            maximum: 1.,
            default_value: 0.,
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(d
        .create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: table.into(),
            parameter_id: driver.into(),
            keys: vec![0., 1.],
            base_key_idx: 0
        })
        .status
        .is_ok());
    assert!(d
        .create_blend_constraint(BlendShapeConstraint {
            samples: Default::default(),
            id: constraint.into(),
            parameter_id: PARAM.into(),
            keys: vec![-1., 1.],
            weights: vec![0., 1.]
        })
        .status
        .is_ok());
    assert!(d
        .create_blend_binding(BlendShapeBinding {
            id: "00000000-0000-4000-8000-000000000013".into(),
            target_id: MESH.into(),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: table.into(),
            constraint_ids: vec![constraint.into()],
            keyforms: DeltaKeyforms::Mesh(vec![
                DeltaMeshKeyform {
                    positions: vec![Vec2::default(); 3],
                    ..Default::default()
                },
                DeltaMeshKeyform {
                    positions: vec![Vec2::new(2., 3.); 3],
                    opacity: Some(-0.25),
                    draw_order: Some(10.),
                    ..Default::default()
                }
            ])
        })
        .status
        .is_ok());
    let artifact = encode(&d, &png, 1);
    write_probe_artifact("advanced-morph", &artifact);
    let entries = archive(&artifact.bytes);
    let text = std::str::from_utf8(&entries.last().unwrap().3).unwrap();
    let xml = roxmltree::Document::parse(text).unwrap();
    let morph = xml
        .descendants()
        .find(|n| n.tag_name().name() == "KeyFormMorphTarget")
        .unwrap();
    let uuid = field(morph, "keyformGuid").attribute("uuid").unwrap();
    let form = xml
        .descendants()
        .find(|n| {
            n.tag_name().name() == "CArtMeshForm"
                && n.has_attribute("xs.id")
                && field(field(field(*n, "super"), "super"), "guid").attribute("uuid") == Some(uuid)
        })
        .unwrap();
    assert_translated_positions(
        &xml,
        field(form, "positions"),
        &[10., 15., 106., 15., 10., 63.],
    );
    assert_eq!(field(field(form, "super"), "opacity").text(), Some("0.75"));
    assert_eq!(field(field(form, "super"), "drawOrder").text(), Some("10"));
    assert_eq!(
        xml.descendants()
            .filter(|n| n.tag_name().name() == "MorphTargetBlendWeightConstraint")
            .count(),
        2
    );
    assert!(text.contains("<?version KeyFormMorphTarget:2?>"));

    let mut canvas = d.source_canvas().unwrap();
    canvas.flag = 0;
    assert!(d.replace_model_setup(canvas).status.is_ok());
    let reversed = encode(&d, &png, 1);
    write_probe_artifact("reversed-morph", &reversed);
    let entries = archive(&reversed.bytes);
    let xml = roxmltree::Document::parse(std::str::from_utf8(&entries.last().unwrap().3).unwrap())
        .unwrap();
    let morph = xml
        .descendants()
        .find(|n| n.tag_name().name() == "KeyFormMorphTarget")
        .unwrap();
    let uuid = field(morph, "keyformGuid").attribute("uuid").unwrap();
    let form = xml
        .descendants()
        .find(|n| {
            n.tag_name().name() == "CArtMeshForm"
                && n.has_attribute("xs.id")
                && field(field(field(*n, "super"), "super"), "guid").attribute("uuid") == Some(uuid)
        })
        .unwrap();
    assert_translated_positions(
        &xml,
        field(form, "positions"),
        &[10., 15., 106., 15., 10., 63.],
    );
}

#[test]
fn offscreen_material_and_keyed_glue_remain_editable() {
    let (mut d, png) = fixture();
    let part = "00000000-0000-4000-8000-000000000020";
    let other = "00000000-0000-4000-8000-000000000021";
    assert!(d
        .create_part(Part {
            id: part.into(),
            runtime_id: "Part".into(),
            name: "Part".into(),
            draw_order: 500.,
            ..Default::default()
        })
        .status
        .is_ok());
    let mut mesh = d.get_mesh(MESH).unwrap().clone();
    mesh.part_id = part.into();
    mesh.raw_blend_mode = Some(3 | (1 << 8));
    assert!(d.replace_mesh(mesh.clone()).status.is_ok());
    mesh.id = other.into();
    mesh.runtime_id = "Other".into();
    assert!(d.create_mesh(mesh).status.is_ok());
    assert!(d
        .create_glue(Glue {
            id: "00000000-0000-4000-8000-000000000022".into(),
            runtime_id: "Glue".into(),
            name: "Glue".into(),
            mesh_a_id: MESH.into(),
            mesh_b_id: other.into(),
            pairs: vec![GlueVertexPair {
                vertex_a: Some(100),
                vertex_b: Some(7),
                weight_a: 0.25,
                weight_b: 0.75,
                ..Default::default()
            }],
            intensity: 1.,
            binding: Some(GlueBinding {
                axes: vec![BindingAxis {
                    id: Default::default(),
                    samples: Default::default(),
                    parameter_id: PARAM.into(),
                    keys: vec![-1., 1.]
                }],
                keyforms: vec![
                    GlueKeyform {
                        samples: Default::default(),
                        intensity: 0.
                    },
                    GlueKeyform {
                        samples: Default::default(),
                        intensity: 1.
                    }
                ]
            })
        })
        .status
        .is_ok());
    assert!(d
        .create_offscreen(Offscreen {
            id: "00000000-0000-4000-8000-000000000023".into(),
            runtime_id: "Offscreen".into(),
            name: "Offscreen".into(),
            part_id: part.into(),
            blend_mode: 10 | (4 << 8),
            flags: 12,
            masks: vec![other.into()],
            keyforms: vec![OffscreenKeyform {
                opacity: 0.6,
                multiply: Some([0.5, 0.75, 1.]),
                screen: None
            }],
            ..Default::default()
        })
        .status
        .is_ok());
    let artifact = encode(&d, &png, 1);
    write_probe_artifact("advanced-offscreen-glue", &artifact);
    let entries = archive(&artifact.bytes);
    let text = std::str::from_utf8(&entries.last().unwrap().3).unwrap();
    let xml = roxmltree::Document::parse(text).unwrap();
    let part_node = xml
        .descendants()
        .find(|n| {
            n.tag_name().name() == "CPartSource"
                && n.has_attribute("xs.id")
                && field(*n, "id").attribute("idstr") == Some("Part")
        })
        .unwrap();
    assert_eq!(field(part_node, "useOffscreen").text(), Some("true"));
    assert_eq!(field(part_node, "invertClippingMask").text(), Some("true"));
    assert_eq!(
        field(part_node, "colorComposition").attribute("v"),
        Some("SCREEN")
    );
    assert_eq!(
        field(part_node, "alphaComposition").attribute("v"),
        Some("DISJOINT")
    );
    let mesh_source = xml
        .descendants()
        .find(|n| {
            n.tag_name().name() == "CArtMeshSource"
                && n.has_attribute("xs.id")
                && field(field(*n, "super"), "id").attribute("idstr") == Some("ArtMesh")
        })
        .unwrap();
    // Editable source geometry must not have Glue applied twice.
    assert_translated_positions(
        &xml,
        field(mesh_source, "positions"),
        &[8., 12., 104., 12., 8., 60.],
    );
    let glue = xml
        .descendants()
        .find(|n| n.tag_name().name() == "CGlueSource" && n.has_attribute("xs.id"))
        .unwrap();
    assert_eq!(
        field(glue, "bindVertexUids").tag_name().name(),
        "long-array"
    );
    assert_eq!(field(glue, "bindVertexUids").text(), Some("0 1"));
    assert_eq!(
        xml.descendants()
            .filter(|n| n.tag_name().name() == "CGlueForm" && n.has_attribute("xs.id"))
            .count(),
        2
    );
}

#[test]
fn invalid_xml_text_is_reported() {
    let (d, png) = fixture();
    let error = encode_cmo3(
        &d,
        &|_: &ImageAsset| Ok(png.clone()),
        &Cmo3Options {
            model_name: "bad\0name".into(),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(matches!(error, Cmo3Error::InvalidDocument(_)));
}

fn write_probe_artifact(name: &str, artifact: &Cmo3Artifact) {
    let target =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/cmo3-acceptance");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join(format!("{name}.cmo3")), &artifact.bytes).unwrap();
}

#[test]
fn parts_panel_reverses_nested_render_ties_without_reversing_draw_order_values() {
    use zipzi_core::draw_order::DrawOrderGroup;
    let (mut d, png) = fixture();
    let mut parameter = d.get_parameter(PARAM).unwrap().clone();
    parameter.repeat = false;
    assert!(d.replace_parameter(parameter).status.is_ok());
    let group = "00000000-0000-4000-8000-000000000030";
    let folder = "00000000-0000-4000-8000-000000000031";
    let other = "00000000-0000-4000-8000-000000000032";
    let outer = "00000000-0000-4000-8000-000000000033";
    let floor = "00000000-0000-4000-8000-000000000034";
    for (id, parent) in [(group, ""), (folder, group)] {
        assert!(d
            .create_part(Part {
                id: id.into(),
                runtime_id: id.into(),
                parent_id: parent.into(),
                draw_order: 5.,
                ..Default::default()
            })
            .status
            .is_ok());
    }
    let mut mesh = d.get_mesh(MESH).unwrap().clone();
    mesh.part_id = folder.into();
    mesh.draw_order = Some(5.);
    assert!(d.replace_mesh(mesh.clone()).status.is_ok());
    for (id, part, order) in [(other, folder, 5.), (outer, group, 20.), (floor, "", 5.)] {
        mesh.id = id.into();
        mesh.runtime_id = id.into();
        mesh.part_id = part.into();
        mesh.draw_order = Some(order);
        assert!(d.create_mesh(mesh.clone()).status.is_ok());
    }
    let mut binding = d.binding_for_mesh(MESH).unwrap().clone();
    for (form, order) in binding.keyforms.iter_mut().zip([0., 5., 10.]) {
        form.draw_order = Some(order);
    }
    assert!(d.replace_binding(binding).status.is_ok());
    assert!(d
        .replace_draw_order_groups(vec![
            DrawOrderGroup {
                owner: "".into(),
                items: vec![floor.into(), group.into()],
                min_order: 0,
                max_order: 1000
            },
            DrawOrderGroup {
                owner: group.into(),
                items: vec![MESH.into(), other.into(), outer.into()],
                min_order: 0,
                max_order: 1000
            },
        ])
        .status
        .is_ok());
    let artifact = encode(&d, &png, 1);
    write_probe_artifact("nested-draw-order", &artifact);
    let entries = archive(&artifact.bytes);
    let text = std::str::from_utf8(&entries.last().unwrap().3).unwrap();
    let xml = roxmltree::Document::parse(text).unwrap();
    for (id, expected) in [
        ("__RootPart__", vec![group, floor]),
        (group, vec![outer, folder]),
        (folder, vec![other, MESH]),
    ] {
        let part = xml
            .descendants()
            .find(|n| {
                n.tag_name().name() == "CPartSource"
                    && n.has_attribute("xs.id")
                    && field(*n, "id").attribute("idstr") == Some(id)
            })
            .unwrap();
        let actual: Vec<_> = field(part, "_childGuids")
            .children()
            .filter_map(|n| n.attribute("uuid"))
            .collect();
        assert_eq!(actual, expected);
    }
    let orders: Vec<_> = xml
        .descendants()
        .filter(|n| n.tag_name().name() == "CArtMeshForm" && n.has_attribute("xs.id"))
        .map(|n| field(n, "super"))
        .map(|n| field(n, "drawOrder").text().unwrap())
        .collect();
    assert_eq!(&orders[..3], &["0", "5", "10"]);

    // The optional native probe also checks animated numeric order against the
    // tree tie order, including a non-group folder inside a drawing group.
    let mut cases = Vec::new();
    for value in [-1., 0., 1.] {
        let mut frame = zipzi_core::DrawableFrame::default();
        let pose = std::collections::HashMap::from([(PARAM.into(), value)]);
        assert!(
            zipzi_core::evaluation::evaluate_frame_including_hidden(&d, &pose, &mut frame).is_ok()
        );
        let canvas = Canvas {
            origin: exported_origin(&xml),
            ..d.source_canvas().unwrap()
        };
        let positions: std::collections::BTreeMap<_, Vec<_>> = frame
            .drawables
            .iter()
            .map(|mesh| {
                (
                    mesh.runtime_id.clone(),
                    mesh.positions
                        .iter()
                        .map(|p| [p.x + canvas.origin.x, canvas.origin.y - p.y])
                        .collect(),
                )
            })
            .collect();
        frame.drawables.sort_by_key(|mesh| mesh.render_order);
        let render_order: Vec<_> = frame
            .drawables
            .iter()
            .map(|mesh| &mesh.runtime_id)
            .collect();
        cases.push(serde_json::json!({"parameters":{"ParamX":value},"positions":positions,"render_order":render_order}));
    }
    let target = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/cmo3-acceptance/nested-draw-order.json");
    std::fs::write(target, serde_json::to_vec(&cases).unwrap()).unwrap();
}

#[test]
fn canvas_reversal_matches_runtime_geometry_texture_uvs_and_triangle_orientation() {
    let (mut doc, png) = fixture();
    let mut canvas = doc.source_canvas().unwrap();
    canvas.flag = 0;
    assert!(doc.replace_model_setup(canvas).status.is_ok());
    let before = doc.clone();
    let artifact = encode(&doc, &png, 1);
    let entries = archive(&artifact.bytes);
    let xml = roxmltree::Document::parse(std::str::from_utf8(&entries.last().unwrap().3).unwrap())
        .unwrap();
    let mesh = xml
        .descendants()
        .find(|n| n.tag_name().name() == "CArtMeshSource" && n.has_attribute("xs.id"))
        .unwrap();
    assert_eq!(field(mesh, "indices").text(), Some("0 2 1"));
    let mut frame = zipzi_core::DrawableFrame::default();
    assert!(zipzi_core::evaluation::evaluate_frame(&doc, &Default::default(), &mut frame).is_ok());
    let canvas = Canvas {
        origin: exported_origin(&xml),
        ..canvas
    };
    let drawable = &frame.drawables[0];
    let expected: Vec<_> = drawable
        .positions
        .iter()
        .flat_map(|p| [p.x + canvas.origin.x, canvas.origin.y - p.y])
        .collect();
    let actual: Vec<f32> = field(mesh, "positions")
        .text()
        .unwrap()
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    for (a, b) in actual.iter().zip(expected) {
        assert!((a - b).abs() < 0.0001);
    }
    let expected: Vec<_> = drawable
        .uvs
        .iter()
        .flat_map(|uv| [uv.x, 1. - uv.y])
        .collect();
    let actual: Vec<f32> = field(mesh, "uvs")
        .text()
        .unwrap()
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    assert_eq!(actual, expected);
    assert!(doc.same_content(&before));
}

#[test]
fn an_unused_empty_image_is_reported_by_identity_before_reading_pixels() {
    let (mut document, png) = fixture();
    let id = "00000000-0000-4000-8000-000000000099";
    assert!(document
        .add_asset(ImageAsset {
            id: id.into(),
            name: "Draft".into(),
            content: None,
            unresolved_source: None
        })
        .status
        .is_ok());
    let error = encode_cmo3(
        &document,
        &|_: &ImageAsset| Ok(png.clone()),
        &Cmo3Options::default(),
    )
    .unwrap_err();
    assert!(matches!(error,Cmo3Error::Asset{asset_id,..} if asset_id==id));
}

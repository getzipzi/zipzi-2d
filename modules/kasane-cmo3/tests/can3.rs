mod common;
use common::*;
use kasane_cmo3::*;
use kasane_core::{document::*, types::*};
use std::sync::Arc;

fn uuid(n: u32) -> String {
    format!("ca030000-0000-4000-8000-{n:012x}")
}
fn key(n: u32, time: f32, value: f32) -> MotionKey {
    MotionKey {
        id: uuid(n),
        time,
        value,
    }
}
fn make_clip() -> MotionClip {
    MotionClip {
        id: uuid(100),
        name: "动画 <一> & walk".into(),
        duration: 1.,
        fps: 30.,
        looping: true,
        restricted_beziers: true,
        fade_in: Some(0.25),
        fade_out: Some(0.5),
        tracks: Vec::new(),
        events: vec![MotionEvent {
            id: uuid(101),
            time: 0.5,
            value: "事件 <你好>\n".into(),
            extensions: Default::default(),
        }],
        extensions: Default::default(),
        meta_extensions: Default::default(),
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    }
}
fn fixture_motion() -> (kasane_core::Document, Vec<u8>) {
    let (mut doc, png) = fixture();
    assert!(doc
        .create_part(Part {
            id: uuid(1),
            runtime_id: "PartArm".into(),
            name: "Arm".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    let mut clip = make_clip();
    clip.tracks.push(MotionTrack {
        id: uuid(10),
        target: MotionTrackTarget::Parameter {
            parameter_id: PARAM.into(),
        },
        initial: key(11, 0., -1.),
        segments: Arc::new(vec![
            MotionSegment::Linear {
                end: key(12, 0.2, 1.),
            },
            MotionSegment::Bezier {
                control1: MotionPoint {
                    time: 0.275,
                    value: 0.75,
                },
                control2: MotionPoint {
                    time: 0.325,
                    value: 0.6,
                },
                end: key(13, 0.4, 0.5),
            },
            MotionSegment::Stepped {
                end: key(14, 0.6, 0.),
            },
            MotionSegment::InverseStepped {
                end: key(15, 1., 1.),
            },
        ]),
        fade_in: Some(0.1),
        fade_out: Some(0.2),
        extensions: Default::default(),
    });
    for (n, target) in [
        (20, MotionTrackTarget::PartOpacity { part_id: uuid(1) }),
        (
            30,
            MotionTrackTarget::Model {
                runtime_id: "Opacity".into(),
            },
        ),
    ] {
        clip.tracks.push(MotionTrack {
            id: uuid(n),
            target,
            initial: key(n + 1, 0., 1.),
            segments: Arc::new(vec![MotionSegment::Linear {
                end: key(n + 2, 1., 0.25),
            }]),
            fade_in: None,
            fade_out: None,
            extensions: Default::default(),
        });
    }
    assert!(doc.create_motion(clip).status.is_ok());
    let mut other = make_clip();
    other.id = uuid(200);
    other.name = "仅事件".into();
    other.events[0].id = uuid(201);
    other.fps = 24.;
    other.duration = 2.;
    other.looping = false;
    other.restricted_beziers = false;
    assert!(doc.create_motion(other).status.is_ok());
    (doc, png)
}
fn xml(bytes: &[u8]) -> String {
    let entries = archive(bytes);
    assert_eq!(entries.len(), 1);
    assert_eq!(
        (&*entries[0].0, &*entries[0].1, entries[0].2),
        ("main.xml", "main_xml", 33)
    );
    String::from_utf8(entries[0].3.clone()).unwrap()
}
fn field<'a, 'b>(node: roxmltree::Node<'a, 'b>, name: &str) -> roxmltree::Node<'a, 'b> {
    node.children()
        .find(|n| n.attribute("xs.n") == Some(name))
        .or_else(|| {
            node.children()
                .find(|n| n.attribute("xs.n") == Some("super"))
                .map(|n| field(n, name))
        })
        .unwrap()
}
fn resolve<'a, 'b>(
    doc: &'a roxmltree::Document<'b>,
    reference: roxmltree::Node<'a, 'b>,
) -> roxmltree::Node<'a, 'b> {
    let id = reference.attribute("xs.ref").unwrap();
    doc.descendants()
        .find(|n| n.attribute("xs.id") == Some(id))
        .unwrap()
}
fn attr<'a, 'b>(doc: &'a roxmltree::Document<'b>, id: &str) -> roxmltree::Node<'a, 'b> {
    doc.descendants()
        .find(|n| {
            n.tag_name().name() == "CMvAttrF"
                && n.attribute("xs.id").is_some()
                && field(*n, "id").attribute("idstr") == Some(id)
        })
        .unwrap()
}
#[test]
fn linked_animation_preserves_scenes_curves_ids_and_metadata() {
    let (doc, png) = fixture_motion();
    let before = doc.clone();
    assert!(encode_cmo3(
        &doc,
        &|_: &ImageAsset| Ok(png.clone()),
        &Cmo3Options::default()
    )
    .is_err());
    let opts = CubismOptions {
        base_name: "角色".into(),
        model: Cmo3Options {
            loss_policy: LossPolicy::Allow,
            ..Default::default()
        },
    };
    let pair = encode_cubism(&doc, &|_: &ImageAsset| Ok(png.clone()), &opts).unwrap();
    assert_eq!(pair.can3.report.diagnostics.len(), 1);
    assert_eq!(
        pair.can3.report.diagnostics[0].code,
        "CAN3_INVERSE_STEP_BOUNDARY"
    );
    assert_eq!(pair.model_file_name, "角色.cmo3");
    assert_eq!(pair.animation_file_name, "角色.can3");
    let text = xml(&pair.can3.bytes);
    let parsed = roxmltree::Document::parse(&text).unwrap();
    assert!(text.contains("SerializeFormatVersion:2"));
    let root = resolve(
        &parsed,
        parsed
            .descendants()
            .find(|n| n.tag_name().name() == "main")
            .unwrap()
            .first_element_child()
            .unwrap(),
    );
    assert_eq!(root.tag_name().name(), "CAnimation");
    assert_eq!(field(root, "_scenes").attribute("count"), Some("2"));
    let scenes: Vec<_> = field(root, "_scenes")
        .children()
        .filter(|n| n.is_element())
        .map(|n| resolve(&parsed, n))
        .collect();
    assert_eq!(
        field(scenes[0], "sceneName").text(),
        Some("动画 <一> & walk")
    );
    assert_eq!(field(scenes[1], "sceneName").text(), Some("仅事件"));
    let movie = field(scenes[0], "movieInfo");
    assert_eq!(field(movie, "duration").text(), Some("30"));
    assert_eq!(field(movie, "fps").text(), Some("30"));
    assert_eq!(field(movie, "isLoopMotion").text(), Some("true"));
    assert_eq!(field(movie, "fadeInMSec").text(), Some("250"));
    assert_eq!(field(movie, "fadeOutMSec").text(), Some("500"));
    assert_eq!(
        field(field(scenes[1], "movieInfo"), "duration").text(),
        Some("48")
    );
    assert_eq!(
        field(field(scenes[1], "movieInfo"), "isBezierRestricted").text(),
        Some("false")
    );
    assert!(parsed
        .descendants()
        .any(|n| n.attribute("xs.n") == Some("srcFile") && n.text() == Some("角色.cmo3")));
    let parameter = attr(&parsed, "live2dParam_ParamX");
    let sequence = field(parameter, "valueData");
    assert_eq!(field(sequence, "keyPts2").text(), Some("0 6 12 18 30"));
    let curves: Vec<_> = field(sequence, "curveTypes")
        .children()
        .filter(|n| n.is_element())
        .map(|n| n.attribute("v").unwrap())
        .collect();
    assert_eq!(
        curves,
        ["LINEAR", "BEZIER", "STEP", "INVERSE_STEP", "LINEAR"]
    );
    let points: Vec<_> = field(sequence, "points")
        .children()
        .filter(|n| n.is_element())
        .collect();
    assert_eq!(
        field(field(points[1], "anchor"), "doubleValue").text(),
        Some("1")
    );
    assert!(
        (field(field(points[1], "next"), "posF")
            .text()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            - 8.25)
            .abs()
            < 0.00001
    );
    assert!(
        (field(field(points[2], "prev"), "posF")
            .text()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            - 9.75)
            .abs()
            < 0.00001
    );
    assert_eq!(
        field(parameter, "optionParam")
            .children()
            .find(|n| n.attribute("xs.n") == Some("KEY_ATTR_FADE_IN"))
            .unwrap()
            .text(),
        Some("100")
    );
    assert_eq!(
        field(attr(&parsed, "live2DPartsOpacity_PartArm"), "guid").attribute("uuid"),
        Some(uuid(1).as_str())
    );
    assert!(parsed
        .descendants()
        .any(|n| n.attribute("idstr") == Some("live2DPartsOpacity_PARTS_ARM")));
    assert!(parsed
        .descendants()
        .any(|n| n.attribute("xs.n") == Some("value") && n.text() == Some("事件 <你好>\n")));
    assert!(field(attr(&parsed, "opacity"), "valueData")
        .descendants()
        .any(|n| n.attribute("xs.n") == Some("doubleValue") && n.text() == Some("0.25")));
    for node in parsed.descendants().filter(|n| n.is_element()) {
        if node.attribute("xs.ref").is_some() {
            assert_eq!(
                node.tag_name().name(),
                resolve(&parsed, node).tag_name().name()
            );
        }
        if let Some(count) = node.attribute("count") {
            let actual = if node.tag_name().name() == "int-array" {
                node.text().unwrap_or("").split_whitespace().count()
            } else {
                node.children().filter(|n| n.is_element()).count()
            };
            assert_eq!(
                count.parse::<usize>().unwrap(),
                actual,
                "{}",
                node.tag_name().name()
            );
        }
    }
    assert!(doc.same_content(&before));
    assert_eq!(doc.revision(), before.revision());
    assert_eq!(
        pair.can3.bytes,
        encode_cubism(&doc, &|_: &ImageAsset| Ok(png.clone()), &opts)
            .unwrap()
            .can3
            .bytes
    );
    let target = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/cmo3-acceptance/animation");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join(&pair.model_file_name), &pair.cmo3.bytes).unwrap();
    std::fs::write(target.join(&pair.animation_file_name), &pair.can3.bytes).unwrap();
}
#[test]
fn empty_document_produces_an_editable_linked_scene_and_signed_keys_work() {
    let (doc, png) = fixture();
    for key in [0, -1, i32::MIN + 91] {
        let artifact = encode_can3(
            &doc,
            &Can3Options {
                obfuscation_key: key,
                ..Default::default()
            },
        )
        .unwrap();
        let text = xml(&artifact.bytes);
        assert!(text.contains("CAnimation"));
        assert!(text.contains("model.cmo3"));
        assert!(!artifact.report.has_loss());
        if key == 0 {
            let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/cmo3-acceptance/empty/model.can3");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, artifact.bytes).unwrap();
            let model = encode_cmo3(
                &doc,
                &|_: &ImageAsset| Ok(png.clone()),
                &Cmo3Options::default(),
            )
            .unwrap();
            std::fs::write(path.with_extension("cmo3"), model.bytes).unwrap();
        }
    }
}
#[test]
fn quantization_and_collisions_are_reported_before_asset_io() {
    let (mut doc, _) = fixture();
    let mut clip = make_clip();
    clip.events.clear();
    clip.tracks.push(MotionTrack {
        id: uuid(10),
        target: MotionTrackTarget::Parameter {
            parameter_id: PARAM.into(),
        },
        initial: key(11, 0., 0.),
        segments: Arc::new(vec![MotionSegment::Linear {
            end: key(12, 0.01, 1.),
        }]),
        fade_in: Some(0.0005),
        fade_out: None,
        extensions: Default::default(),
    });
    clip.events = vec![
        MotionEvent {
            id: uuid(101),
            time: 0.,
            value: "a".into(),
            extensions: Default::default(),
        },
        MotionEvent {
            id: uuid(102),
            time: 0.01,
            value: "b".into(),
            extensions: Default::default(),
        },
    ];
    assert!(doc.create_motion(clip).status.is_ok());
    let report = inspect_can3(&doc).unwrap();
    for code in [
        "CAN3_FRAME_QUANTIZATION",
        "CAN3_KEY_COLLISION",
        "CAN3_EVENT_COLLISION",
        "CAN3_FADE_QUANTIZATION",
    ] {
        assert!(report.diagnostics.iter().any(|d| d.code == code), "{code}");
    }
    let resolver =
        |_: &ImageAsset| -> Result<Vec<u8>, Cmo3Error> { panic!("losses must precede image I/O") };
    assert!(matches!(
        encode_cubism(&doc, &resolver, &CubismOptions::default()),
        Err(Cmo3Error::Unsupported(_))
    ));
    assert!(matches!(
        encode_can3(&doc, &Can3Options::default()),
        Err(Cmo3Error::Unsupported(_))
    ));
    let artifact = encode_can3(
        &doc,
        &Can3Options {
            loss_policy: LossPolicy::Allow,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(artifact.report, report);
    let text = xml(&artifact.bytes);
    let parsed = roxmltree::Document::parse(&text).unwrap();
    let value = field(attr(&parsed, "live2dParam_ParamX"), "valueData");
    assert_eq!(field(value, "points").attribute("count"), Some("1"));
}
#[test]
fn unsupported_effects_registrations_and_duplicate_tracks_are_explicit() {
    let (mut doc, _) = fixture();
    let mut clip = make_clip();
    clip.events.clear();
    for (n, target) in [
        (
            10,
            MotionTrackTarget::Model {
                runtime_id: "EyeBlink".into(),
            },
        ),
        (
            20,
            MotionTrackTarget::Unresolved {
                category: "Parameter".into(),
                runtime_id: "Missing".into(),
            },
        ),
        (
            30,
            MotionTrackTarget::Model {
                runtime_id: "Opacity".into(),
            },
        ),
        (
            40,
            MotionTrackTarget::Model {
                runtime_id: "Opacity".into(),
            },
        ),
    ] {
        clip.tracks.push(MotionTrack {
            id: uuid(n),
            target,
            initial: key(n + 1, 0., 1.),
            segments: Arc::new(Vec::new()),
            fade_in: None,
            fade_out: None,
            extensions: Default::default(),
        });
    }
    assert!(doc.create_motion(clip).status.is_ok());
    assert!(doc
        .set_motion_groups(vec![MotionGroup {
            name: "Idle".into(),
            entries: vec![MotionRegistration {
                clip_id: uuid(100),
                fade_in: None,
                fade_out: None,
                sound: Some("voice.wav".into()),
                extensions: Default::default()
            }]
        }])
        .status
        .is_ok());
    let report = inspect_can3(&doc).unwrap();
    for code in [
        "CAN3_UNSUPPORTED_TARGET",
        "CAN3_TRACK_COLLISION",
        "CAN3_MOTION_REGISTRATIONS",
    ] {
        assert!(report.diagnostics.iter().any(|d| d.code == code));
    }
}
#[test]
fn invalid_names_frame_overflows_and_xml_characters_fail_without_artifacts() {
    let (mut doc, png) = fixture();
    for name in [
        "../model.cmo3",
        "x/model.cmo3",
        "C:\\model.cmo3",
        "model.moc3",
        "CON.cmo3",
        "",
    ] {
        assert!(encode_can3(
            &doc,
            &Can3Options {
                model_file_name: name.into(),
                ..Default::default()
            }
        )
        .is_err());
    }
    assert!(encode_cubism(
        &doc,
        &|_: &ImageAsset| Ok(png.clone()),
        &CubismOptions {
            base_name: "../bad".into(),
            ..Default::default()
        }
    )
    .is_err());
    assert!(encode_can3(
        &doc,
        &Can3Options {
            animation_name: "bad\u{1}".into(),
            ..Default::default()
        }
    )
    .is_err());
    let mut short = make_clip();
    short.events.clear();
    short.duration = 0.000001;
    assert!(doc.create_motion(short).status.is_ok());
    assert!(inspect_can3(&doc)
        .unwrap()
        .diagnostics
        .iter()
        .any(|d| d.code == "CAN3_MINIMUM_DURATION"));
    assert!(doc.erase_object(&uuid(100)).status.is_ok());
    let mut clip = make_clip();
    clip.duration = f32::MAX;
    assert!(doc.create_motion(clip).status.is_ok());
    assert!(matches!(
        inspect_can3(&doc),
        Err(Cmo3Error::InvalidDocument(_))
    ));
}

#[test]
fn delayed_bezier_tracks_retain_absolute_frames() {
    let (mut doc, png) = fixture();
    let mut clip = make_clip();
    clip.events.clear();
    clip.duration = 2.;
    clip.looping = false;
    clip.tracks.push(MotionTrack {
        id: uuid(10),
        target: MotionTrackTarget::Parameter {
            parameter_id: PARAM.into(),
        },
        initial: key(11, 0.5, 0.),
        segments: Arc::new(vec![MotionSegment::Bezier {
            control1: MotionPoint {
                time: 5. / 6.,
                value: 0.2,
            },
            control2: MotionPoint {
                time: 7. / 6.,
                value: 0.8,
            },
            end: key(12, 1.5, 1.),
        }]),
        fade_in: None,
        fade_out: None,
        extensions: Default::default(),
    });
    let mut unrestricted = clip.clone();
    unrestricted.id = uuid(200);
    unrestricted.name = "Unrestricted Bezier".into();
    unrestricted.restricted_beziers = false;
    unrestricted.tracks[0].id = uuid(21);
    unrestricted.tracks[0].initial.id = uuid(22);
    unrestricted.tracks[0].segments = Arc::new(vec![MotionSegment::Bezier {
        control1: MotionPoint {
            time: 0.7,
            value: 0.2,
        },
        control2: MotionPoint {
            time: 1.1,
            value: 0.8,
        },
        end: key(23, 1.5, 1.),
    }]);
    assert!(doc.create_motion(clip).status.is_ok());
    assert!(doc.create_motion(unrestricted).status.is_ok());
    let artifact = encode_cubism(
        &doc,
        &|_: &ImageAsset| Ok(png.clone()),
        &CubismOptions::default(),
    )
    .unwrap();
    assert!(!artifact.report.has_loss());
    let text = xml(&artifact.can3.bytes);
    let parsed = roxmltree::Document::parse(&text).unwrap();
    assert_eq!(
        field(
            field(attr(&parsed, "live2dParam_ParamX"), "valueData"),
            "keyPts2"
        )
        .text(),
        Some("15 45")
    );
    assert_eq!(
        field(
            field(attr(&parsed, "live2dParam_ParamX"), "valueData"),
            "posStart"
        )
        .text(),
        Some("0")
    );
    let target = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/cmo3-acceptance/delayed");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join(&artifact.model_file_name), artifact.cmo3.bytes).unwrap();
    std::fs::write(
        target.join(&artifact.animation_file_name),
        artifact.can3.bytes,
    )
    .unwrap();
}

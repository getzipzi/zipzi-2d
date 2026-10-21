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

#[test]
fn automatic_time_grid_preserves_millisecond_rounded_beziers_at_sixty_fps() {
    let (mut doc, _) = fixture();
    let mut clip = make_clip();
    clip.events.clear();
    clip.tracks.push(MotionTrack {
        id: uuid(10),
        target: MotionTrackTarget::Parameter {
            parameter_id: PARAM.into(),
        },
        initial: key(11, 0., 0.),
        segments: Arc::new(vec![
            MotionSegment::Linear {
                end: key(12, 0.017, -0.2),
            },
            MotionSegment::Bezier {
                control1: MotionPoint {
                    time: 0.139,
                    value: 0.8,
                },
                control2: MotionPoint {
                    time: 0.261,
                    value: -0.7,
                },
                end: key(13, 0.383, 0.6),
            },
            MotionSegment::Linear {
                end: key(14, 1., 1.),
            },
        ]),
        fade_in: None,
        fade_out: None,
        extensions: Default::default(),
    });
    assert!(doc.create_motion(clip.clone()).status.is_ok());
    let mut second = make_clip();
    second.id = uuid(200);
    second.name = "Same declared FPS".into();
    second.events.clear();
    assert!(doc.create_motion(second).status.is_ok());
    let mut other_rate = make_clip();
    other_rate.id = uuid(300);
    other_rate.name = "Independent 24 FPS group".into();
    other_rate.fps = 24.;
    other_rate.events.clear();
    assert!(doc.create_motion(other_rate).status.is_ok());
    let options = Can3Options {
        auto_frame_rate: true,
        bake_animation: true,
        ..Default::default()
    };
    let artifact = encode_can3(&doc, &options).unwrap();
    assert!(!artifact.report.has_loss());
    assert!(!artifact
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "CAN3_BAKED_TRACK"));
    let mut returned = fixture().0;
    import_animation(&artifact.bytes, &mut returned).unwrap();
    let after = returned.get_motion(&clip.id).unwrap();
    assert_eq!(after.fps, 60.);
    assert_eq!(returned.get_motion(&uuid(200)).unwrap().fps, 60.);
    assert_eq!(returned.get_motion(&uuid(300)).unwrap().fps, 24.);
    let track = &after.tracks[0];
    assert_eq!(track.keys().count(), 4);
    assert_eq!(track.segments.len(), clip.tracks[0].segments.len());
    assert_eq!(track.segments[0].end().time, 1. / 60.);
    assert_eq!(track.segments[1].end().time, 23. / 60.);
    let MotionSegment::Bezier {
        control1, control2, ..
    } = &track.segments[1]
    else {
        panic!("Bezier was lost")
    };
    assert_eq!(control1.value, 0.8);
    assert_eq!(control2.value, -0.7);
    let before = kasane_animation::CompiledCurve::from(&clip.tracks[0]);
    let result = kasane_animation::CompiledCurve::from(track);
    for step in 0..=100 {
        let fraction = step as f32 / 100.;
        let source = 0.017 + (0.383 - 0.017) * fraction;
        let output = 1. / 60. + (22. / 60.) * fraction;
        let a = kasane_animation::sample_motion_curve(&before, source, true);
        let b = kasane_animation::sample_motion_curve(&result, output, true);
        assert!(
            (a - b).abs() < 0.00001,
            "Bezier fraction {fraction}: {a} != {b}"
        );
    }
}

#[test]
fn automatic_grid_keeps_unrestricted_bezier_time_handles() {
    let (mut doc, _) = fixture();
    let mut clip = make_clip();
    clip.restricted_beziers = false;
    clip.events.clear();
    clip.tracks.push(MotionTrack {
        id: uuid(10),
        target: MotionTrackTarget::Parameter {
            parameter_id: PARAM.into(),
        },
        initial: key(11, 0.017, 0.),
        segments: Arc::new(vec![MotionSegment::Bezier {
            control1: MotionPoint {
                time: 0.05,
                value: 0.9,
            },
            control2: MotionPoint {
                time: 0.3,
                value: -0.7,
            },
            end: key(12, 0.383, 0.6),
        }]),
        fade_in: None,
        fade_out: None,
        extensions: Default::default(),
    });
    assert!(doc.create_motion(clip.clone()).status.is_ok());
    let artifact = encode_can3(
        &doc,
        &Can3Options {
            auto_frame_rate: true,
            bake_animation: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!artifact.report.has_loss());
    let mut returned = fixture().0;
    import_animation(&artifact.bytes, &mut returned).unwrap();
    let after = returned.get_motion(&clip.id).unwrap();
    assert_eq!(after.fps, 60.);
    assert!(!after.restricted_beziers);
    assert_eq!(after.tracks[0].keys().count(), 2);
    let before = kasane_animation::CompiledCurve::from(&clip.tracks[0]);
    let result = kasane_animation::CompiledCurve::from(&after.tracks[0]);
    for step in 0..=100 {
        let fraction = step as f32 / 100.;
        let a = kasane_animation::sample_motion_curve(&before, 0.017 + 0.366 * fraction, false);
        let b =
            kasane_animation::sample_motion_curve(&result, 1. / 60. + 22. / 60. * fraction, false);
        assert!((a - b).abs() < 0.00002, "fraction {fraction}: {a} != {b}");
    }
}

#[test]
fn automatic_time_grid_repairs_only_inverse_step_boundaries() {
    let (mut doc, _) = fixture();
    let mut clip = make_clip();
    clip.events.clear();
    clip.tracks.push(MotionTrack {
        id: uuid(10),
        target: MotionTrackTarget::Parameter {
            parameter_id: PARAM.into(),
        },
        initial: key(11, 0., 0.),
        segments: Arc::new(vec![
            MotionSegment::Bezier {
                control1: MotionPoint {
                    time: 0.167,
                    value: 0.2,
                },
                control2: MotionPoint {
                    time: 0.333,
                    value: 0.8,
                },
                end: key(12, 0.5, 1.),
            },
            MotionSegment::InverseStepped {
                end: key(13, 0.517, -0.5),
            },
            MotionSegment::Linear {
                end: key(14, 1., 0.),
            },
        ]),
        fade_in: None,
        fade_out: None,
        extensions: Default::default(),
    });
    assert!(doc.create_motion(clip.clone()).status.is_ok());
    let artifact = encode_can3(
        &doc,
        &Can3Options {
            auto_frame_rate: true,
            bake_animation: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!artifact.report.has_loss());
    assert!(artifact
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "CAN3_INVERSE_STEP_REPAIRED"));
    assert!(!artifact
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "CAN3_BAKED_TRACK"));
    let mut returned = fixture().0;
    import_animation(&artifact.bytes, &mut returned).unwrap();
    let after = returned.get_motion(&clip.id).unwrap();
    assert_eq!(after.fps, 60.);
    assert_eq!(after.tracks[0].keys().count(), 5);
    assert!(matches!(
        after.tracks[0].segments[0],
        MotionSegment::Bezier { .. }
    ));
    let a = kasane_animation::CompiledCurve::from(&clip.tracks[0]);
    let b = kasane_animation::CompiledCurve::from(&after.tracks[0]);
    for frame in 0..=60 {
        let time = frame as f32 / 60.;
        // Restore the source's millisecond-rounded final short interval.
        let source = if frame >= 31 {
            0.517 + (time - 31. / 60.) * (1. - 0.517) / (1. - 31. / 60.)
        } else {
            time
        };
        let expected = kasane_animation::sample_motion_curve(&a, source, true);
        let actual = kasane_animation::sample_motion_curve(&b, time, true);
        assert!(
            (expected - actual).abs() < 0.00001,
            "frame {frame}: {expected} != {actual}"
        );
    }
}

#[test]
fn unsupported_precision_uses_only_explicit_baking_fallback() {
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
            end: key(12, 0.012345, 1.),
        }]),
        fade_in: None,
        fade_out: None,
        extensions: Default::default(),
    });
    assert!(doc.create_motion(clip).status.is_ok());
    assert!(matches!(
        encode_can3(
            &doc,
            &Can3Options {
                auto_frame_rate: true,
                ..Default::default()
            }
        ),
        Err(Cmo3Error::Unsupported(_))
    ));
    let artifact = encode_can3(
        &doc,
        &Can3Options {
            auto_frame_rate: true,
            bake_animation: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(artifact
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "CAN3_TIME_GRID_UNRESOLVED"));
    assert!(artifact
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "CAN3_BAKED_TRACK"));
    assert!(!artifact.report.has_loss());
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
        tracks: Default::default(),
        events: vec![MotionEvent {
            id: uuid(101),
            time: 0.5,
            value: "事件 <你好>\n".into(),
            extensions: Default::default(),
        }]
        .into(),
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
        bake_animation: false,
        auto_frame_rate: false,
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
    *clip.events = vec![
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

#[test]
fn baked_expression_blends_keep_default_model_poses_and_source_data() {
    let (mut doc, png) = fixture();
    let mut parameter = doc.get_parameter(PARAM).unwrap().clone();
    parameter.default_value = 0.5;
    assert!(doc.replace_parameter(parameter).status.is_ok());
    for (index, blend, value) in [
        (300, ExpressionBlend::Add, 0.25),
        (301, ExpressionBlend::Multiply, 0.2),
        (302, ExpressionBlend::Overwrite, -0.7),
        (303, ExpressionBlend::Multiply, -5.),
        (304, ExpressionBlend::Overwrite, 5.),
    ] {
        assert!(doc
            .create_expression(ExpressionAsset {
                id: uuid(index),
                name: format!("{blend:?}_{index}"),
                file_type: Some("Live2D Expression".into()),
                fade_in: Some(0.3),
                fade_out: Some(0.5),
                entries: vec![ExpressionEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    target: ExpressionTarget::Resolved {
                        parameter_id: PARAM.into()
                    },
                    blend: Some(blend),
                    value,
                    extensions: Default::default(),
                }],
                extensions: Default::default(),
                opaque_source_ids: None,
                opaque_source_content_hash: None,
            })
            .status
            .is_ok());
    }
    let before = doc.clone();
    assert!(!encode_can3(&doc, &Can3Options::default())
        .unwrap()
        .report
        .has_loss());
    let options = CubismOptions {
        bake_animation: true,
        ..Default::default()
    };
    let pair = encode_cubism(&doc, &|_: &ImageAsset| Ok(png.clone()), &options).unwrap();
    assert!(!pair.report.has_loss());
    assert_eq!(
        pair.report
            .diagnostics
            .iter()
            .filter(|d| d.code == "CAN3_EXPRESSION_SEMANTICS")
            .count(),
        4
    );
    let mut returned = import_model(&pair.cmo3.bytes).unwrap().document;
    import_animation(&pair.can3.bytes, &mut returned).unwrap();
    for (index, pose) in [(300, 0.75), (301, 0.1), (302, -0.7), (303, -1.), (304, 1.)] {
        let expression = returned.get_expression(&uuid(index)).unwrap();
        assert_eq!(expression.entries.len(), 1);
        assert_eq!(expression.fade_in, Some(0.3));
        assert_eq!(expression.fade_out, Some(0.5));
        assert_eq!(
            kasane_core::document::ExpressionEntryFields::from(&expression.entries[0]),
            kasane_core::document::ExpressionEntryFields::from(
                &doc.get_expression(&uuid(index)).unwrap().entries[0]
            )
        );
        assert!(
            (expression.source_scene().unwrap().tracks[0].initial.value - pose).abs() < 0.00001
        );
    }
    for baseline in [-1., 0., 0.5, 1.] {
        for id in doc.expression_order() {
            let mut expected = kasane_animation::ExpressionPreview::new(&doc);
            let mut actual = kasane_animation::ExpressionPreview::new(&returned);
            for preview in [&mut expected, &mut actual] {
                preview.set_base_parameter(PARAM, baseline).unwrap();
                preview.schedule_expression(id, 0.).unwrap();
            }
            for dt in [0., 0.05, 0.1, 0.15, 0.3] {
                assert_eq!(
                    expected.advance(dt).unwrap().parameters,
                    actual.advance(dt).unwrap().parameters
                );
            }
        }
    }
    assert!(doc.same_content(&before));
    // Exercise a moving baseline, fades and expression cross-fades together.
    let mut clip = make_clip();
    clip.looping = false;
    clip.events.clear();
    clip.tracks.push(MotionTrack {
        id: uuid(600),
        target: MotionTrackTarget::Parameter {
            parameter_id: PARAM.into(),
        },
        initial: key(601, 0., 0.8),
        segments: Arc::new(vec![MotionSegment::Linear {
            end: key(602, 1., -0.8),
        }]),
        fade_in: Some(0.),
        fade_out: Some(0.),
        extensions: Default::default(),
    });
    assert!(doc.create_motion(clip.clone()).status.is_ok());
    assert!(returned.create_motion(clip.clone()).status.is_ok());
    let mut expected = kasane_animation::MotionPreview::new(&doc);
    let mut actual = kasane_animation::MotionPreview::new(&returned);
    for preview in [&mut expected, &mut actual] {
        preview.schedule_motion(&clip.id, 0.).unwrap();
        preview.schedule_expression(&uuid(301), 0.1).unwrap();
        preview.schedule_expression(&uuid(302), 0.5).unwrap();
        preview.schedule_expression(&uuid(300), 0.8).unwrap();
    }
    for _ in 0..90 {
        assert_eq!(
            expected.advance(1. / 60.).unwrap().parameters,
            actual.advance(1. / 60.).unwrap().parameters
        );
    }
    let target = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/cmo3-acceptance/expression-semantics");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("model.cmo3"), &pair.cmo3.bytes).unwrap();
    std::fs::write(target.join("model.can3"), &pair.can3.bytes).unwrap();
    assert_eq!(before.expression_order(), doc.expression_order());
}

#[test]
fn baking_retains_frame_samples_when_keys_collide_and_inverse_steps_jump() {
    let (mut doc, _) = fixture();
    let mut clip = make_clip();
    clip.duration = 1.017;
    clip.events.clear();
    clip.tracks.push(MotionTrack {
        id: uuid(10),
        target: MotionTrackTarget::Parameter {
            parameter_id: PARAM.into(),
        },
        initial: key(11, 0., -0.5),
        segments: Arc::new(vec![
            MotionSegment::Linear {
                end: key(12, 0.01, 0.8),
            },
            MotionSegment::Linear {
                end: key(13, 0.017, -0.2),
            },
            MotionSegment::Bezier {
                control1: MotionPoint {
                    time: 0.09,
                    value: 0.9,
                },
                control2: MotionPoint {
                    time: 0.22,
                    value: -0.8,
                },
                end: key(14, 0.41, 0.3),
            },
            MotionSegment::InverseStepped {
                end: key(15, 0.6, 0.7),
            },
            MotionSegment::Stepped {
                end: key(16, 1.017, -0.9),
            },
        ]),
        fade_in: None,
        fade_out: None,
        extensions: Default::default(),
    });
    assert!(doc.create_motion(clip.clone()).status.is_ok());
    let artifact = encode_can3(
        &doc,
        &Can3Options {
            bake_animation: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!artifact.report.has_loss());
    assert!(artifact
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "CAN3_BAKED_TRACK"));
    let mut returned = fixture().0;
    import_animation(&artifact.bytes, &mut returned).unwrap();
    let after = returned.get_motion(&clip.id).unwrap();
    let curve = kasane_animation::CompiledCurve::from(&clip.tracks[0]);
    let actual = kasane_animation::CompiledCurve::from(&after.tracks[0]);
    for frame in 0..=31 {
        let time = frame as f32 / clip.fps;
        let expected = kasane_animation::sample_motion_curve(&curve, time, clip.restricted_beziers);
        let value = kasane_animation::sample_motion_curve(&actual, time, after.restricted_beziers);
        assert!(
            (expected - value).abs() < 0.00001,
            "frame {frame}: {expected} != {value}"
        );
    }
    let keys: Vec<_> = after.tracks[0].keys().collect();
    assert!(keys.windows(2).all(|pair| pair[0].time < pair[1].time));
}

#[test]
fn baking_keeps_exact_curves_and_still_rejects_unsupported_targets() {
    let (mut doc, _) = fixture();
    let mut clip = make_clip();
    clip.events.clear();
    clip.tracks.push(MotionTrack {
        id: uuid(10),
        target: MotionTrackTarget::Parameter {
            parameter_id: PARAM.into(),
        },
        initial: key(11, 0., -0.5),
        segments: Arc::new(vec![MotionSegment::Linear {
            end: key(12, 1., 0.8),
        }]),
        fade_in: None,
        fade_out: None,
        extensions: Default::default(),
    });
    assert!(doc.create_motion(clip.clone()).status.is_ok());
    let strict = encode_can3(&doc, &Can3Options::default()).unwrap();
    let baked = encode_can3(
        &doc,
        &Can3Options {
            bake_animation: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(strict.bytes, baked.bytes);
    clip.tracks[0].target = MotionTrackTarget::Model {
        runtime_id: "EyeBlink".into(),
    };
    assert!(doc.replace_motion(clip).status.is_ok());
    assert!(matches!(
        encode_can3(
            &doc,
            &Can3Options {
                bake_animation: true,
                ..Default::default()
            }
        ),
        Err(Cmo3Error::Unsupported(_))
    ));
}

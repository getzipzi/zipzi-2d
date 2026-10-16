use editor_core::*;
use kasane_core::{
    Canvas, Parameter, Part, Vec2,
    document::{
        CdiParameterEntry, CdiParameterGroup, MotionClip, MotionEvent, MotionKey, MotionPoint,
        MotionSegment, MotionTrack, MotionTrackTarget,
    },
};
use kasane_sdk::AuthoringSession;
use std::sync::Arc;

pub(super) fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

pub(super) fn fixture(
    duplicate: bool,
    opaque: bool,
) -> anyhow::Result<(Editor, tempfile::TempDir)> {
    let temp = tempfile::tempdir()?;
    let mut s = AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.))?;
    s.edit("Fixture", None, |edit| {
        for (n, runtime, name, default_value) in [(2, "ParamX", "X", 0.), (3, "ParamY", "Y", 0.3)] {
            edit.create_parameter(Parameter {
                id: id(n),
                runtime_id: runtime.into(),
                name: name.into(),
                minimum: -2.,
                maximum: 2.,
                default_value,
                ..Default::default()
            })?;
        }
        edit.create_part(Part {
            id: id(7),
            runtime_id: "Part0".into(),
            name: "Part".into(),
            ..Default::default()
        })?;
        let mut info = edit.candidate_document().display_info().clone();
        info.parameter_groups = Some(vec![
            CdiParameterGroup {
                id: id(5),
                runtime_id: "Root".into(),
                name: "Root".into(),
                parent_id: None,
                extensions: Default::default(),
            },
            CdiParameterGroup {
                id: id(6),
                runtime_id: "Child".into(),
                name: "Child".into(),
                parent_id: Some(id(5)),
                extensions: Default::default(),
            },
        ]);
        info.parameters = Some(vec![
            CdiParameterEntry::Resolved {
                parameter_id: id(3),
                group_id: Some(id(5)),
                extensions: Default::default(),
            },
            CdiParameterEntry::Resolved {
                parameter_id: id(2),
                group_id: Some(id(6)),
                extensions: Default::default(),
            },
        ]);
        edit.replace_display_info(info)?;
        let track = MotionTrack {
            id: id(8),
            target: MotionTrackTarget::Parameter {
                parameter_id: id(2),
            },
            initial: MotionKey {
                id: id(10),
                time: 0.1137,
                value: -0.8,
            },
            segments: Arc::new(vec![
                MotionSegment::Bezier {
                    control1: MotionPoint {
                        time: 0.2,
                        value: 1.2,
                    },
                    control2: MotionPoint {
                        time: 0.45,
                        value: -0.2,
                    },
                    end: MotionKey {
                        id: id(11),
                        time: 0.6,
                        value: 0.5,
                    },
                },
                MotionSegment::Linear {
                    end: MotionKey {
                        id: id(12),
                        time: 1.5,
                        value: 0.8,
                    },
                },
            ]),
            fade_in: None,
            fade_out: None,
            extensions: Default::default(),
        };
        let mut tracks = vec![track.clone()];
        if duplicate {
            let mut second = track;
            second.id = id(9);
            second.initial.id = id(30);
            for (i, segment) in Arc::make_mut(&mut second.segments).iter_mut().enumerate() {
                segment.end_mut().id = id(31 + i as u32);
            }
            tracks.push(second);
        }
        tracks.push(MotionTrack {
            id: id(50),
            target: MotionTrackTarget::Unresolved {
                category: "Parameter".into(),
                runtime_id: "Missing".into(),
            },
            initial: MotionKey {
                id: id(51),
                time: 0.5,
                value: 0.4,
            },
            segments: Arc::new(vec![]),
            fade_in: None,
            fade_out: None,
            extensions: Default::default(),
        });
        edit.create_motion(MotionClip {
            id: id(4),
            name: "Fixture".into(),
            duration: 2.,
            fps: 30.,
            looping: true,
            restricted_beziers: false,
            fade_in: Some(1.),
            fade_out: Some(1.),
            tracks,
            events: vec![
                MotionEvent {
                    id: id(40),
                    time: 0.5,
                    value: "B".into(),
                    extensions: Default::default(),
                },
                MotionEvent {
                    id: id(41),
                    time: 0.5,
                    value: "A".into(),
                    extensions: Default::default(),
                },
            ],
            extensions: if opaque {
                [("Future".into(), serde_json::json!(true))].into()
            } else {
                Default::default()
            },
            meta_extensions: Default::default(),
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        })
    })?;
    let receipt = s.save_project(&temp.path().join("fixture.kasane"), None)?;
    let mut editor = crate::support::editor_from_document(Document::open(&receipt.manifest)?);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_canvas_metrics(CanvasMetrics {
        width: 256.,
        height: 256.,
        scale_factor: 1.,
    });
    let document = editor.active_id().unwrap();
    // Timeline authoring checks inspect raw clip values rather than Full Effect.
    editor.set_animation_preview_mode(document, AnimationPreviewMode::Editing)?;
    editor.select_motion(
        document,
        MotionSelection {
            clip_id: id(4),
            entry: None,
        },
    )?;
    editor.update_timeline_view(document, |view| view.snap = false)?;
    Ok((editor, temp))
}

pub(super) fn key(n: u32) -> TimelineKeyRef {
    TimelineKeyRef {
        clip_id: id(4),
        track_id: if n >= 30 { id(9) } else { id(8) },
        key_id: id(n),
    }
}

pub(super) fn write(channel: u32, track_id: Option<String>, value: f32) -> TimelineCommand {
    TimelineCommand::WriteValue {
        channel: TimelineChannelId::Parameter(id(channel)),
        track_id,
        selected_key: None,
        value,
    }
}

pub(super) fn evaluate(
    editor: &mut Editor,
    evaluator: &mut AnimationEvaluator,
) -> anyhow::Result<AnimationEvaluated> {
    evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)
}

use super::common::*;
use crate::support::{pointer_input, present_canvas as shown};
use zipzi_editor_core::*;

fn frame(editor: &Editor, object: &ObjectRef) -> WarpFrameInfo {
    editor
        .warp_frame_info(editor.primary_viewport_binding().unwrap(), object)
        .unwrap()
}
fn action(editor: &mut Editor, object: &ObjectRef, action: WarpFrameAction) -> anyhow::Result<()> {
    editor.change_warp_frame(
        editor.primary_viewport_binding().unwrap(),
        object.clone(),
        action,
    )
}
fn native_drag(
    editor: &mut Editor,
    start: ViewportPoint,
    end: ViewportPoint,
    cancel: bool,
) -> anyhow::Result<()> {
    pointer_input(
        editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    pointer_input(
        editor,
        CanvasInput::Move {
            point: end,
            left_down: true,
        },
    )?;
    assert!(
        !editor.document().unwrap().has_pending_edit(),
        "frame adjustment must never acquire a model candidate"
    );
    pointer_input(
        editor,
        if cancel {
            CanvasInput::Cancel
        } else {
            CanvasInput::Up { point: end }
        },
    )?;
    Ok(())
}

#[test]
fn warp_frame_adjustment_move_rotation_scale_and_cancel_leave_the_model_unchanged()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    editor.execute(EditorCommand::ToggleWarpFine)?;
    let before = control_points(&mut editor, &object)?;
    action(&mut editor, &object, WarpFrameAction::ToggleAdjust)?;
    let original = frame(&editor, &object);
    for cancel in [true, false] {
        let handles = editor.overlay().handles;
        let start = ViewportPoint {
            x: (handles[1].position.x + handles[2].position.x) / 2.,
            y: handles[1].position.y,
        };
        let end = ViewportPoint {
            x: start.x + 20.,
            y: start.y + 10.,
        };
        native_drag(&mut editor, start, end, cancel)?;
        let info = frame(&editor, &object);
        assert_eq!(info.frame.half_size, original.frame.half_size);
        assert!(
            (info.frame.center.x - original.frame.center.x - if cancel { 0. } else { 20. }).abs()
                < 0.01
        );
        assert!(
            (info.frame.center.y - original.frame.center.y + if cancel { 0. } else { 10. }).abs()
                < 0.01
        );
    }
    let t = shown(&mut editor)?.transform;
    let center = t.to_viewport(frame(&editor, &object).frame.center);
    let start = editor.overlay().handles.last().unwrap().position;
    let end = ViewportPoint {
        x: center.x + start.y - center.y,
        y: center.y - start.x + center.x,
    };
    native_drag(&mut editor, start, end, false)?;
    let info = frame(&editor, &object);
    assert!((info.frame.radians - std::f32::consts::FRAC_PI_2).abs() < 0.001);
    let start = editor.overlay().handles[3].position;
    let end = ViewportPoint {
        x: start.x,
        y: start.y - info.frame.half_size[0] * t.scale,
    };
    native_drag(&mut editor, start, end, false)?;
    assert!(
        (frame(&editor, &object).frame.half_size[0] - info.frame.half_size[0] * 1.5).abs() < 0.01
    );
    assert_eq!(
        frame(&editor, &object).frame.half_size[1],
        info.frame.half_size[1]
    );
    assert_eq!(control_points(&mut editor, &object)?, before);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn aligned_frame_scales_along_its_own_axis_and_numeric_rotation_uses_the_adjusted_center()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    editor.execute(EditorCommand::ToggleWarpFine)?;
    editor.execute(EditorCommand::AdjustDeformer {
        object: object.clone(),
        adjustment: DeformerAdjustment {
            translation: ModelVector::default(),
            angle: 30.,
            scale: 1.,
        },
    })?;
    let before = control_points(&mut editor, &object)?;
    action(&mut editor, &object, WarpFrameAction::Align)?;
    let aligned = frame(&editor, &object).frame;
    assert!((aligned.radians.to_degrees() - 30.).abs() < 0.001);
    assert_eq!(control_points(&mut editor, &object)?, before);
    let t = shown(&mut editor)?.transform;
    let start = editor.overlay().handles[3].position;
    let distance = 2. * aligned.half_size[0] * t.scale;
    let end = ViewportPoint {
        x: start.x + distance * aligned.radians.cos(),
        y: start.y - distance * aligned.radians.sin(),
    };
    drag(&mut editor, start, end, false)?;
    let after = control_points(&mut editor, &object)?;
    let (sin, cos) = aligned.radians.sin_cos();
    let anchor = ModelPoint {
        x: aligned.center.x - aligned.half_size[0] * cos,
        y: aligned.center.y - aligned.half_size[0] * sin,
    };
    for (a, b) in before.iter().zip(&after) {
        let local_x = (a.x - anchor.x) * cos + (a.y - anchor.y) * sin;
        assert!((b.x - a.x - local_x * cos).abs() < 0.02);
        assert!((b.y - a.y - local_x * sin).abs() < 0.02);
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    action(&mut editor, &object, WarpFrameAction::ToggleAdjust)?;
    let handles = editor.overlay().handles;
    let start = ViewportPoint {
        x: (handles[0].position.x + handles[1].position.x) / 2.,
        y: (handles[0].position.y + handles[1].position.y) / 2.,
    };
    native_drag(
        &mut editor,
        start,
        ViewportPoint {
            x: start.x + 40.,
            y: start.y,
        },
        false,
    )?;
    action(&mut editor, &object, WarpFrameAction::ToggleAdjust)?;
    let pivot = frame(&editor, &object).frame.center;
    editor.execute(EditorCommand::AdjustWarp {
        target: editor.warp_transform_target(&object).unwrap(),
        adjustment: DeformerAdjustment {
            translation: ModelVector::default(),
            angle: 90.,
            scale: 1.,
        },
    })?;
    for (a, b) in after.iter().zip(control_points(&mut editor, &object)?) {
        assert!((b.x - (pivot.x - a.y + pivot.y)).abs() < 0.02);
        assert!((b.y - (pivot.y + a.x - pivot.x)).abs() < 0.02);
    }
    Ok(())
}

#[test]
fn frame_visibility_is_per_viewport_hidden_handles_do_not_hit_and_points_remain_editable()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    editor.execute(EditorCommand::ToggleWarpFine)?;
    let first = shown(&mut editor)?;
    let other = ViewportId::new();
    editor.register_viewport(other, ViewportKind::Modeling)?;
    editor.set_viewport_metrics(other, first.transform.metrics);
    let binding = editor.viewport_binding(other).unwrap();
    let second = editor.preview_for(binding)?.canvas.unwrap();
    assert!(editor.present_frame(second));
    let rotate = editor.overlay().handles.last().unwrap().position;
    action(&mut editor, &object, WarpFrameAction::ToggleVisible)?;
    assert!(editor.overlay().handles.is_empty());
    assert_eq!(editor.overlay_for_viewport(binding).handles.len(), 9);
    editor.hover_canvas(first.version.binding, Some(rotate))?;
    assert!(editor.overlay().active_handle.is_none());
    let point = editor.overlay().vertices[0].position;
    click(&mut editor, point, false)?;
    assert_eq!(
        editor
            .warp_transform_target(&object)
            .unwrap()
            .components
            .len(),
        1
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn switching_targets_and_external_geometry_changes_retire_manual_frames() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    action(&mut editor, &object, WarpFrameAction::ToggleAdjust)?;
    editor.execute(EditorCommand::SelectObjects(vec![]))?;
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    shown(&mut editor)?;
    assert!(!frame(&editor, &object).adjusting);
    editor.execute(EditorCommand::AdjustDeformer {
        object: object.clone(),
        adjustment: DeformerAdjustment {
            translation: ModelVector::default(),
            angle: 25.,
            scale: 1.,
        },
    })?;
    shown(&mut editor)?;
    action(&mut editor, &object, WarpFrameAction::Align)?;
    assert!((frame(&editor, &object).frame.radians.to_degrees() - 25.).abs() < 0.001);
    editor.execute(EditorCommand::AdjustDeformer {
        object: object.clone(),
        adjustment: DeformerAdjustment {
            translation: ModelVector { x: 3., y: 0. },
            angle: 0.,
            scale: 1.,
        },
    })?;
    shown(&mut editor)?;
    assert_eq!(frame(&editor, &object).frame.radians, 0.);
    action(&mut editor, &object, WarpFrameAction::Align)?;
    action(&mut editor, &object, WarpFrameAction::Reset)?;
    assert_eq!(frame(&editor, &object).frame.radians, 0.);
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    Ok(())
}

#[test]
fn frame_commands_cannot_capture_geometry_from_a_cancelled_model_candidate() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    editor.execute(EditorCommand::ToggleWarpFine)?;
    let before = control_points(&mut editor, &object)?;
    let start = editor.overlay().handles[3].position;
    pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: ViewportPoint {
                x: start.x + 40.,
                y: start.y,
            },
            left_down: true,
        },
    )?;
    shown(&mut editor)?;
    assert!(editor.document().unwrap().has_pending_edit());
    assert!(action(&mut editor, &object, WarpFrameAction::ToggleAdjust).is_err());
    assert!(!editor.document().unwrap().has_pending_edit());
    assert!(!editor.viewport_interaction_active());
    assert_eq!(control_points(&mut editor, &object)?, before);
    assert!(!frame(&editor, &object).adjusting);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn ambiguous_warp_direction_is_rejected_without_changing_the_frame_or_history() -> anyhow::Result<()>
{
    let dir = tempfile::tempdir()?;
    let mut sdk = zipzi_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::Canvas::new(256., 256., Default::default(), 1.),
    )?;
    let object = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::new_v4().to_string());
    sdk.edit("Ambiguous Warp", None, |edit| {
        edit.create_transform(zipzi_core::Transform {
            id: object.id.clone(),
            data: zipzi_core::TransformData::Warp(zipzi_core::WarpTransform {
                rows: 1,
                columns: 1,
                quad: false,
                bezier: None,
                points: vec![
                    zipzi_core::Vec2::new(32., 32.),
                    zipzi_core::Vec2::new(224., 32.),
                    zipzi_core::Vec2::new(224., 224.),
                    zipzi_core::Vec2::new(32., 224.),
                ],
            }),
            ..Default::default()
        })
    })?;
    let saved = sdk.save_project(&dir.path().join("ambiguous"), None)?;
    let mut editor = crate::support::editor_from_document(Document::open(&saved.manifest)?);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    shown(&mut editor)?;
    let before = frame(&editor, &object);
    let error = action(&mut editor, &object, WarpFrameAction::Align).unwrap_err();
    assert!(error.to_string().contains("stable horizontal direction"));
    assert_eq!(frame(&editor, &object), before);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

use super::common::*;
use crate::support::{pointer_input, present_canvas as shown};
use zipzi_editor_core::*;

fn frame(editor: &Editor, object: &ObjectRef) -> WarpFrameInfo {
    editor
        .warp_frame_info(editor.primary_viewport_binding().unwrap(), object)
        .unwrap()
}
fn frame_handles(editor: &Editor) -> Vec<OverlayHandle> {
    editor
        .overlay()
        .handles
        .into_iter()
        .filter(|h| !h.tangent)
        .collect()
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
fn single_point_frame_commands_use_whole_warp_without_changing_numeric_point_edits()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let before = control_points(&mut editor, &object)?;
    let whole = frame(&editor, &object).frame;
    let point = editor.overlay().vertices[0].position;
    click(&mut editor, point, false)?;
    assert_eq!(frame(&editor, &object).frame, whole);
    assert_eq!(frame_handles(&editor).len(), 9);
    for command in [WarpFrameAction::Align, WarpFrameAction::Reset] {
        action(&mut editor, &object, command)?;
        assert_eq!(frame(&editor, &object).frame, whole);
        assert_eq!(frame_handles(&editor).len(), 9);
    }
    action(&mut editor, &object, WarpFrameAction::ToggleVisible)?;
    assert!(frame_handles(&editor).is_empty());
    assert!(editor.overlay().warp_frame_lines.is_empty());
    action(&mut editor, &object, WarpFrameAction::ToggleAdjust)?;
    let handles = frame_handles(&editor);
    let start = ViewportPoint {
        x: (handles[0].position.x + handles[1].position.x) / 2.,
        y: handles[0].position.y,
    };
    native_drag(
        &mut editor,
        start,
        ViewportPoint {
            x: start.x + 20.,
            y: start.y + 10.,
        },
        false,
    )?;
    let adjusted = frame(&editor, &object).frame;
    assert_eq!(adjusted.half_size, whole.half_size);
    assert!((adjusted.center.x - whole.center.x - 20.).abs() < 0.001);
    assert!((adjusted.center.y - whole.center.y + 10.).abs() < 0.001);
    assert_eq!(control_points(&mut editor, &object)?, before);
    action(&mut editor, &object, WarpFrameAction::ToggleAdjust)?;

    // The numeric dialog still targets the explicit point selection. Its own
    // pivot must not become the unrelated, adjusted whole-object frame center.
    let target = editor.warp_transform_target(&object).unwrap();
    assert_eq!(target.components.len(), 1);
    editor.execute(EditorCommand::AdjustWarp {
        target,
        adjustment: DeformerAdjustment {
            translation: ModelVector { x: 3., y: -2. },
            angle: 90.,
            scale: 2.,
        },
    })?;
    let numeric = control_points(&mut editor, &object)?;
    assert!((numeric[0].x - before[0].x - 3.).abs() < 0.001);
    assert!((numeric[0].y - before[0].y + 2.).abs() < 0.001);
    assert_eq!(&numeric[1..], &before[1..]);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.nudge_selection(
        editor.primary_viewport_binding().unwrap(),
        ModelVector { x: 3., y: -2. },
    )?;
    let after = control_points(&mut editor, &object)?;
    assert!((after[0].x - numeric[0].x - 3.).abs() < 0.001);
    assert!((after[0].y - numeric[0].y + 2.).abs() < 0.001);
    assert_eq!(&after[1..], &before[1..]);
    action(&mut editor, &object, WarpFrameAction::Reset)?;
    assert!(
        frame(&editor, &object)
            .frame
            .half_size
            .iter()
            .all(|size| *size > 0.)
    );
    assert_eq!(frame_handles(&editor).len(), 9);
    Ok(())
}

#[test]
fn frame_adjustment_handles_remain_draggable_when_overlapping_disabled_controls()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let before = control_points(&mut editor, &object)?;
    action(&mut editor, &object, WarpFrameAction::ToggleAdjust)?;
    assert!(editor.overlay().warp_frame_adjusting);
    let overlay = editor.overlay();
    let handles: Vec<_> = overlay.handles.iter().filter(|h| !h.round).collect();
    let corner = handles[0].position;
    let point = overlay
        .vertices
        .iter()
        .min_by(|a, b| {
            let distance = |p: ViewportPoint| (p.x - corner.x).hypot(p.y - corner.y);
            distance(a.position).total_cmp(&distance(b.position))
        })
        .unwrap()
        .position;
    let edge = ViewportPoint {
        x: (handles[1].position.x + handles[2].position.x) / 2.,
        y: handles[1].position.y,
    };
    native_drag(
        &mut editor,
        edge,
        ViewportPoint {
            x: edge.x + point.x - corner.x,
            y: edge.y + point.y - corner.y,
        },
        false,
    )?;
    let aligned = frame(&editor, &object);
    let handle = editor
        .overlay()
        .handles
        .iter()
        .find(|h| !h.round)
        .unwrap()
        .position;
    assert!((handle.x - point.x).hypot(handle.y - point.y) < 0.01);
    native_drag(
        &mut editor,
        handle,
        ViewportPoint {
            x: handle.x - 25.,
            y: handle.y - 25.,
        },
        false,
    )?;
    assert!(frame(&editor, &object).frame.half_size[0] > aligned.frame.half_size[0]);
    assert_eq!(control_points(&mut editor, &object)?, before);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));

    Ok(())
}

#[test]
fn warp_frame_adjustment_move_rotation_scale_and_cancel_leave_the_model_unchanged()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let before = control_points(&mut editor, &object)?;
    action(&mut editor, &object, WarpFrameAction::ToggleAdjust)?;
    let original = frame(&editor, &object);
    for cancel in [true, false] {
        let handles = frame_handles(&editor);
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
    let start = frame_handles(&editor).last().unwrap().position;
    let end = ViewportPoint {
        x: center.x + start.y - center.y,
        y: center.y - start.x + center.x,
    };
    native_drag(&mut editor, start, end, false)?;
    let info = frame(&editor, &object);
    assert!((info.frame.radians - std::f32::consts::FRAC_PI_2).abs() < 0.001);
    let start = frame_handles(&editor)[3].position;
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
    let start = frame_handles(&editor)[3].position;
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
    let handles = frame_handles(&editor);
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
    let first = shown(&mut editor)?;
    let other = ViewportId::new();
    editor.register_viewport(other, ViewportKind::Modeling)?;
    editor.set_viewport_metrics(other, first.transform.metrics);
    let binding = editor.viewport_binding(other).unwrap();
    let second = editor.preview_for(binding)?.canvas.unwrap();
    assert!(editor.present_frame(second));
    let rotate = frame_handles(&editor).last().unwrap().position;
    action(&mut editor, &object, WarpFrameAction::ToggleVisible)?;
    assert!(frame_handles(&editor).is_empty());
    assert_eq!(
        editor
            .overlay_for_viewport(binding)
            .handles
            .iter()
            .filter(|h| !h.tangent)
            .count(),
        9
    );
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
    let before = control_points(&mut editor, &object)?;
    let start = frame_handles(&editor)[3].position;
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

#[test]
fn selected_points_across_warps_share_frame_state_and_numeric_pivot() -> anyhow::Result<()> {
    let (mut document, dir, warp, _, _) = rich_document()?;
    let saved = document.save(&dir.path().join("independent.zipzi"))?;
    let mut sdk = zipzi_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::ModelSetup::default(),
    )?;
    sdk.open_project(&saved.manifest, None)?;
    let mut copy = sdk.transform(&warp).unwrap();
    copy.id = uuid::Uuid::new_v4().to_string();
    copy.runtime_id = "SecondWarp".into();
    for point in &mut copy.warp_mut().unwrap().points {
        point.x += 300.;
    }
    let second = ObjectRef::new(ObjectKind::Deformer, &copy.id);
    sdk.edit("Second Warp", None, |edit| edit.create_transform(copy))?;
    sdk.save_project(&saved.manifest, None)?;
    let first = ObjectRef::new(ObjectKind::Deformer, &warp);
    let mut editor = crate::support::editor_from_document(Document::open(&saved.manifest)?);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 1024.,
        height: 768.,
        scale_factor: 1.,
    });
    editor.execute(EditorCommand::SelectObjects(vec![
        first.clone(),
        second.clone(),
    ]))?;
    let before_a = control_points(&mut editor, &first)?;
    let before_b = control_points(&mut editor, &second)?;
    let canvas = shown(&mut editor)?;
    click(
        &mut editor,
        canvas.transform.to_viewport(before_a[0]),
        false,
    )?;
    click(&mut editor, canvas.transform.to_viewport(before_b[0]), true)?;

    action(&mut editor, &second, WarpFrameAction::ToggleVisible)?;
    assert!(!frame(&editor, &first).visible);
    assert!(frame_handles(&editor).is_empty());
    action(&mut editor, &first, WarpFrameAction::ToggleVisible)?;
    action(&mut editor, &second, WarpFrameAction::ToggleAdjust)?;
    assert_eq!(frame(&editor, &first), frame(&editor, &second));
    assert!(
        editor
            .overlay()
            .vertices
            .iter()
            .all(|point| !point.editable)
    );
    let handles = frame_handles(&editor);
    let start = ViewportPoint {
        x: (handles[1].position.x + handles[2].position.x) / 2.,
        y: handles[1].position.y,
    };
    native_drag(
        &mut editor,
        start,
        ViewportPoint {
            x: start.x + 20.,
            y: start.y + 10.,
        },
        false,
    )?;
    assert_eq!(frame(&editor, &first), frame(&editor, &second));
    let center = frame(&editor, &first).frame.center;
    action(&mut editor, &second, WarpFrameAction::ToggleAdjust)?;
    editor.execute(EditorCommand::AdjustWarp {
        target: editor.warp_transform_target(&second).unwrap(),
        adjustment: DeformerAdjustment {
            translation: ModelVector::default(),
            angle: 90.,
            scale: 1.,
        },
    })?;
    for (object, before) in [(&first, &before_a), (&second, &before_b)] {
        let after = control_points(&mut editor, object)?;
        assert!((after[0].x - (center.x - before[0].y + center.y)).abs() < 0.02);
        assert!((after[0].y - (center.y + before[0].x - center.x)).abs() < 0.02);
        assert_eq!(&after[1..], &before[1..]);
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(control_points(&mut editor, &first)?, before_a);
    assert_eq!(control_points(&mut editor, &second)?, before_b);
    Ok(())
}

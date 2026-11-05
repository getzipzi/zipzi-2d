use super::common::*;
use crate::support::{pointer_input, present_canvas as shown};
use zipzi_editor_core::*;

#[test]
fn normalized_rotation_edits_use_converted_limits_for_base_and_keyforms() -> anyhow::Result<()> {
    use zipzi_core::{
        BindingAxis, BindingAxisEditing, Canvas, Parameter, RotationKeyform, RotationPose,
        RotationTransform, SceneBinding, SceneTrack, Transform, TransformData,
    };
    for unit in [5800., 0.25] {
        for keyed in [false, true] {
            let dir = tempfile::tempdir()?;
            let id = |n| uuid::Uuid::from_u128(n).to_string();
            let mut model = zipzi_core::Document::new();
            assert!(
                model
                    .initialize(id(1), Canvas::new(400., 400., Default::default(), 5800.))
                    .is_ok()
            );
            assert!(
                model
                    .create_imported_transform(Transform {
                        id: id(2),
                        rotation_scale_unit: Some(unit),
                        data: TransformData::Rotation(RotationTransform {
                            pose: RotationPose {
                                scale: unit,
                                ..Default::default()
                            },
                            ..Default::default()
                        }),
                        ..Default::default()
                    })
                    .status
                    .is_ok()
            );
            if keyed {
                assert!(
                    model
                        .create_parameter(Parameter {
                            id: id(3),
                            runtime_id: "Scale".into(),
                            ..Default::default()
                        })
                        .status
                        .is_ok()
                );
                assert!(
                    model
                        .create_scene_binding(SceneBinding {
                            id: id(4),
                            axes: vec![BindingAxis::new(id(3), vec![0., 1.])],
                            track: SceneTrack::Rotation {
                                target_id: id(2).into(),
                                keyforms: [0., 1.]
                                    .into_iter()
                                    .map(|key| RotationKeyform {
                                        keys: vec![key],
                                        rotation: RotationPose {
                                            scale: unit * (1. + key),
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    })
                                    .collect()
                            }
                        })
                        .status
                        .is_ok()
                );
            }
            let mut project = zipzi_project::DocumentSession::from_authoring_document(model);
            let path = dir.path().join("normalized.zipzi.json");
            assert!(project.save(&path).status.is_ok());
            let mut editor = crate::support::editor_from_document(Document::open(&path)?);
            let object = ObjectRef::new(ObjectKind::Deformer, id(2));
            editor.execute(EditorCommand::SelectObject(object.clone()))?;
            let adjust = |angle, scale| EditorCommand::AdjustDeformer {
                object: object.clone(),
                adjustment: DeformerAdjustment {
                    angle,
                    scale,
                    translation: Default::default(),
                },
            };
            let pose = |editor: &Editor| {
                let document = editor.document().unwrap();
                let host = document.runtime_host();
                let runtime = host.read();
                let doc = runtime.document(document.runtime_session_id()).unwrap();
                if keyed {
                    let SceneTrack::Rotation { keyforms, .. } =
                        &doc.binding_for_scene(&object.id).unwrap().track
                    else {
                        panic!("expected rotation binding");
                    };
                    keyforms[0].rotation
                } else {
                    doc.get_transform(&object.id)
                        .unwrap()
                        .rotation()
                        .unwrap()
                        .pose
                }
            };
            editor.execute(adjust(10., 1.))?;
            assert_eq!((pose(&editor).angle, pose(&editor).scale), (10., unit));
            editor.execute(adjust(0., 2.))?;
            assert_eq!(pose(&editor).scale, unit * 2.);
            let history = editor.document().unwrap().history_lengths();
            // Both converted bounds apply, even when the raw value would fit 0.0001..=100.
            for scale in [0.00001, 100.] {
                assert!(editor.execute(adjust(0., scale)).is_err());
                assert_eq!(pose(&editor).scale, unit * 2.);
                assert_eq!(editor.document().unwrap().history_lengths(), history);
            }
            editor.execute(EditorCommand::Undo)?;
            assert_eq!((pose(&editor).angle, pose(&editor).scale), (10., unit));
            editor.execute(EditorCommand::Redo)?;
            assert_eq!(pose(&editor).scale, unit * 2.);
            let document = editor.document().unwrap();
            let host = document.runtime_host();
            let runtime = host.read();
            let doc = runtime.document(document.runtime_session_id()).unwrap();
            assert_eq!(
                doc.get_transform(&object.id).unwrap().rotation_scale_unit,
                Some(unit)
            );
            if keyed {
                assert_eq!(
                    doc.get_transform(&object.id)
                        .unwrap()
                        .rotation()
                        .unwrap()
                        .pose
                        .scale,
                    unit
                );
                let SceneTrack::Rotation { keyforms, .. } =
                    &doc.binding_for_scene(&object.id).unwrap().track
                else {
                    unreachable!()
                };
                assert_eq!(
                    (keyforms[1].rotation.angle, keyforms[1].rotation.scale),
                    (0., unit * 2.)
                );
            }
        }
    }
    Ok(())
}

fn grouped_rotation() -> anyhow::Result<(Editor, tempfile::TempDir, ObjectRef)> {
    let (mut doc, dir, warp, _, _) = rich_document()?;
    let plan = doc.plan_scene_object_creation(
        SceneObjectType::Rotation,
        "Rotation".into(),
        &[ObjectRef::new(ObjectKind::Deformer, warp)],
    )?;
    let object = doc
        .execute_scene_object_creation(&plan)?
        .suggested_selection[0]
        .clone();
    let mut editor = crate::support::editor_from_document(doc);
    editor.reset_view();
    editor.set_canvas_metrics(ViewportMetrics {
        width: 640.,
        height: 640.,
        scale_factor: 2.,
    });
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    shown(&mut editor)?;
    Ok((editor, dir, object))
}

fn bone_length(frame: &CanvasFrame, object: &ObjectRef) -> f32 {
    let control = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == *object)
        .unwrap();
    let gizmo = frame
        .geometry
        .deformer_gizmo(control, frame.transform)
        .unwrap();
    (gizmo.handles[0].1.x - gizmo.center.x).hypot(gizmo.handles[0].1.y - gizmo.center.y)
}

fn imported_rotation_editor() -> anyhow::Result<(Editor, tempfile::TempDir, ObjectRef, ObjectRef)> {
    imported_rotation_editor_in_space(false, false, true, false)
}

fn imported_rotation_editor_in_space(
    reflected_warp: bool,
    reflected_rotation: bool,
    nested_warp: bool,
    with_meshes: bool,
) -> anyhow::Result<(Editor, tempfile::TempDir, ObjectRef, ObjectRef)> {
    use zipzi_core::{
        PreciseVec2, RotationPose, RotationTransform, Transform, TransformData, Vec2, WarpTransform,
    };
    let dir = tempfile::tempdir()?;
    let id = |n| uuid::Uuid::from_u128(n).to_string();
    let mut model = zipzi_core::Document::new();
    assert!(
        model
            .initialize(
                &id(1),
                zipzi_core::Canvas::new(400., 400., Vec2::default(), 3749.)
            )
            .is_ok()
    );
    let y = if reflected_warp { -1. } else { 1. };
    for (n, parent, data) in [
        (
            2,
            None,
            TransformData::Warp(WarpTransform {
                rows: 1,
                columns: 1,
                quad: true,
                bezier: None,
                points: vec![
                    Vec2::new(-200., -200. * y),
                    Vec2::new(200., -200. * y),
                    Vec2::new(-200., 200. * y),
                    Vec2::new(200., 200. * y),
                ],
            }),
        ),
        (
            3,
            Some(2),
            TransformData::Rotation(RotationTransform {
                pose: RotationPose {
                    origin: PreciseVec2::new(0.25, 0.25),
                    scale: 1. / 3749.,
                    reflect_y: reflected_rotation,
                    ..Default::default()
                },
                ..Default::default()
            }),
        ),
        (
            4,
            Some(3),
            TransformData::Warp(WarpTransform {
                rows: 1,
                columns: 1,
                quad: true,
                bezier: None,
                points: vec![
                    Vec2::default(),
                    Vec2::new(374900., 0.),
                    Vec2::new(0., 374900.),
                    Vec2::new(374900., 374900.),
                ],
            }),
        ),
        (
            5,
            Some(if nested_warp { 4 } else { 3 }),
            TransformData::Rotation(RotationTransform {
                pose: RotationPose {
                    origin: PreciseVec2::new(0.75, 0.5),
                    ..Default::default()
                },
                ..Default::default()
            }),
        ),
    ] {
        let result = model.create_imported_transform(Transform {
            id: id(n),
            parent_id: parent.map(id).map(Into::into),
            data,
            ..Default::default()
        });
        assert!(result.status.is_ok(), "{:?}", result.status);
    }
    if with_meshes {
        for (n, x, enabled) in [(6, 0., true), (7, -374900., false)] {
            let result = model.create_mesh(zipzi_core::Mesh {
                id: id(n),
                deformer_id: id(5),
                enabled,
                vertex_ids: vec![0, 1, 2],
                base_positions: vec![
                    Vec2::new(x, 0.),
                    Vec2::new(x + 374900., 0.),
                    Vec2::new(x, 149960.),
                ],
                uvs: vec![Vec2::default(), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                triangles: vec![[0, 1, 2]],
                ..Default::default()
            });
            assert!(result.status.is_ok(), "{:?}", result.status);
        }
    }
    let mut project = zipzi_project::DocumentSession::from_authoring_document(model);
    let path = dir.path().join("imported.zipzi.json");
    let saved = project.save(&path);
    assert!(saved.status.is_ok(), "{:?}", saved.status);
    let document = Document::open(&path)?;
    assert!(!document.modified());
    let mut editor = crate::support::editor_from_document(document);
    editor.reset_view();
    editor.set_canvas_metrics(ViewportMetrics {
        width: 800.,
        height: 600.,
        scale_factor: 2.,
    });
    Ok((
        editor,
        dir,
        ObjectRef::new(ObjectKind::Deformer, id(3)),
        ObjectRef::new(ObjectKind::Deformer, id(5)),
    ))
}

#[test]
fn imported_bones_normalize_coordinate_units_and_keep_zoom_picking_and_pose_scaling()
-> anyhow::Result<()> {
    let (mut editor, _dir, parent, child) = imported_rotation_editor()?;
    for zoom in [0.1, 0.5, 1., 2.] {
        editor.set_zoom_at(zoom, None);
        editor.execute(EditorCommand::SelectObjects(Vec::new()))?;
        let frame = shown(&mut editor)?;
        assert_eq!(editor.overlay().rotation_bones.len(), 2);
        for object in [&parent, &child] {
            assert!((bone_length(&frame, object) - 128. * zoom).abs() < 0.001);
            let control = frame
                .geometry
                .controls
                .iter()
                .find(|c| c.object == *object)
                .unwrap();
            let g = frame
                .geometry
                .deformer_gizmo(control, frame.transform)
                .unwrap();
            let tip = g.handles[0].1;
            assert!((tip.x - g.center.x).abs() < 0.001);
            assert!((tip.y - g.center.y + 128. * zoom).abs() < 0.001);
            let point = rotation_body_pointer(g.center, tip);
            assert!(
                editor
                    .pick_candidates(point, Some(frame.version))
                    .iter()
                    .any(|c| c.object == *object)
            );
        }
        for bone in editor.overlay().rotation_bones {
            assert!((bone.scale - zoom).abs() < 0.001);
            assert!(((bone.points[1].x - bone.points[3].x).abs() - 16. * zoom).abs() < 0.001);
        }
    }
    editor.set_zoom_at(1., None);
    editor.execute(EditorCommand::SelectObject(child.clone()))?;
    let original = shown(&mut editor)?;
    let c = original
        .geometry
        .controls
        .iter()
        .find(|c| c.object == child)
        .unwrap();
    let g = original
        .geometry
        .deformer_gizmo(c, original.transform)
        .unwrap();
    let tip = g.handles[0].1;
    let end = rotation_scale_pointer(g.center, tip, 1.5);
    assert_eq!(
        original.geometry.hit(
            tip,
            original.transform,
            &[child.clone()],
            ToolId::Select,
            DeformerVisibility::default()
        ),
        Some(HitTarget::DeformerHandle(
            child.clone(),
            DeformerHandle::RotationScale
        ))
    );
    pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: tip,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    pointer_input(&mut editor, CanvasInput::Up { point: end })?;
    assert!((bone_length(&shown(&mut editor)?, &child) - 192.).abs() < 0.001);
    editor.execute(EditorCommand::AdjustDeformer {
        object: parent,
        adjustment: DeformerAdjustment {
            scale: 2.,
            angle: 0.,
            translation: ModelVector::default(),
        },
    })?;
    assert!((bone_length(&shown(&mut editor)?, &child) - 384.).abs() < 0.002);
    editor.execute(EditorCommand::Undo)?;
    editor.execute(EditorCommand::Undo)?;
    assert!((bone_length(&shown(&mut editor)?, &child) - 128.).abs() < 0.001);
    editor.execute(EditorCommand::Redo)?;
    assert!((bone_length(&shown(&mut editor)?, &child) - 192.).abs() < 0.001);
    Ok(())
}

#[test]
fn imported_bone_direction_tracks_parent_and_own_angles() -> anyhow::Result<()> {
    let (mut editor, _dir, parent, child) = imported_rotation_editor()?;
    for (object, angle, expected) in [(&parent, 23., 113_f32), (&child, -41., 72.)] {
        editor.execute(EditorCommand::AdjustDeformer {
            object: object.clone(),
            adjustment: DeformerAdjustment {
                angle,
                scale: 1.,
                translation: ModelVector::default(),
            },
        })?;
        let frame = shown(&mut editor)?;
        let control = frame
            .geometry
            .controls
            .iter()
            .find(|c| c.object == child)
            .unwrap();
        let g = frame
            .geometry
            .deformer_gizmo(control, frame.transform)
            .unwrap();
        let tip = g.handles[0].1;
        let length = bone_length(&frame, &child);
        assert!(((tip.x - g.center.x) / length - expected.to_radians().cos()).abs() < 0.001);
        assert!(((tip.y - g.center.y) / length + expected.to_radians().sin()).abs() < 0.001);
        let body = rotation_body_pointer(g.center, tip);
        assert!(
            editor
                .pick_candidates(body, Some(frame.version))
                .iter()
                .any(|c| c.object == child)
        );
    }
    Ok(())
}

#[test]
fn imported_bone_length_and_scale_reference_survive_save_and_reopen() -> anyhow::Result<()> {
    let (mut editor, dir, _, child) = imported_rotation_editor()?;
    editor.execute(EditorCommand::SelectObject(child.clone()))?;
    let frame = shown(&mut editor)?;
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == child)
        .unwrap();
    let g = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
    let tip = g.handles[0].1;
    let end = rotation_scale_pointer(g.center, tip, 1.5);
    pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: tip,
            shift: false,
            alt: true,
            primary: false,
        },
    )?;
    pointer_input(&mut editor, CanvasInput::Up { point: end })?;
    assert!((bone_length(&shown(&mut editor)?, &child) - 192.).abs() < 0.001);
    editor.execute(EditorCommand::AdjustDeformer {
        object: child.clone(),
        adjustment: DeformerAdjustment {
            scale: 2.,
            angle: 0.,
            translation: ModelVector::default(),
        },
    })?;
    assert!((bone_length(&shown(&mut editor)?, &child) - 384.).abs() < 0.002);
    let before_save = shown(&mut editor)?;
    let axis = before_save
        .geometry
        .controls
        .iter()
        .find(|c| c.object == child)
        .unwrap()
        .rotation_axis;
    let document = editor.document().unwrap();
    let mut saving =
        Document::attach_runtime(document.runtime_host(), document.runtime_session_id())?;
    let saved = saving.save(&dir.path().join("saved.zipzi"))?;
    let doc = Document::open(&saved.path)?;
    assert!(!doc.modified());
    let mut reopened = crate::support::editor_from_document(doc);
    reopened.reset_view();
    reopened.set_canvas_metrics(ViewportMetrics {
        width: 800.,
        height: 600.,
        scale_factor: 2.,
    });
    let after_reopen = shown(&mut reopened)?;
    assert!((bone_length(&after_reopen, &child) - 384.).abs() < 0.002);
    assert_eq!(
        after_reopen
            .geometry
            .controls
            .iter()
            .find(|c| c.object == child)
            .unwrap()
            .rotation_axis,
        axis
    );
    Ok(())
}

#[test]
fn scaling_bone_and_handles_follow_each_presented_frame_in_every_viewport() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = grouped_rotation()?;
    let original = shown(&mut editor)?;
    let second = ViewportId::new();
    editor.register_viewport(second, ViewportKind::Modeling)?;
    editor.set_viewport_metrics(
        second,
        ViewportMetrics {
            width: 480.,
            height: 400.,
            scale_factor: 1.,
        },
    );
    let second = editor.viewport_binding(second).unwrap();
    editor.set_viewport_zoom(second, 0.5, None)?;
    let peer = editor.preview_for(second)?.canvas.unwrap();
    assert!(editor.present_frame(peer.clone()));
    let start = editor.overlay().handles[0].position;
    let control = original
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let center = original
        .geometry
        .deformer_gizmo(control, original.transform)
        .unwrap()
        .center;
    pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    let mut frames = vec![(original, peer)];
    for ratio in [1.5, 2.] {
        pointer_input(
            &mut editor,
            CanvasInput::Move {
                point: rotation_scale_pointer(center, start, ratio),
                left_down: true,
            },
        )?;
        frames.push((
            editor.preview()?.canvas.unwrap(),
            editor.preview_for(second)?.canvas.unwrap(),
        ));
    }
    for (index, (main, peer)) in frames.into_iter().enumerate() {
        if index > 0 {
            assert!(editor.present_frame(main.clone()));
            assert!(editor.present_frame(peer.clone()));
        }
        for frame in [main, peer] {
            let c = frame
                .geometry
                .controls
                .iter()
                .find(|c| c.object == object)
                .unwrap();
            let gizmo = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
            let expected = c.rotation_bone_length * [1., 1.5, 2.][index] * frame.transform.scale;
            assert!((bone_length(&frame, &object) - expected).abs() < 0.001);
            let overlay = editor.overlay_for_viewport(frame.version.binding);
            let near = |a: ViewportPoint, b: ViewportPoint| {
                assert!((a.x - b.x).hypot(a.y - b.y) < 0.001, "{a:?} vs {b:?}");
            };
            near(overlay.handles[0].position, gizmo.handles[0].1);
            assert_eq!(overlay.rotation_lines.len(), gizmo.outline.len());
            for ((a, b), (c, d)) in overlay.rotation_lines.into_iter().zip(gizmo.outline) {
                near(a, c);
                near(b, d);
            }
        }
    }
    pointer_input(&mut editor, CanvasInput::Cancel)?;
    Ok(())
}

#[test]
fn command_rotation_controls_keep_children_fixed_and_cancel_return_and_undo_restore_all_channels()
-> anyhow::Result<()> {
    for handle in 0..3 {
        let (mut editor, _dir, object) = grouped_rotation()?;
        let frame = shown(&mut editor)?;
        let c = frame
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap();
        let gizmo = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
        let start = match handle {
            0 => gizmo.center,
            1 => rotation_body_pointer(gizmo.center, gizmo.handles[0].1),
            _ => gizmo.handles[0].1,
        };
        let end = if handle == 0 {
            ViewportPoint {
                x: start.x + 14.,
                y: start.y + 9.,
            }
        } else if handle == 2 {
            rotation_scale_pointer(gizmo.center, start, 1.5)
        } else {
            let (sin, cos) = 23.4f32.to_radians().sin_cos();
            let (x, y) = (start.x - gizmo.center.x, start.y - gizmo.center.y);
            ViewportPoint {
                x: gizmo.center.x + x * cos - y * sin,
                y: gizmo.center.y + x * sin + y * cos,
            }
        };
        let children = |frame: &CanvasFrame| {
            frame
                .geometry
                .meshes
                .iter()
                .flat_map(|m| m.vertices.iter().map(|(_, p)| *p))
                .collect::<Vec<_>>()
        };
        let before = children(&frame);
        let origin = c.points.clone();
        let history = editor.document().unwrap().history_lengths();
        for finish in 0..3 {
            pointer_input(
                &mut editor,
                CanvasInput::Down {
                    point: start,
                    shift: false,
                    alt: false,
                    primary: true,
                },
            )?;
            // Repeated previews must always compensate from the press state.
            for _ in 0..2 {
                pointer_input(
                    &mut editor,
                    CanvasInput::Move {
                        point: end,
                        left_down: true,
                    },
                )?;
                assert!(editor.overlay().deformer_keep_children);
                for (shift, alt) in [(true, false), (true, true), (false, false)] {
                    pointer_input(&mut editor, CanvasInput::ModifiersChanged { shift, alt })?;
                    let after = shown(&mut editor)?;
                    for (a, b) in before.iter().zip(children(&after)) {
                        assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
                    }
                    let c = after
                        .geometry
                        .controls
                        .iter()
                        .find(|c| c.object == object)
                        .unwrap();
                    let gizmo = after.geometry.deformer_gizmo(c, after.transform).unwrap();
                    assert_eq!(editor.overlay().handles[0].position, gizmo.handles[0].1);
                }
                let after = shown(&mut editor)?;
                for (a, b) in before.iter().zip(children(&after)) {
                    assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
                }
                if finish == 1 {
                    pointer_input(
                        &mut editor,
                        CanvasInput::Move {
                            point: start,
                            left_down: true,
                        },
                    )?;
                }
            }
            pointer_input(
                &mut editor,
                match finish {
                    0 => CanvasInput::Cancel,
                    1 => CanvasInput::Up { point: start },
                    _ => CanvasInput::Up { point: end },
                },
            )?;
            let after = shown(&mut editor)?;
            if finish < 2 {
                assert_eq!(editor.document().unwrap().history_lengths(), history);
                assert_eq!(control_points(&mut editor, &object)?, origin);
                assert_eq!(children(&after), before);
            }
        }
        assert_eq!(
            editor.document().unwrap().history_lengths(),
            (history.0 + 1, 0)
        );
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(children(&shown(&mut editor)?), before);
        assert_eq!(control_points(&mut editor, &object)?, origin);
        editor.execute(EditorCommand::Redo)?;
        let redo = shown(&mut editor)?;
        for (a, b) in before.iter().zip(children(&redo)) {
            assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
        }
    }
    Ok(())
}

#[test]
fn bone_initializes_from_descendant_mesh_size_and_follows_own_parent_scale_and_zoom()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = grouped_rotation()?;
    let frame = shown(&mut editor)?;
    let control = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let origin = control.points[0];
    let initial = bone_length(&frame, &object);
    assert!((initial - 65.28).abs() < 0.001);
    assert!((origin.x - 128.).abs() < 0.001 && (origin.y - 128.).abs() < 0.001);
    let child = frame
        .geometry
        .controls
        .iter()
        .find(|c| {
            c.kind == ControlKind::Warp && frame.geometry.ancestors(&c.object).contains(&object)
        })
        .unwrap()
        .object
        .clone();
    editor.execute(EditorCommand::AdjustDeformer {
        object: child,
        adjustment: DeformerAdjustment {
            angle: 0.,
            scale: 4.,
            translation: ModelVector::default(),
        },
    })?;
    let larger_children = shown(&mut editor)?;
    let content_radius = larger_children
        .geometry
        .member_meshes(&object)
        .iter()
        .flat_map(|m| &m.vertices)
        .map(|(_, p)| (p.x - origin.x).hypot(p.y - origin.y))
        .fold(0., f32::max);
    assert!(content_radius * frame.transform.scale > initial * 2.);
    assert!((bone_length(&larger_children, &object) - initial).abs() < 0.001);
    editor.execute(EditorCommand::AdjustDeformer {
        object: object.clone(),
        adjustment: DeformerAdjustment {
            angle: 37.,
            scale: 2.,
            translation: ModelVector::default(),
        },
    })?;
    let scaled = shown(&mut editor)?;
    assert!((bone_length(&scaled, &object) - initial * 2.).abs() < 0.001);

    let plan = editor.document().unwrap().plan_scene_object_creation(
        SceneObjectType::Rotation,
        "Parent".into(),
        std::slice::from_ref(&object),
    )?;
    let parent = editor
        .execute_scene_object_creation(editor.active_id().unwrap(), &plan)?
        .suggested_selection[0]
        .clone();
    editor.execute(EditorCommand::AdjustDeformer {
        object: parent,
        adjustment: DeformerAdjustment {
            angle: -23.,
            scale: 1.5,
            translation: ModelVector::default(),
        },
    })?;
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    let inherited = shown(&mut editor)?;
    assert!((bone_length(&inherited, &object) - initial * 3.).abs() < 0.001);
    for zoom in [MIN_ZOOM, 0.5, 2., MAX_ZOOM] {
        editor.set_zoom_at(zoom, None);
        let zoomed = shown(&mut editor)?;
        let expected = initial * 3. * zoom;
        // Viewport endpoints are f32; absolute precision scales with zoom.
        assert!(
            (bone_length(&zoomed, &object) - expected).abs() < 0.002_f32.max(expected.abs() * 1e-6)
        );
    }
    Ok(())
}

#[test]
fn unselected_bone_is_visible_and_pickable_without_deformer_helpers() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(true)?;
    let history = editor.document().unwrap().history_lengths();
    for zoom in [0.1, 0.5, 1., 2., 4.] {
        editor.set_zoom_at(zoom, None);
        editor.execute(EditorCommand::SelectObjects(Vec::new()))?;
        let frame = shown(&mut editor)?;
        assert_eq!(editor.deformer_visibility(), DeformerVisibility::default());
        let overlay = editor.overlay();
        let bone = overlay.rotation_bones.iter().find(|b| !b.selected).unwrap();
        assert!((bone.scale - zoom).abs() < 0.001);
        // Width and length both follow zoom, including below the former minimum.
        assert!((bone_length(&frame, &object) - 128. * zoom).abs() < 0.001);
        assert!(((bone.points[1].y - bone.points[3].y).abs() - 16. * zoom).abs() < 0.001);
        let body = ViewportPoint {
            x: bone.points[1].x,
            y: (bone.points[1].y + bone.points[3].y) * 0.5,
        };
        let points = [bone.points[0], body, bone.points[2]];
        for point in points {
            editor.execute(EditorCommand::SelectObjects(Vec::new()))?;
            let frame = shown(&mut editor)?;
            assert_eq!(
                frame.geometry.hit(
                    point,
                    frame.transform,
                    &[],
                    ToolId::Select,
                    DeformerVisibility::default()
                ),
                Some(HitTarget::Object(object.clone()))
            );
            assert!(
                editor
                    .pick_candidates(point, Some(frame.version))
                    .iter()
                    .any(|c| c.object == object)
            );
            click(&mut editor, point, false)?;
            assert_eq!(
                editor.document().unwrap().context().selections,
                [object.clone()]
            );
        }
    }
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    Ok(())
}

#[test]
fn overlapping_bones_all_participate_in_candidates_and_respect_locks_and_visibility()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = grouped_rotation()?;
    editor.execute(EditorCommand::SelectObjects(Vec::new()))?;
    let frame = shown(&mut editor)?;
    let bones: Vec<_> = frame
        .geometry
        .controls
        .iter()
        .filter(|c| c.kind == ControlKind::Rotation)
        .map(|c| c.object.clone())
        .collect();
    assert_eq!(bones.len(), 2);
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let gizmo = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
    let point = rotation_body_pointer(gizmo.center, gizmo.handles[0].1);
    let candidates = editor.pick_candidates(point, Some(frame.version));
    for bone in &bones {
        assert!(candidates.iter().any(|c| c.object == *bone));
    }
    editor.execute(EditorCommand::SetObjectsLocked {
        objects: bones.clone(),
        locked: true,
    })?;
    let frame = shown(&mut editor)?;
    assert!(
        editor
            .pick_candidates(point, Some(frame.version))
            .iter()
            .all(|c| !bones.contains(&c.object))
    );
    editor.execute(EditorCommand::SetObjectsLocked {
        objects: bones.clone(),
        locked: false,
    })?;
    for bone in &bones {
        editor.execute(EditorCommand::SetObjectVisible {
            object: bone.clone(),
            visible: false,
        })?;
    }
    let frame = shown(&mut editor)?;
    assert!(editor.overlay().rotation_bones.is_empty());
    assert!(
        editor
            .pick_candidates(point, Some(frame.version))
            .iter()
            .all(|c| !bones.contains(&c.object))
    );
    Ok(())
}

#[test]
fn rotation_controls_keep_plain_hover_and_drag_overlays_with_matching_scale_geometry()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = grouped_rotation()?;
    let frame = shown(&mut editor)?;
    let control = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let gizmo = frame
        .geometry
        .deformer_gizmo(control, frame.transform)
        .unwrap();
    let initial_radius = bone_length(&frame, &object);
    let start = rotation_body_pointer(gizmo.center, gizmo.handles[0].1);
    let baseline = editor.overlay();
    let tip = frame.geometry.control_points(control, frame.transform)[1];
    for point in [gizmo.center, tip, gizmo.handles[0].1, start] {
        editor.hover_canvas(frame.version.binding, Some(point))?;
        let hovered = editor.overlay();
        assert!(hovered.active_handle.is_some() || hovered.active_component.is_some());
        if point == gizmo.center {
            assert!(hovered.vertices.iter().any(|v| v.hovered));
        } else if point == tip {
            assert!(hovered.handles.iter().any(|h| h.hovered));
        }
        assert!(hovered.deformers.iter().all(|d| !d.hovered));
        assert_eq!(hovered.rotation_lines, baseline.rotation_lines);
    }
    assert_eq!(editor.overlay().active_handle, Some(DeformerHandle::Rotate));
    let center = gizmo.center;
    let end = ViewportPoint {
        x: center.x - (start.y - center.y),
        y: center.y + (start.x - center.x),
    };
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
            point: end,
            left_down: true,
        },
    )?;
    shown(&mut editor)?;
    let overlay = editor.overlay();
    assert_eq!(overlay.rotation_lines.len(), baseline.rotation_lines.len());
    assert_eq!(overlay.guide_lines, baseline.guide_lines);
    assert_eq!(overlay.gizmo_lines, baseline.gizmo_lines);
    assert!(overlay.interaction_label.unwrap().contains("90.0°"));
    pointer_input(&mut editor, CanvasInput::Cancel)?;
    let frame = shown(&mut editor)?;
    assert!((bone_length(&frame, &object) - initial_radius).abs() < 0.001);
    let start = editor.overlay().handles[0].position;
    let end = rotation_scale_pointer(center, start, 2.);
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
            point: end,
            left_down: true,
        },
    )?;
    shown(&mut editor)?;
    let handle = editor.overlay().handles[0].position;
    assert!((handle.x - end.x).hypot(handle.y - end.y) < 0.001);
    let index = editor.document().unwrap().object_index();
    assert!(
        (index.get(&object).unwrap().properties["rotation"]["pose"]["scale"]
            .as_f64()
            .unwrap()
            - 2.)
            .abs()
            < 0.001
    );
    pointer_input(&mut editor, CanvasInput::Up { point: end })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 0));
    Ok(())
}

#[test]
fn alt_rotation_tip_length_is_pose_independent_captured_and_one_undo_step() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = grouped_rotation()?;
    let original = shown(&mut editor)?;
    let control = original
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let gizmo = original
        .geometry
        .deformer_gizmo(control, original.transform)
        .unwrap();
    let start = gizmo.handles[0].1;
    let end = rotation_scale_pointer(gizmo.center, start, 1.5);
    let pose = editor
        .document()
        .unwrap()
        .object_index()
        .get(&object)
        .unwrap()
        .properties
        .clone();
    let meshes = |frame: &CanvasFrame| {
        frame
            .geometry
            .meshes
            .iter()
            .map(|m| m.vertices.clone())
            .collect::<Vec<_>>()
    };
    let children = meshes(&original);
    let history = editor.document().unwrap().history_lengths();
    editor.hover_canvas(original.version.binding, Some(start))?;
    editor.update_canvas_modifiers(original.version.binding, false, true)?;
    assert!(editor.overlay().rotation_length_adjusting);
    assert!(!editor.overlay().handles[0].selected);
    for finish in 0..3 {
        pointer_input(
            &mut editor,
            CanvasInput::Down {
                point: start,
                shift: false,
                alt: true,
                primary: false,
            },
        )?;
        pointer_input(
            &mut editor,
            CanvasInput::Move {
                point: end,
                left_down: true,
            },
        )?;
        // Releasing Alt keeps the length operation chosen at press time.
        pointer_input(
            &mut editor,
            CanvasInput::ModifiersChanged {
                shift: false,
                alt: false,
            },
        )?;
        let frame = shown(&mut editor)?;
        assert!(
            (bone_length(&frame, &object) - bone_length(&original, &object) * 1.5).abs() < 0.001
        );
        assert!(editor.overlay().rotation_length_adjusting);
        assert!(!editor.overlay().handles[0].selected);
        assert_eq!(meshes(&frame), children);
        assert_eq!(
            editor
                .document()
                .unwrap()
                .object_index()
                .get(&object)
                .unwrap()
                .properties,
            pose
        );
        let end_input = match finish {
            0 => CanvasInput::Cancel,
            1 => CanvasInput::Up { point: start },
            _ => CanvasInput::Up { point: end },
        };
        pointer_input(&mut editor, end_input)?;
        let frame = shown(&mut editor)?;
        if finish < 2 {
            assert!((bone_length(&frame, &object) - bone_length(&original, &object)).abs() < 0.001);
            assert_eq!(editor.document().unwrap().history_lengths(), history);
        }
    }
    assert_eq!(
        editor.document().unwrap().history_lengths(),
        (history.0 + 1, 0)
    );
    editor.execute(EditorCommand::Undo)?;
    assert!(
        (bone_length(&shown(&mut editor)?, &object) - bone_length(&original, &object)).abs()
            < 0.001
    );
    editor.execute(EditorCommand::Redo)?;
    assert!(
        (bone_length(&shown(&mut editor)?, &object) - bone_length(&original, &object) * 1.5).abs()
            < 0.001
    );
    Ok(())
}

#[test]
fn rotation_tip_scale_ignores_perpendicular_motion_and_keeps_the_press_operation()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(true)?;
    let original = shown(&mut editor)?;
    let control = original
        .geometry
        .controls
        .iter()
        .find(|c| c.object == object)
        .unwrap();
    let gizmo = original
        .geometry
        .deformer_gizmo(control, original.transform)
        .unwrap();
    let start = gizmo.handles[0].1;
    let (dx, dy) = (start.x - gizmo.center.x, start.y - gizmo.center.y);
    let span = dx.hypot(dy);
    let side = ViewportPoint {
        x: start.x - dy / span * 30.,
        y: start.y + dx / span * 30.,
    };
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
            point: side,
            left_down: true,
        },
    )?;
    assert_eq!(
        editor
            .document()
            .unwrap()
            .object_index()
            .get(&object)
            .unwrap()
            .properties["rotation"]["pose"]["scale"],
        1.
    );
    pointer_input(
        &mut editor,
        CanvasInput::ModifiersChanged {
            shift: false,
            alt: true,
        },
    )?;
    let end = ViewportPoint {
        x: side.x + dx * 0.5,
        y: side.y + dy * 0.5,
    };
    pointer_input(&mut editor, CanvasInput::Up { point: end })?;
    let properties = editor
        .document()
        .unwrap()
        .object_index()
        .get(&object)
        .unwrap()
        .properties
        .clone();
    assert_eq!(properties["rotation"]["pose"]["scale"], 1.5);
    assert_eq!(properties["rotation"]["pose"]["angle"], 0.);
    let frame = shown(&mut editor)?;
    assert_eq!(
        frame
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap()
            .rotation_bone_length,
        128.
    );
    Ok(())
}

#[test]
fn imported_rotation_drag_follows_cursor_under_reflected_parents() -> anyhow::Result<()> {
    for (warp, rotation, nested) in [
        (true, false, true),
        (false, true, false),
        (true, true, true),
        (false, false, true),
    ] {
        for degrees in [-35_f32, 35.] {
            let (mut editor, _dir, parent, child) =
                imported_rotation_editor_in_space(warp, rotation, nested, false)?;
            for object in [&parent, &child] {
                editor.execute(EditorCommand::SelectObject(object.clone()))?;
                let frame = shown(&mut editor)?;
                let c = frame
                    .geometry
                    .controls
                    .iter()
                    .find(|c| c.object == *object)
                    .unwrap();
                let g = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
                // Test rotation on the bone surface, away from crossing parent lines.
                let body = |g: &DeformerGizmo, weights: [f32; 4]| ViewportPoint {
                    x: g.outline
                        .iter()
                        .zip(weights)
                        .map(|((p, _), w)| p.x * w)
                        .sum(),
                    y: g.outline
                        .iter()
                        .zip(weights)
                        .map(|((p, _), w)| p.y * w)
                        .sum(),
                };
                let weights = [
                    [0.25; 4],
                    [0.1, 0.7, 0.1, 0.1],
                    [0.1, 0.1, 0.1, 0.7],
                    [0.1, 0.4, 0.1, 0.4],
                ]
                .into_iter()
                .find(|&weights| {
                    frame.geometry.hit(
                        body(&g, weights),
                        frame.transform,
                        std::slice::from_ref(object),
                        ToolId::Select,
                        DeformerVisibility::default(),
                    ) == Some(HitTarget::RotationHandle(object.clone()))
                })
                .expect("bone must have an unobscured surface");
                let start = body(&g, weights);
                let (sin, cos) = degrees.to_radians().sin_cos();
                let (x, y) = (start.x - g.center.x, start.y - g.center.y);
                let end = ViewportPoint {
                    x: g.center.x + x * cos - y * sin,
                    y: g.center.y + x * sin + y * cos,
                };
                pointer_input(
                    &mut editor,
                    CanvasInput::Down {
                        point: start,
                        shift: false,
                        alt: false,
                        primary: false,
                    },
                )?;
                pointer_input(&mut editor, CanvasInput::Up { point: end })?;
                let after = shown(&mut editor)?;
                let c = after
                    .geometry
                    .controls
                    .iter()
                    .find(|c| c.object == *object)
                    .unwrap();
                let g = after.geometry.deformer_gizmo(c, after.transform).unwrap();
                let body_point = body(&g, weights);
                assert!(
                    (body_point.x - end.x).hypot(body_point.y - end.y) < 0.01,
                    "warp={warp} rotation={rotation} nested={nested} degrees={degrees}: {body_point:?} != {end:?}"
                );
                editor.execute(EditorCommand::Undo)?;
                let restored = shown(&mut editor)?;
                let c = restored
                    .geometry
                    .controls
                    .iter()
                    .find(|c| c.object == *object)
                    .unwrap();
                let g = restored
                    .geometry
                    .deformer_gizmo(c, restored.transform)
                    .unwrap();
                let body_point = body(&g, weights);
                assert!((body_point.x - start.x).hypot(body_point.y - start.y) < 0.01);
                editor.execute(EditorCommand::Redo)?;
            }
        }
    }
    Ok(())
}

#[test]
fn selected_rotation_does_not_capture_child_mesh_clicks() -> anyhow::Result<()> {
    for tool in [ToolId::Select, ToolId::Lasso] {
        let (mut editor, _dir, rotation) = grouped_rotation()?;
        editor.execute(EditorCommand::SetTool(tool))?;
        let frame = shown(&mut editor)?;
        let mesh = ObjectRef::new(
            ObjectKind::Mesh,
            &frame.geometry.member_meshes(&rotation)[0].id,
        );
        let [x0, y0, x1, y1] = frame.geometry.mesh(&mesh.id).unwrap().bounds.unwrap();
        let point = (1..10)
            .flat_map(|x| (1..10).map(move |y| (x, y)))
            .find_map(|(x, y)| {
                let p = frame.transform.to_viewport(ModelPoint {
                    x: x0 + (x1 - x0) * x as f32 / 10.,
                    y: y0 + (y1 - y0) * y as f32 / 10.,
                });
                (frame.geometry.hit(
                    p,
                    frame.transform,
                    &[rotation.clone()],
                    tool,
                    DeformerVisibility::default(),
                ) == Some(HitTarget::Object(mesh.clone())))
                .then_some(p)
            })
            .expect("child mesh must have a hittable surface outside the bone");
        let history = editor.document().unwrap().history_lengths();
        click(&mut editor, point, false)?;
        assert_eq!(
            editor.document().unwrap().context().selections,
            [mesh.clone()]
        );
        editor.execute(EditorCommand::SelectObject(rotation.clone()))?;
        click(&mut editor, point, true)?;
        assert_eq!(
            editor.document().unwrap().context().selections,
            [rotation, mesh]
        );
        assert_eq!(editor.document().unwrap().history_lengths(), history);
    }
    Ok(())
}

#[test]
fn rotation_handle_hit_areas_stay_fixed_as_bone_zoom_changes() -> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(true)?;
    for zoom in [0.25, 1., 8.] {
        editor.set_zoom_at(zoom, None);
        let frame = shown(&mut editor)?;
        let control = frame
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap();
        let g = frame
            .geometry
            .deformer_gizmo(control, frame.transform)
            .unwrap();
        let hit = |p| {
            frame.geometry.hit(
                p,
                frame.transform,
                &[object.clone()],
                ToolId::Select,
                DeformerVisibility::default(),
            )
        };
        assert_eq!(
            hit(ViewportPoint {
                x: g.center.x,
                y: g.center.y + 6.
            }),
            Some(HitTarget::Component(ComponentRef {
                object: object.clone(),
                id: 0
            }))
        );
        assert_ne!(
            hit(ViewportPoint {
                x: g.center.x,
                y: g.center.y + 12.
            }),
            Some(HitTarget::Component(ComponentRef {
                object: object.clone(),
                id: 0
            }))
        );
        let tip = g.handles[0].1;
        assert_eq!(
            hit(ViewportPoint {
                x: tip.x,
                y: tip.y + 6.
            }),
            Some(HitTarget::DeformerHandle(
                object.clone(),
                DeformerHandle::RotationScale
            ))
        );
        assert_ne!(
            hit(ViewportPoint {
                x: tip.x,
                y: tip.y + 12.
            }),
            Some(HitTarget::DeformerHandle(
                object.clone(),
                DeformerHandle::RotationScale
            ))
        );
    }
    Ok(())
}

#[test]
fn imported_bone_lengths_use_nested_mesh_bounds_and_persist_without_resizing() -> anyhow::Result<()>
{
    let (mut editor, dir, parent, child) =
        imported_rotation_editor_in_space(false, false, true, true)?;
    let frame = shown(&mut editor)?;
    // The two descendant Meshes span 200 x 40 model pixels, one hidden.
    // Both bones point up: the child's origin is on the lower edge, while
    // the parent's origin is below it. Stop 15% inside the forward box segment.
    assert!((bone_length(&frame, &child) - 34.).abs() < 0.001);
    let parent_length = bone_length(&frame, &parent);
    assert!((parent_length - 84.).abs() < 0.001);

    editor.execute(EditorCommand::SelectObject(child.clone()))?;
    let document = editor.document().unwrap();
    let mut editing =
        Document::attach_runtime(document.runtime_host(), document.runtime_session_id())?;
    editing.set_object_property(&child, "rotation.bone_length", serde_json::json!(180.))?;
    let saved = editing.save(&dir.path().join("length.zipzi"))?;
    let reopened = Document::open(&saved.path)?;
    assert!(!reopened.modified());
    let key = zipzi_editor_core::property::deformer::BONE_LENGTH.key;
    assert_eq!(
        reopened.property_values(&child)?[&key],
        PropertyValue::Float(180.)
    );
    assert!(
        (reopened.property_values(&parent)?[&key]
            .text()
            .parse::<f32>()?
            - parent_length)
            .abs()
            < 0.001
    );
    let mut reopened_editor = crate::support::editor_from_document(reopened);
    reopened_editor.reset_view();
    reopened_editor.set_canvas_metrics(ViewportMetrics {
        width: 800.,
        height: 600.,
        scale_factor: 2.,
    });
    assert!((bone_length(&shown(&mut reopened_editor)?, &child) - 180.).abs() < 0.001);
    Ok(())
}

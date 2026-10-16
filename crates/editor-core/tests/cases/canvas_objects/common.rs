use super::super::support::{self, present_canvas as shown};
use editor_core::*;
use kasane_core::Vec2;
use std::sync::Arc;

pub(super) fn setup() -> anyhow::Result<(Editor, tempfile::TempDir)> {
    let (doc, dir) = support::sample_document_with_two_layers()?;
    let mut editor = crate::support::editor_from_document(doc);
    // Fixed pixel/source-unit assertions use an explicit 100% view.
    editor.reset_view();
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    shown(&mut editor)?;
    Ok((editor, dir))
}
pub(super) fn click(editor: &mut Editor, p: ViewportPoint, shift: bool) -> anyhow::Result<()> {
    editor.canvas_input(CanvasInput::Down { point: p, shift })?;
    editor.canvas_input(CanvasInput::Up { point: p })?;
    Ok(())
}
pub(super) fn texture(id: &str, alpha: [u8; 2]) -> SceneTexture {
    SceneTexture {
        asset_id: id.into(),
        cache_key: id.into(),
        width: 2,
        height: 1,
        rgba: vec![255, 255, 255, alpha[0], 255, 255, 255, alpha[1]].into(),
    }
}
pub(super) fn pixel_scene(doc: &mut Document) -> anyhow::Result<SceneSnapshot> {
    let mut source = doc.snapshot()?;
    let f = Arc::make_mut(&mut source.viewport_frame);
    for d in &mut f.drawables {
        d.texture_asset_id = "solid".into();
    }
    source.textures = vec![texture("solid", [255, 255]), texture("cutout", [0, 255])].into();
    Ok(source)
}

pub(super) fn rich_document()
-> anyhow::Result<(Document, tempfile::TempDir, String, String, String)> {
    rich_document_with_canvas(kasane_core::Canvas::new(256., 256., Vec2::new(0., 0.), 1.))
}
pub(super) fn rich_document_with_canvas(
    canvas: kasane_core::Canvas,
) -> anyhow::Result<(Document, tempfile::TempDir, String, String, String)> {
    use kasane_core::{
        Mesh, Part, PreciseVec2, RotationPose, RotationTransform, Transform, TransformData,
        WarpTransform,
    };
    use kasane_sdk::{AuthoringSession, prepare_png_asset};
    let dir = tempfile::tempdir()?;
    let asset_path = dir.path().join("a.png");
    std::fs::write(&asset_path, include_bytes!("../../fixtures/accent.png"))?;
    let id = |n| uuid::Uuid::from_u128(n).to_string();
    let mut s = AuthoringSession::new(&id(1), canvas)?;
    s.edit("fixture", None, |e| {
        e.create_asset(prepare_png_asset(&id(2), "a", &asset_path)?)?;
        e.create_part(Part {
            id: id(3),
            name: "Part".into(),
            ..Default::default()
        })?;
        e.create_transform(Transform {
            id: id(4),
            name: "Warp".into(),
            data: TransformData::Warp(WarpTransform {
                bezier: None,
                rows: 1,
                columns: 1,
                quad: true,
                points: vec![
                    Vec2::new(32., 32.),
                    Vec2::new(224., 32.),
                    Vec2::new(32., 224.),
                    Vec2::new(224., 224.),
                ],
            }),
            ..Default::default()
        })?;
        e.create_transform(Transform {
            id: id(5),
            name: "Rotation".into(),
            data: TransformData::Rotation(RotationTransform {
                base_angle: 0.,
                pose: RotationPose {
                    origin: PreciseVec2::new(128., 128.),
                    ..Default::default()
                },
            }),
            ..Default::default()
        })?;
        e.create_mesh(Mesh {
            id: id(6),
            name: "Mesh".into(),
            texture_asset_id: id(2),
            part_id: id(3),
            deformer_id: id(4),
            vertex_ids: vec![10, 11, 12, 13],
            base_positions: vec![
                Vec2::new(0.1, 0.1),
                Vec2::new(0.9, 0.1),
                Vec2::new(0.9, 0.9),
                Vec2::new(0.1, 0.9),
            ],
            uvs: vec![
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(0., 1.),
            ],
            triangles: vec![[10, 11, 12], [10, 12, 13]],
            ..Default::default()
        })?;
        Ok(())
    })?;
    let receipt = s.save_project(&dir.path().join("fixture"), None)?;
    Ok((Document::open(&receipt.manifest)?, dir, id(4), id(5), id(3)))
}

pub(super) fn deformer_editor(
    rotation: bool,
) -> anyhow::Result<(Editor, tempfile::TempDir, ObjectRef)> {
    let (doc, dir, warp, rotate, _) = rich_document()?;
    let object = ObjectRef::new(ObjectKind::Deformer, if rotation { rotate } else { warp });
    let mut editor = crate::support::editor_from_document(doc);
    // Drag distances below are calibrated at 100% on a 2x display.
    editor.reset_view();
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    editor.execute(EditorCommand::SetTool(ToolId::Select))?;
    shown(&mut editor)?;
    Ok((editor, dir, object))
}
pub(super) fn drag(
    editor: &mut Editor,
    start: ViewportPoint,
    end: ViewportPoint,
    shift: bool,
) -> anyhow::Result<()> {
    editor.canvas_input(CanvasInput::Down {
        point: start,
        shift,
    })?;
    editor.canvas_input(CanvasInput::Up { point: end })?;
    Ok(())
}

pub(super) fn control_points(
    editor: &mut Editor,
    object: &ObjectRef,
) -> anyhow::Result<Vec<ModelPoint>> {
    Ok(shown(editor)?
        .geometry
        .controls
        .iter()
        .find(|c| &c.object == object)
        .unwrap()
        .points
        .clone())
}

pub(super) fn bound_deformer_editor(
    rotation: bool,
    reversed: bool,
) -> anyhow::Result<(Editor, tempfile::TempDir, ObjectRef, String)> {
    use kasane_core::{
        BindingAxis, Parameter, RotationKeyform, RotationPose, RotationTransform, SceneBinding,
        SceneTrack, Transform, TransformData, WarpKeyform, WarpTransform,
    };
    let id = |n| uuid::Uuid::from_u128(n).to_string();
    let dir = tempfile::tempdir()?;
    let mut canvas = kasane_core::Canvas::new(256., 256., Vec2::new(0., 0.), 1.);
    if reversed {
        canvas.flag = 0;
    } else {
        canvas.flag = 1;
    }
    let mut session = kasane_sdk::AuthoringSession::new(&id(21), canvas)?;
    let positions = vec![
        Vec2::new(0.1, 0.1),
        Vec2::new(0.9, 0.1),
        Vec2::new(0.1, 0.9),
        Vec2::new(0.9, 0.9),
    ];
    let pose = RotationPose {
        origin: kasane_core::PreciseVec2::new(0.5123456789, 0.4123456789),
        ..Default::default()
    };
    session.edit("fixture", None, |edit| {
        edit.create_transform(Transform {
            id: id(22),
            data: TransformData::Warp(WarpTransform {
                bezier: None,
                rows: 1,
                columns: 1,
                quad: true,
                points: vec![
                    Vec2::new(20., 30.),
                    Vec2::new(220., 30.),
                    Vec2::new(20., 230.),
                    Vec2::new(220., 230.),
                ],
            }),
            ..Default::default()
        })?;
        edit.create_transform(Transform {
            id: id(23),
            parent_id: Some(id(22).into()),
            data: if rotation {
                TransformData::Rotation(RotationTransform {
                    pose,
                    ..Default::default()
                })
            } else {
                TransformData::Warp(WarpTransform {
                    bezier: None,
                    rows: 1,
                    columns: 1,
                    quad: true,
                    points: positions.clone(),
                })
            },
            ..Default::default()
        })?;
        edit.create_parameter(Parameter {
            id: id(24),
            minimum: 0.,
            maximum: 1.,
            default_value: 0.,
            ..Default::default()
        })?;
        edit.create_scene_binding(SceneBinding {
            id: id(25),
            axes: vec![BindingAxis {
                parameter_id: id(24),
                keys: vec![0., 1.],
            }],
            track: if rotation {
                SceneTrack::Rotation {
                    target_id: id(23).into(),
                    keyforms: vec![0., 1.]
                        .into_iter()
                        .map(|v| RotationKeyform {
                            keys: vec![v],
                            rotation: pose,
                            ..Default::default()
                        })
                        .collect(),
                }
            } else {
                SceneTrack::Warp {
                    target_id: id(23).into(),
                    keyforms: vec![0., 1.]
                        .into_iter()
                        .map(|v| WarpKeyform {
                            bezier: None,
                            keys: vec![v],
                            positions: positions.clone(),
                            ..Default::default()
                        })
                        .collect(),
                }
            },
        })?;
        Ok(())
    })?;
    let saved = session.save_project(&dir.path().join("bound"), None)?;
    let mut editor = crate::support::editor_from_document(Document::open(&saved.manifest)?);
    let object = ObjectRef::new(ObjectKind::Deformer, id(23));
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 2.,
    });
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    editor.execute(EditorCommand::SetTool(ToolId::Select))?;
    shown(&mut editor)?;
    Ok((editor, dir, object, id(24)))
}

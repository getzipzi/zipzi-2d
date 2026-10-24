use tempfile::TempDir;
use uuid::Uuid;
use zipzi_core::{Canvas, Mesh, Vec2};
use zipzi_editor_core::{CanvasFrame, Document, Editor, ObjectKind, ObjectRef};
use zipzi_sdk::{AuthoringSession, prepare_png_asset};

mod rectangle;
#[allow(unused_imports)]
pub use rectangle::{alpha_document, rectangle_document};

/// Drive synchronous pointer scripts through the same explicit gesture API as the UI.
#[allow(dead_code)]
pub fn pointer_input(
    editor: &mut Editor,
    input: zipzi_editor_core::CanvasInput,
) -> anyhow::Result<bool> {
    if editor.editing_context() != Some(zipzi_editor_core::EditingContext::Modeling) {
        return Ok(false);
    }
    let binding = editor
        .primary_viewport_binding()
        .ok_or_else(|| anyhow::anyhow!("No available Modeling viewport"))?;
    pointer_input_for(editor, binding, input)
}

#[allow(dead_code)]
pub fn pointer_input_for(
    editor: &mut Editor,
    binding: zipzi_editor_core::ViewportBinding,
    input: zipzi_editor_core::CanvasInput,
) -> anyhow::Result<bool> {
    use zipzi_editor_core::CanvasInput;
    if let CanvasInput::Down { point, shift, alt } = input {
        return editor
            .begin_canvas_gesture(binding, point, shift, alt)
            .map(|update| update.model_changed);
    }
    if let Some(token) = editor.current_viewport_interaction().filter(|token| {
        token.binding == binding
            && token.kind == zipzi_editor_core::ViewportInteractionKind::Modeling
    }) {
        return editor
            .update_canvas_gesture(token, input)
            .map(|update| update.model_changed);
    }
    match input {
        CanvasInput::Move { point, .. } => editor.hover_canvas(binding, Some(point))?,
        CanvasInput::Leave => editor.hover_canvas(binding, None)?,
        _ => {}
    }
    Ok(false)
}

#[allow(dead_code)]
pub fn object(kind: ObjectKind, n: u128) -> ObjectRef {
    ObjectRef::new(kind, Uuid::from_u128(n).to_string())
}

#[allow(dead_code)]
pub fn present_canvas(editor: &mut Editor) -> anyhow::Result<CanvasFrame> {
    attach_viewports(editor);
    let frame = editor.preview()?.canvas.unwrap();
    assert!(editor.present_frame(frame.clone()));
    Ok(frame)
}

#[allow(dead_code)]
pub fn attach_viewports(editor: &mut Editor) {
    if editor.primary_viewport_binding().is_some() {
        return;
    }
    for kind in [
        zipzi_editor_core::ViewportKind::Modeling,
        zipzi_editor_core::ViewportKind::Animation,
    ] {
        editor
            .register_primary_viewport(zipzi_editor_core::ViewportId::new(), kind)
            .unwrap();
    }
}

pub fn sample_document() -> anyhow::Result<(Document, TempDir)> {
    sample_document_with_options(false, false)
}

#[allow(dead_code)]
pub fn sample_document_with_binding(bound: bool) -> anyhow::Result<(Document, TempDir)> {
    sample_document_with_options(bound, false)
}

#[allow(dead_code)]
pub fn sample_document_with_two_layers() -> anyhow::Result<(Document, TempDir)> {
    sample_document_with_options(false, true)
}

pub fn sample_document_with_parts() -> anyhow::Result<(Document, TempDir)> {
    let (mut document, assets) = sample_document_with_two_layers()?;
    let meshes: Vec<_> = document.part_tree()[0]
        .children
        .iter()
        .map(|node| node.id.clone())
        .collect();
    let saved = document.save(&assets.path().join("parts.zipzi"))?;
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::default(), 1.),
    )?;
    session.open_project(&saved.manifest, None)?;
    let id = |value| Uuid::from_u128(value).to_string();
    session.edit("Parts fixture", None, |edit| {
        for (value, name, parent) in [
            (101, "Face", None),
            (102, "Body", None),
            (103, "Eyes", Some(101)),
        ] {
            edit.create_part(zipzi_core::Part {
                id: id(value),
                name: name.into(),
                parent_id: parent.map(id).unwrap_or_default(),
                ..Default::default()
            })?;
        }
        edit.set_mesh_part(&meshes[0], &id(101))?;
        edit.set_mesh_part(&meshes[1], &id(103))
    })?;
    let saved = session.save_project(&saved.manifest, None)?;
    Ok((Document::open(&saved.manifest)?, assets))
}

#[allow(dead_code)]
pub fn sample_document_with_scene_objects() -> anyhow::Result<(Document, TempDir)> {
    use zipzi_core::{Offscreen, Transform, TransformData, WarpTransform};
    let (mut document, assets) = sample_document_with_parts()?;
    let saved = document.save(&assets.path().join("scene.zipzi"))?;
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::default(), 1.),
    )?;
    session.open_project(&saved.manifest, None)?;
    let meshes = session.mesh_ids().to_vec();
    let id = |n| Uuid::from_u128(n).to_string();
    session.edit("Scene objects fixture", None, |edit| {
        edit.create_transform(Transform {
            id: id(501),
            name: "Rotation".into(),
            part_id: Some(id(101).into()),
            ..Default::default()
        })?;
        edit.create_transform(Transform {
            id: id(502),
            name: "Warp".into(),
            part_id: Some(id(103).into()),
            parent_id: Some(id(501).into()),
            data: TransformData::Warp(WarpTransform {
                bezier: None,
                rows: 1,
                columns: 1,
                quad: false,
                points: vec![
                    Vec2::new(0., 0.),
                    Vec2::new(256., 0.),
                    Vec2::new(0., 256.),
                    Vec2::new(256., 256.),
                ],
            }),
            ..Default::default()
        })?;
        edit.set_deform_parent(&meshes[0], &id(501))?;
        edit.set_deform_parent(&meshes[1], &id(502))?;
        edit.create_offscreen(Offscreen {
            id: id(601),
            name: "Face composite".into(),
            part_id: id(101),
            ..Default::default()
        })
    })?;
    session.save_project(&saved.manifest, None)?;
    Ok((Document::open(&saved.manifest)?, assets))
}

#[allow(dead_code)]
pub fn sample_document_with_parameters(count: usize) -> anyhow::Result<(Document, TempDir)> {
    use zipzi_core::Parameter;

    let directory = tempfile::tempdir()?;
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::new(0., 0.), 1.),
    )?;
    session.edit("Create parameters", None, |edit| {
        for index in 0..count {
            edit.create_parameter(Parameter {
                id: Uuid::from_u128(index as u128 + 1).to_string(),
                name: format!("Parameter {index}"),
                minimum: 0.,
                maximum: 1.,
                default_value: 0.,
                ..Default::default()
            })?;
        }
        Ok(())
    })?;
    let receipt = session.save_project(&directory.path().join("fixture.zipzi"), None)?;
    Ok((Document::open(&receipt.manifest)?, directory))
}

#[allow(dead_code)]
pub fn sample_document_with_parameter_groups() -> anyhow::Result<(Document, TempDir)> {
    use zipzi_core::{Parameter, document::CdiParameterGroup};

    let directory = tempfile::tempdir()?;
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::default(), 1.),
    )?;
    session.edit("Grouped parameters", None, |edit| {
        for (index, name, parent) in [
            (101, "Face", None),
            (102, "Eyes", Some(101)),
            (103, "Body", None),
            (104, "Expressions", None),
        ] {
            edit.create_parameter_group(CdiParameterGroup {
                id: Uuid::from_u128(index).to_string(),
                runtime_id: name.into(),
                name: name.into(),
                parent_id: parent.map(|id| Uuid::from_u128(id).to_string()),
                extensions: Default::default(),
            })?;
        }
        // Deliberately interleave memberships to exercise presentation ordering.
        for (index, name, group) in [
            (1, "Angle X", Some(101)),
            (2, "Body Angle", Some(103)),
            (3, "Eye Open L", Some(102)),
            (4, "Eye Open R", Some(102)),
            (5, "Breath", None),
        ] {
            let id = Uuid::from_u128(index).to_string();
            edit.create_parameter(Parameter {
                id: id.clone(),
                runtime_id: format!("Param{index}"),
                name: name.into(),
                minimum: 0.,
                maximum: 1.,
                default_value: 0.,
                ..Default::default()
            })?;
            let group = group.map(|id| Uuid::from_u128(id).to_string());
            edit.set_parameter_group(&id, group.as_deref())?;
        }
        Ok(())
    })?;
    let saved = session.save_project(&directory.path().join("grouped.zipzi"), None)?;
    Ok((Document::open(&saved.manifest)?, directory))
}

#[allow(dead_code)]
pub fn sample_document_with_bound_and_unbound_layers() -> anyhow::Result<(Document, TempDir)> {
    sample_document_with_options(true, true)
}

fn sample_document_with_options(
    bound: bool,
    two_layers: bool,
) -> anyhow::Result<(Document, TempDir)> {
    let directory = tempfile::tempdir()?;
    let texture_path = directory.path().join("accent.png");
    std::fs::write(&texture_path, include_bytes!("../fixtures/accent.png"))?;
    let asset_id = Uuid::new_v4().to_string();
    let mesh_id = Uuid::new_v4().to_string();
    let asset = prepare_png_asset(&asset_id, "Accent", &texture_path)?;
    let mesh = Mesh {
        id: mesh_id,
        name: "Diamond".into(),
        texture_asset_id: (asset_id).into(),
        vertex_ids: vec![0, 1, 2, 3],
        base_positions: vec![
            Vec2::new(128., 24.),
            Vec2::new(232., 128.),
            Vec2::new(128., 232.),
            Vec2::new(24., 128.),
        ],
        uvs: vec![
            Vec2::new(0.5, 0.),
            Vec2::new(1., 0.5),
            Vec2::new(0.5, 1.),
            Vec2::new(0., 0.5),
        ],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        ..Default::default()
    };
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::new(0., 0.), 1.),
    )?;
    session.edit("Create test layer", None, |edit| {
        edit.create_asset(asset)?;
        edit.create_mesh(mesh.clone())?;
        if two_layers {
            let mut second = mesh.clone();
            second.id = Uuid::new_v4().to_string();
            second.name = "Second".into();
            edit.create_mesh(second)?;
        }
        if bound {
            use zipzi_core::{BindingAxis, MeshBinding, MeshKeyform, Parameter};
            let parameter_id = Uuid::new_v4().to_string();
            edit.create_parameter(Parameter {
                id: parameter_id.clone(),
                name: "Pose".into(),
                minimum: 0.,
                maximum: 1.,
                default_value: 0.,
                ..Default::default()
            })?;
            edit.create_binding(MeshBinding {
                id: Uuid::new_v4().to_string(),
                mesh_id: mesh.id.clone(),
                axes: vec![BindingAxis {
                    id: Default::default(),
                    samples: Default::default(),
                    parameter_id,
                    keys: vec![0., 1.],
                }],
                keyforms: vec![0., 1.]
                    .into_iter()
                    .map(|key| MeshKeyform {
                        keys: vec![key],
                        positions: mesh.base_positions.clone(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            })?;
        }
        Ok(())
    })?;
    let receipt = session.save_project(&directory.path().join("fixture.zipzi"), None)?;
    let mut document = Document::open(&receipt.manifest)?;
    document.select(&mesh.id)?;
    Ok((document, directory))
}

#[allow(dead_code)]
pub fn sample_document_with_nonuniform_parameter_keys() -> anyhow::Result<(Document, TempDir)> {
    use zipzi_core::{BindingAxis, Parameter, Part, PartKeyform, SceneBinding, SceneTrack};
    let assets = tempfile::tempdir()?;
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::new(0., 0.), 1.),
    )?;
    let ids = [Uuid::new_v4().to_string(), Uuid::new_v4().to_string()];
    let part = Uuid::new_v4().to_string();
    let keys = vec![0.13, 0.375, 0.91];
    session.edit("Nonuniform parameter keys", None, |edit| {
        for id in &ids {
            edit.create_parameter(Parameter {
                id: id.clone(),
                minimum: 0.13,
                maximum: 0.91,
                default_value: 0.13,
                decimal_places: 2,
                ..Default::default()
            })?;
        }
        edit.create_part(Part {
            id: part.clone(),
            ..Default::default()
        })?;
        edit.create_scene_binding(SceneBinding {
            id: Uuid::new_v4().to_string(),
            axes: ids
                .iter()
                .map(|id| BindingAxis {
                    id: Default::default(),
                    samples: Default::default(),
                    parameter_id: id.clone(),
                    keys: keys.clone(),
                })
                .collect(),
            track: SceneTrack::Part {
                target_id: part.into(),
                keyforms: keys
                    .iter()
                    .flat_map(|&y| {
                        keys.iter().map(move |&x| PartKeyform {
                            samples: Default::default(),
                            keys: vec![x, y],
                            draw_order: x + y,
                        })
                    })
                    .collect(),
            },
        })?;
        Ok(())
    })?;
    let receipt = session.save_project(&assets.path().join("nonuniform.zipzi"), None)?;
    Ok((Document::open(&receipt.manifest)?, assets))
}

/// A parameter-bound rectangle with repeated registrations (distinct fades),
/// and an ungrouped looping clip.
#[allow(dead_code)]
pub fn animation_document() -> anyhow::Result<(Document, TempDir)> {
    use zipzi_core::document::{
        ExpressionAsset, ExpressionBlend, ExpressionEntry, ExpressionTarget, MotionClip,
        MotionGroup, MotionKey, MotionRegistration, MotionSegment, MotionTrack, MotionTrackTarget,
    };
    let (mut doc, assets) = rectangle_document(true)?;
    let path = doc.save(&assets.path().join("animation.zipzi"))?.manifest;
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::default(), 1.),
    )?;
    session.open_project(&path, None)?;
    let parameter_id = session.parameter_ids()[0].clone();
    let clip = Uuid::new_v4().to_string();
    let make_clip = |id: String, name: &str, looping: bool, duration: f32| MotionClip {
        track_order: vec![],
        id,
        name: name.into(),
        duration,
        fps: 60.,
        looping,
        restricted_beziers: true,
        fade_in: Some(0.),
        fade_out: Some(0.),
        tracks: vec![MotionTrack {
            id: Uuid::new_v4().to_string(),
            target: MotionTrackTarget::Parameter {
                parameter_id: parameter_id.clone(),
            },
            initial: MotionKey {
                id: Uuid::new_v4().to_string(),
                time: 0.,
                value: 0.,
            },
            segments: (duration > 0.)
                .then(|| MotionSegment::Linear {
                    end: MotionKey {
                        id: Uuid::new_v4().to_string(),
                        time: duration,
                        value: 1.,
                    },
                })
                .into_iter()
                .collect::<Vec<_>>()
                .into(),
            fade_in: None,
            fade_out: None,
            extensions: Default::default(),
        }]
        .into(),
        events: vec![].into(),
        extensions: Default::default(),
        meta_extensions: Default::default(),
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    };
    session.edit("Animation fixture", None, |edit| {
        for (name, blend, value) in [
            ("Add", ExpressionBlend::Add, 0.3),
            ("Multiply", ExpressionBlend::Multiply, 0.5),
            ("Overwrite", ExpressionBlend::Overwrite, 0.8),
        ] {
            edit.create_expression(ExpressionAsset {
                id: Uuid::new_v4().to_string(),
                name: name.into(),
                file_type: None,
                fade_in: Some(0.2),
                fade_out: Some(0.3),
                entries: vec![ExpressionEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    target: ExpressionTarget::Resolved {
                        parameter_id: parameter_id.clone(),
                    },
                    value,
                    blend: Some(blend),
                    extensions: Default::default(),
                }],
                extensions: Default::default(),
                opaque_source_ids: None,
                opaque_source_content_hash: None,
            })?;
        }
        edit.create_motion(make_clip(clip.clone(), "Once", false, 2.))?;
        edit.create_motion(make_clip(Uuid::new_v4().to_string(), "Loop", true, 1.))?;
        edit.set_motion_library(
            vec![MotionGroup {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Idle".into(),
                entries: [0., 1.]
                    .into_iter()
                    .map(|fade| MotionRegistration {
                        id: uuid::Uuid::new_v4().to_string(),
                        clip_id: clip.clone(),
                        fade_in: Some(fade),
                        fade_out: Some(0.),
                        sound: Some("test.wav".into()),
                        extensions: Default::default(),
                    })
                    .collect(),
            }],
            vec![],
        )
    })?;
    session.save_project(&path, None)?;
    Ok((Document::open(&path)?, assets))
}

/// Non-looping motion and expressions with an actual pendulum and Pose group.
#[allow(dead_code)]
pub fn combined_animation_document() -> anyhow::Result<(Document, TempDir)> {
    let (mut document, assets) = animation_document()?;
    let path = document
        .save(&assets.path().join("combined.zipzi"))?
        .manifest;
    let mut session = AuthoringSession::new(&Uuid::new_v4().to_string(), document.model_setup())?;
    session.open_project(&path, None)?;
    let mut input = session.parameter(&session.parameter_ids()[0]).unwrap();
    input.runtime_id = "ParamX".into();
    let mut part = session
        .document()
        .get_part(&session.document().part_order()[0])
        .unwrap()
        .clone();
    part.runtime_id = "Part0".into();
    let physics = serde_json::json!({
        "Version": 3,
        "Meta": {"PhysicsSettingCount":1,"TotalInputCount":1,"TotalOutputCount":1,"VertexCount":2,"Fps":30,
            "EffectiveForces":{"Gravity":{"X":0,"Y":-1},"Wind":{"X":0,"Y":0}},
            "PhysicsDictionary":[{"Id":"Probe","Name":"Probe"}]},
        "PhysicsSettings":[{"Id":"Probe",
            "Input":[{"Source":{"Target":"Parameter","Id":"ParamX"},"Weight":100,"Type":"X","Reflect":false}],
            "Output":[{"Destination":{"Target":"Parameter","Id":"ParamY"},"VertexIndex":1,"Scale":1,"Weight":100,"Type":"Angle","Reflect":false}],
            "Vertices":[{"Position":{"X":0,"Y":0},"Mobility":1,"Delay":1,"Acceleration":1,"Radius":0},
                {"Position":{"X":0,"Y":1},"Mobility":0.8,"Delay":0.9,"Acceleration":1.5,"Radius":1}],
            "Normalization":{"Position":{"Minimum":-1,"Default":0,"Maximum":1},"Angle":{"Minimum":-1,"Default":0,"Maximum":1}}
        }]
    });
    session.edit("Combined runtime fixture", None, |edit| {
        edit.replace_parameter(input)?;
        edit.create_parameter(zipzi_core::Parameter {
            id: Uuid::new_v4().to_string(),
            runtime_id: "ParamY".into(),
            name: "Physics output".into(),
            minimum: -1.,
            maximum: 1.,
            ..Default::default()
        })?;
        edit.replace_part(part)?;
        edit.create_part(zipzi_core::Part {
            id: Uuid::new_v4().to_string(),
            runtime_id: "Part1".into(),
            name: "Pose peer".into(),
            ..Default::default()
        })?;
        edit.import_physics3(&Uuid::new_v4().to_string(), &physics.to_string())?;
        edit.import_pose3(
            &Uuid::new_v4().to_string(),
            r#"{"Type":"Live2D Pose","FadeInTime":0.5,"Groups":[[{"Id":"Part0"},{"Id":"Part1"}]]}"#,
        )?;
        Ok(())
    })?;
    session.save_project(&path, None)?;
    Ok((Document::open(&path)?, assets))
}

/// Unsupported and unresolved resources are retained for diagnostics, never repaired by preview.
#[allow(dead_code)]
pub fn diagnostic_animation_document() -> anyhow::Result<(Document, TempDir)> {
    use zipzi_core::document::{
        ExpressionAsset, ExpressionEntry, ExpressionTarget, MotionTrackTarget,
    };
    let (mut document, assets) = animation_document()?;
    let path = document
        .save(&assets.path().join("diagnostics.zipzi"))?
        .manifest;
    let mut session = AuthoringSession::new(&Uuid::new_v4().to_string(), document.model_setup())?;
    session.open_project(&path, None)?;
    let mut clip = session
        .document()
        .get_motion(&session.document().motion_order()[0])
        .unwrap()
        .clone();
    clip.id = Uuid::new_v4().to_string();
    clip.name = "Unsupported".into();
    let mut unsupported = clip.tracks[0].clone();
    unsupported.id = Uuid::new_v4().to_string();
    unsupported.target = MotionTrackTarget::Model {
        runtime_id: "EyeBlink".into(),
    };
    clip.tracks[0].id = Uuid::new_v4().to_string();
    clip.tracks[0].target = MotionTrackTarget::Unresolved {
        category: "Parameter".into(),
        runtime_id: "MissingParam".into(),
    };
    clip.tracks.push(unsupported);
    for track in &mut clip.tracks {
        track.initial.id = Uuid::new_v4().to_string();
        for segment in std::sync::Arc::make_mut(&mut track.segments) {
            segment.end_mut().id = Uuid::new_v4().to_string();
        }
    }
    session.edit("Diagnostic resources", None, |edit| {
        edit.create_motion(clip)?;
        edit.create_expression(ExpressionAsset {
            id: Uuid::new_v4().to_string(),
            name: "Unavailable".into(),
            file_type: None,
            fade_in: None,
            fade_out: None,
            entries: vec![ExpressionEntry {
                id: uuid::Uuid::new_v4().to_string(),
                target: ExpressionTarget::Unresolved {
                    runtime_id: "MissingExpressionParam".into(),
                },
                value: 1.,
                blend: None,
                extensions: Default::default(),
            }],
            extensions: Default::default(),
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        })
    })?;
    session.save_project(&path, None)?;
    Ok((Document::open(&path)?, assets))
}

#[allow(dead_code)]
pub fn editor_from_document(document: Document) -> Editor {
    let mut editor = Editor::from_document(document);
    attach_viewports(&mut editor);
    editor
}
#[allow(dead_code)]
pub fn new_editor() -> anyhow::Result<Editor> {
    Ok(editor_from_document(Document::new_blank()?))
}
#[allow(dead_code)]
pub fn empty_editor() -> Editor {
    let mut editor = Editor::empty();
    attach_viewports(&mut editor);
    editor
}

/// Two nontrivial affine parent chains, optionally with a varying parent driver.
#[allow(dead_code)]
pub fn sample_document_for_reparent(driven: bool) -> anyhow::Result<(Document, TempDir)> {
    use zipzi_core::{PreciseVec2, RotationPose, RotationTransform, Transform, TransformData};
    let (mut d, assets) = sample_document_with_two_layers()?;
    let saved = d.save(&assets.path().join("reparent.zipzi"))?;
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::default(), 1.),
    )?;
    session.open_project(&saved.manifest, None)?;
    let meshes = session.mesh_ids().to_vec();
    let id = |n| Uuid::from_u128(n).to_string();
    session.edit("Affine parents", None, |e| {
        for (n, name, angle, scale) in [
            (501, "Source rotation", 25., 0.8),
            (502, "Destination rotation", -18., 1.2),
        ] {
            e.create_transform(Transform {
                id: id(n),
                name: name.into(),
                data: TransformData::Rotation(RotationTransform {
                    base_angle: 0.,
                    pose: RotationPose {
                        angle,
                        scale,
                        origin: PreciseVec2::new(8., -3.),
                        ..Default::default()
                    },
                }),
                ..Default::default()
            })?;
        }
        e.set_deform_parent(&meshes[0], &id(501))?;
        e.set_deform_parent(&meshes[1], &id(502))?;
        e.create_parameter(zipzi_core::Parameter {
            id: id(701),
            name: "Driver".into(),
            minimum: 0.,
            maximum: 1.,
            default_value: 0.5,
            ..Default::default()
        })?;
        if driven {
            e.create_scene_binding(zipzi_core::SceneBinding {
                id: id(702),
                axes: vec![zipzi_core::BindingAxis::new(id(701), vec![0., 1.])],
                track: zipzi_core::SceneTrack::Rotation {
                    target_id: id(502).into(),
                    keyforms: [(-15., 0.), (35., 1.)]
                        .into_iter()
                        .map(|(angle, key)| zipzi_core::RotationKeyform {
                            keys: vec![key],
                            rotation: RotationPose {
                                angle,
                                scale: 1.,
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .collect(),
                },
            })?;
        }
        Ok(())
    })?;
    session.save_project(&saved.manifest, None)?;
    let mut d = Document::open(&saved.manifest)?;
    d.select(&meshes[0])?;
    Ok((d, assets))
}

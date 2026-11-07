mod support;

use std::collections::HashMap;
use support::{image, prepare};
use zipzi_core::document::{SceneContentEdit, ShapeTarget, VertexContentPatch};
use zipzi_core::state::{DocumentState, RecordDomain};

use zipzi_core::{
    evaluate_frame, BindingAxis, Canvas, DrawableFrame, Mesh, MeshBinding, MeshKeyform, Parameter,
    Vec2, VertexId,
};

fn id(n: i32) -> String {
    format!("{}1111111-1111-4111-8111-111111111111", n)
}

#[test]
fn test_keyform_data_editing_and_evaluation() {
    let base = DocumentState::new(
        id(1),
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 100.0).into(),
    )
    .unwrap();
    let mut builder = base.builder();
    builder.create_asset(image(id(2), "asset", 8, 8)).unwrap();

    let m = Mesh {
        id: id(3),
        name: "mesh".to_string(),
        texture_asset_id: (id(2)).into(),
        vertex_ids: vec![9, 1, 7],
        base_positions: vec![
            Vec2::new(10.0, 10.0),
            Vec2::new(40.0, 10.0),
            Vec2::new(20.0, 40.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(0.5, 1.0),
        ],
        triangles: vec![[9, 1, 7]],
        runtime_id: "Mesh".to_string(),
        ..Default::default()
    };
    builder.create_mesh(m.clone()).unwrap();

    builder
        .create_parameter(Parameter {
            id: id(4),
            name: "Param".to_string(),
            runtime_id: "parameter".to_string(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 6,
            ..Default::default()
        })
        .unwrap();

    let mut b = MeshBinding {
        id: id(5),
        mesh_id: id(3),
        axes: vec![BindingAxis {
            id: Default::default(),
            samples: Default::default(),
            parameter_id: id(4),
            keys: vec![-1.0, 0.0, 1.0],
        }],
        keyforms: Vec::new(),
    };
    for key in [1.0f32, 0.0, -1.0] {
        let mut positions = m.base_positions.clone();
        for p in &mut positions {
            p.x += key * 20.0;
        }
        b.keyforms.push(MeshKeyform {
            keys: vec![key],
            positions,
            ..Default::default()
        });
    }
    builder.create_binding(b).unwrap();
    let prepared = builder.freeze().unwrap();
    assert!(prepared
        .writes
        .records()
        .iter()
        .any(|w| w.domain == RecordDomain::Parameter && w.id == id(4)));
    let d = prepared.state;
    assert_eq!(d.binding(&id(5)).unwrap().keyforms[0].keys[0], -1.0);

    let content = d.content_id();
    let mut frame = DrawableFrame::default();
    let mut preview = HashMap::new();
    preview.insert(id(4), 0.5f32);
    assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
    assert!((frame.drawables[0].positions[0].x - 20.).abs() < 1e-6);
    assert_eq!(d.content_id(), content);

    let preserved = frame.drawables[0].positions.clone();
    let mut bad_preview = HashMap::new();
    bad_preview.insert(id(4), f32::NAN);
    assert!(!evaluate_frame(&d, &bad_preview, &mut frame).is_ok());
    assert_eq!(frame.drawables[0].positions, preserved);

    let mut unknown_preview = HashMap::new();
    unknown_preview.insert(id(6), 0.0f32);
    assert!(!evaluate_frame(&d, &unknown_preview, &mut frame).is_ok());

    let mut clamped_preview = HashMap::new();
    clamped_preview.insert(id(4), 10.0f32);
    assert!(evaluate_frame(&d, &clamped_preview, &mut frame).is_ok());
    assert!(frame.parameters[0].clamped && frame.parameters[0].value == 1.0);

    let vertex: VertexId = 9;
    let changed_base = Vec2::new(999.0, 10.0);
    let d = prepare(&d, |builder| {
        builder.edit_scene_content(
            SceneContentEdit::MeshVertices {
                mesh_id: id(3),
                target: ShapeTarget::Base,
                updates: vec![VertexContentPatch {
                    vertex_id: vertex,
                    position: Some(changed_base),
                    uv: None,
                }],
            },
            Default::default(),
            &Default::default(),
        )?;
        Ok(())
    })
    .unwrap()
    .state;
    assert!(evaluate_frame(&d, &preview, &mut frame).is_ok());
    assert_eq!(frame.drawables[0].positions, preserved);

    let binding = d.binding(&id(5)).unwrap();
    let mut invalid = binding.clone();
    invalid.axes[0].keys = vec![-1.0, 0.0, 0.0];
    assert!(prepare(&d, |b| b.replace_binding(invalid)).is_err());
    let mut invalid = binding.clone();
    invalid.keyforms[1].positions.pop();
    assert_eq!(
        prepare(&d, |b| b.replace_binding(invalid))
            .unwrap_err()
            .code,
        "INVALID_LENGTH"
    );

    assert!(prepare(&d, |b| b.remove_asset(&id(2))).is_err());
    assert!(prepare(&d, |b| b.remove_mesh(&id(3))).is_err());
    assert!(prepare(&d, |b| b.remove_parameter(&id(4))).is_err());
    assert_eq!(d.binding(&id(5)), Some(binding));

    let unbound = prepare(&d, |b| b.remove_binding(&id(5))).unwrap().state;
    assert!(evaluate_frame(&unbound, &HashMap::new(), &mut frame).is_ok());
    assert!((frame.drawables[0].positions[0].x - 999.).abs() < 1e-5);

    let empty = prepare(&unbound, |b| {
        b.remove_parameter(&id(4))?;
        b.remove_mesh(&id(3))?;
        b.remove_asset(&id(2))
    })
    .unwrap()
    .state;
    assert!(evaluate_frame(&empty, &HashMap::new(), &mut frame).is_ok());
    assert!(frame.drawables.is_empty());
}

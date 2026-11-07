use super::*;
use crate::ManualMeshOperation;
fn publish(s: &mut AuthoringSession, run: impl FnOnce(&mut Commands<'_>) -> Result<(), Status>) {
    let mut d = s.begin_draft();
    d.batch(d.source(), run).unwrap();
    s.publish(d.prepare("mesh").unwrap()).unwrap();
}
#[test]
fn manual_mesh_transfer_samples_ordinary_and_blend_channels_with_one_history_step() {
    let mut s = AuthoringSession::new(snapshot());
    publish(&mut s, |c| {
        c.create_mesh_binding_spec(MeshBindingCreateSpec {
            id: Some(id(20)),
            mesh_id: id(4),
            axes: vec![MeshBindingAxisSpec {
                parameter_id: id(2),
                keys: vec![0., 1.],
            }],
            keyforms: [0., 1.]
                .into_iter()
                .map(|v| MeshKeyformSpec {
                    keys: vec![v],
                    positions: mesh()
                        .base_positions
                        .iter()
                        .map(|p| Vec2::new(p.x + v * 8., p.y))
                        .collect(),
                    opacity: None,
                    multiply: None,
                    screen: None,
                    draw_order: None,
                })
                .collect(),
        })?;
        c.create_parameter(Parameter {
            id: id(30),
            kind: zipzi_core::ParameterKind::BlendShape,
            ..Default::default()
        })?;
        c.edit_blend_shape(
            BlendAuthoringEdit::CreateTable {
                table: zipzi_core::BlendShapeKeyTable {
                    id: id(31),
                    parameter_id: id(30),
                    keys: vec![0., 1.],
                    base_key_idx: 0,
                    samples: vec![],
                    base_sample_id: String::new(),
                },
            },
            Default::default(),
            &Default::default(),
        )?;
        c.edit_blend_shape(
            BlendAuthoringEdit::CreateBinding {
                id: id(32),
                target_id: id(4),
                target_kind: zipzi_core::BlendShapeTargetKind::Mesh,
                key_table_id: id(31),
            },
            Default::default(),
            &Default::default(),
        )?;
        Ok(())
    });
    let mut blend = s
        .snapshot()
        .document()
        .blend_binding(&id(32))
        .unwrap()
        .clone();
    let zipzi_core::DeltaKeyforms::Mesh(forms) = &mut blend.keyforms else {
        panic!()
    };
    forms[1].positions = vec![Vec2::new(0., 0.), Vec2::new(4., 0.), Vec2::new(0., 4.)];
    publish(&mut s, |c| c.replace_blend_binding(blend));
    let before = s.snapshot().clone();
    let steps = s.history_state().undo_steps;
    publish(&mut s, |c| {
        let result = c.edit_manual_mesh(
            &id(4),
            ManualMeshOperation::InsertVertex {
                id: 10,
                uv: Vec2::new(0.25, 0.25),
            },
            Default::default(),
            &Default::default(),
        )?;
        assert_eq!(result.report.added, 1);
        assert_eq!(result.report.shape_count, 5);
        assert!(result
            .provenance
            .iter()
            .any(|p| matches!(p, crate::VertexProvenance::Sampled { .. })));
        Ok(())
    });
    let state = s.snapshot().document();
    let mesh = state.mesh(&id(4)).unwrap();
    let slot = mesh.vertex_ids.iter().position(|&v| v == 10).unwrap();
    assert_eq!(mesh.base_positions[slot], Vec2::new(0.25, 0.25));
    assert_eq!(
        state.binding(&id(20)).unwrap().keyforms[1].positions[slot],
        Vec2::new(8.25, 0.25)
    );
    let zipzi_core::DeltaKeyforms::Mesh(forms) = &state.blend_binding(&id(32)).unwrap().keyforms
    else {
        panic!()
    };
    assert_eq!(forms[1].positions[slot], Vec2::new(1., 1.));
    assert_eq!(s.history_state().undo_steps, steps + 1);
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    s.redo(s.version()).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .mesh(&id(4))
            .unwrap()
            .vertex_ids
            .len(),
        4
    );
}
#[test]
fn manual_mesh_budget_and_invalid_uv_abort_batch_without_touching_source() {
    for limited in [false, true] {
        let s = AuthoringSession::new(snapshot());
        let before = s.snapshot().clone();
        let mut d = s.begin_draft();
        let result = d.batch(d.source(), |c| {
            c.rename(NameTarget::Mesh(&id(4)), "pending".into())?;
            c.edit_manual_mesh(
                &id(4),
                ManualMeshOperation::InsertVertex {
                    id: 10,
                    uv: Vec2::new(f32::NAN, 0.25),
                },
                ConversionLimits {
                    max_workspace_bytes: if limited { 1 } else { 64 * 1024 * 1024 },
                    ..Default::default()
                },
                &Default::default(),
            )?;
            Ok(())
        });
        assert!(result.is_err());
        assert!(d.prepare("failure").is_err());
        assert_eq!(s.snapshot().content_id(), before.content_id());
    }
}

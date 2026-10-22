use kasane_core::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct CountingAllocator;
thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCATIONS.try_with(|count| {
            if let Some(n) = count.get() {
                count.set(Some(n + 1));
            }
        });
        System.alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocations(f: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|count| count.set(Some(0)));
    f();
    ALLOCATIONS.with(|count| count.replace(None).unwrap())
}

fn id(n: usize) -> String {
    format!("{n:08x}-1111-4111-8111-111111111111")
}

fn document(count: usize) -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(id(1), Canvas::new(640., 480., Vec2::new(320., 240.), 100.))
        .is_ok());
    assert!(doc
        .add_asset(ImageAsset {
            unresolved_source: None,
            id: id(2),
            name: String::new(),
            content: Some(kasane_core::ImageContent {
                source: "tex.png".into(),
                width: 64,
                height: 64,
                ..Default::default()
            }),
        })
        .status
        .is_ok());
    for n in 0..count {
        assert!(doc
            .create_mesh(Mesh {
                id: id(n + 3),
                texture_asset_id: (id(2)).into(),
                vertex_ids: vec![1, 2, 3],
                base_positions: vec![Vec2::new(0., 0.), Vec2::new(10., 0.), Vec2::new(0., 10.)],
                uvs: vec![Vec2::new(0., 0.); 3],
                triangles: vec![[1, 2, 3]],
                ..Default::default()
            })
            .status
            .is_ok());
    }
    doc
}

#[test]
fn saved_content_queries_do_not_allocate_and_still_detect_reverted_edits() {
    let mut doc = document(100);
    doc.mark_saved();
    let original = doc.clone();
    let count = allocations(|| {
        assert!(!doc.modified());
        assert!(doc.same_content(&original));
    });
    assert_eq!(count, 0);
    assert!(doc.rename_mesh(&id(3), "changed".into()).status.is_ok());
    assert!(doc.modified());
    assert!(!doc.same_content(&original));
    doc.restore_from(&original);
    assert!(!doc.modified());
}

#[test]
fn workspace_reuses_geometry_and_preserves_output_on_late_failure() {
    let mut doc = document(100);
    let preview = PreviewValues::new();
    let mut output = DrawableFrame::default();
    let mut evaluator = FrameEvaluator::default();
    for _ in 0..3 {
        assert!(evaluator.evaluate(&doc, &preview, &mut output).is_ok());
    }
    let positions = output.drawables[0].positions.as_ptr();
    let reused = allocations(|| {
        assert!(evaluator.evaluate(&doc, &preview, &mut output).is_ok());
    });
    assert!(evaluator.evaluate(&doc, &preview, &mut output).is_ok());
    assert_eq!(positions, output.drawables[0].positions.as_ptr());
    let mut fresh = DrawableFrame::default();
    let one_shot = allocations(|| {
        assert!(evaluate_frame(&doc, &preview, &mut fresh).is_ok());
    });
    assert_eq!(output, fresh);
    // Callers own the output and may edit it; recycled slots must reset all fields.
    output.drawables[0].multiply_color[3] = 0.0;
    output.drawables[0].screen_color[3] = 0.0;
    for _ in 0..2 {
        assert!(evaluator.evaluate(&doc, &preview, &mut output).is_ok());
    }
    assert_eq!(output, fresh);
    assert!(
        reused * 2 < one_shot,
        "reused={reused}, one_shot={one_shot}"
    );
    assert_eq!(
        reused, 0,
        "Warmed static meshes must not allocate per frame"
    );
    println!("100 meshes: reused={reused}, one_shot={one_shot} allocations");

    let valid = doc.clone();
    let mut overflow = doc.get_mesh(&id(3)).unwrap().clone();
    overflow.base_positions[0].x = f32::MAX;
    assert!(doc.replace_mesh(overflow).status.is_ok());
    let rotation_id = id(9999);
    assert!(doc
        .create_transform(Transform {
            id: rotation_id.clone(),
            data: TransformData::Rotation(RotationTransform {
                pose: RotationPose {
                    scale: 2.,
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        })
        .status
        .is_ok());
    let mut overflow_mesh = doc.get_mesh(&id(3)).unwrap().clone();
    overflow_mesh.deformer_id = rotation_id;
    assert!(doc.replace_mesh(overflow_mesh).status.is_ok());
    assert_eq!(
        evaluator.evaluate(&doc, &preview, &mut output).code,
        "NON_FINITE"
    );
    assert_eq!(output, fresh);
    doc.restore_from(&valid);
    assert!(evaluator.evaluate(&doc, &preview, &mut output).is_ok());
    assert!(evaluate_frame(&doc, &preview, &mut fresh).is_ok());
    assert_eq!(output, fresh);
}

// A renderer that retains the previous published frame through the next
// evaluation prevents PreviewState::invalidate from reclaiming its buffers.
// Measure that ownership cost before adopting persistent Arc frame caching.
#[test]
fn published_frame_ownership_preserves_snapshots_and_recycles_released_buffers() {
    use kasane_core::preview::PreviewState;

    let mut doc = document(100);
    let parameter = id(200);
    assert!(doc
        .create_parameter(Parameter {
            id: parameter.clone(),
            minimum: -1.0,
            maximum: 1.0,
            ..Default::default()
        })
        .status
        .is_ok());

    let measure = |retain: bool| {
        let mut state = PreviewState::default();
        let mut retained: Option<std::sync::Arc<DrawableFrame>> = None;
        let mut step = |i| {
            let value = if i % 2 == 0 { -0.5 } else { 0.5 };
            assert!(state
                .replace(&doc, 1, [(parameter.clone(), value)].into())
                .unwrap());
            let frame = state.frame(&doc, 1).unwrap();
            assert_eq!(frame.drawables.len(), 100);
            assert_eq!(frame.parameters[0].value, value);
            if let Some(old) = &retained {
                assert_eq!(old.parameters[0].value, -value);
            }
            retained = retain.then_some(frame);
        };
        for i in 0..8 {
            step(i);
        }
        allocations(|| {
            for i in 8..20 {
                step(i);
            }
        })
    };
    let released = measure(false);
    let retained = measure(true);
    println!("12 publications / 100 meshes: released={released}, retained={retained} allocations");
    // Gate cheap recycling, but do not make the current retained-frame penalty
    // a required behavior: a future pool may legitimately improve that path.
    assert!(released < 120, "released={released}, retained={retained}");
}

#[test]
fn binding_index_tracks_retarget_delete_and_restore() {
    let mut doc = document(2);
    let parameter = id(10);
    assert!(doc
        .create_parameter(Parameter {
            id: parameter.clone(),
            minimum: -1.,
            maximum: 1.,
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc.binding_for_mesh(&id(3)).is_none());
    let binding = MeshBinding {
        id: id(11),
        mesh_id: id(3),
        axes: vec![BindingAxis {
            id: Default::default(),
            samples: Default::default(),
            parameter_id: parameter,
            keys: vec![0.],
        }],
        keyforms: vec![MeshKeyform {
            keys: vec![0.],
            positions: doc.get_mesh(&id(3)).unwrap().base_positions.clone(),
            ..Default::default()
        }],
    };
    assert!(doc.create_binding(binding.clone()).status.is_ok());
    assert_eq!(doc.binding_for_mesh(&id(3)).unwrap().id, binding.id);
    let saved = doc.clone();
    let mut moved = binding.clone();
    moved.mesh_id = id(4);
    assert!(doc.replace_binding(moved).status.is_ok());
    assert!(doc.binding_for_mesh(&id(3)).is_none());
    assert_eq!(doc.binding_for_mesh(&id(4)).unwrap().id, binding.id);
    assert!(doc.erase_object(&binding.id).status.is_ok());
    assert!(doc.binding_for_mesh(&id(4)).is_none());
    doc.restore_from(&saved);
    assert_eq!(doc.binding_for_mesh(&id(3)).unwrap().id, binding.id);
    assert!(doc.binding_for_mesh(&id(4)).is_none());
}

#[test]
fn static_geometry_survives_positions_and_names_but_not_topology_changes() {
    use std::sync::Arc;
    let mut doc = document(1);
    let mut evaluator = FrameEvaluator::default();
    let mut frame = DrawableFrame::default();
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    let indices = frame.drawables[0].indices.clone();
    let uvs = frame.drawables[0].uvs.clone();
    assert!(doc.rename_mesh(&id(3), "renamed".into()).status.is_ok());
    assert!(doc
        .set_vertex_positions(&id(3), &[1], &[Vec2::new(2., 3.)])
        .status
        .is_ok());
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert!(Arc::ptr_eq(&indices, &frame.drawables[0].indices));
    assert!(Arc::ptr_eq(&uvs, &frame.drawables[0].uvs));
    let mut mesh = doc.get_mesh(&id(3)).unwrap().clone();
    mesh.uvs[0] = Vec2::new(0.5, 0.75);
    assert!(doc.replace_mesh(mesh).status.is_ok());
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert!(!Arc::ptr_eq(&uvs, &frame.drawables[0].uvs));
    assert_eq!(uvs[0], Vec2::new(0., 0.));
    // Reusing an evaluator on another document with the same revision is safe.
    let other = document(2);
    assert!(evaluator
        .evaluate(&other, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert_eq!(frame.drawables.len(), 2);
}

#[test]
fn metadata_reuses_preview_but_resources_and_restore_invalidate() {
    use kasane_core::preview::PreviewState;
    use std::sync::Arc;
    let mut doc = document(1);
    let saved = doc.clone();
    let mut preview = PreviewState::default();
    let first = preview.frame(&doc, 1).unwrap();
    assert!(doc.rename_mesh(&id(3), "renamed".into()).status.is_ok());
    assert!(Arc::ptr_eq(&first, &preview.frame(&doc, 1).unwrap()));
    assert_eq!(preview.evaluation_count(), 1);
    let mut asset = doc.get_asset(&id(2)).unwrap().clone();
    asset.content.as_mut().unwrap().source = "replacement.png".into();
    assert_eq!(doc.replace_asset(asset).changes.kind, ChangeKind::Resources);
    let second = preview.frame(&doc, 1).unwrap();
    assert!(!Arc::ptr_eq(&first, &second));
    assert!(Arc::ptr_eq(
        &first.drawables[0].uvs,
        &second.drawables[0].uvs
    ));
    doc.restore_from(&saved);
    assert!(!Arc::ptr_eq(&second, &preview.frame(&doc, 1).unwrap()));
}

#[test]
fn keyform_order_edit_invalidates_prepared_group_bounds() {
    let mut doc = document(2);
    assert!(doc
        .create_parameter(Parameter {
            id: id(10),
            ..Default::default()
        })
        .status
        .is_ok());
    let mut form = MeshKeyform {
        keys: vec![0.],
        positions: doc.get_mesh(&id(3)).unwrap().base_positions.clone(),
        draw_order: Some(0.),
        ..Default::default()
    };
    assert!(doc
        .create_binding(MeshBinding {
            id: id(11),
            mesh_id: id(3),
            axes: vec![BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: id(10),
                keys: vec![0.]
            }],
            keyforms: vec![form.clone()],
        })
        .status
        .is_ok());
    let mut evaluator = FrameEvaluator::default();
    let mut frame = DrawableFrame::default();
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert!(frame.drawables[0].render_order < frame.drawables[1].render_order);
    form.draw_order = Some(10.);
    assert_eq!(
        doc.set_mesh_keyform(&id(11), form).changes.kind,
        ChangeKind::Structure
    );
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert!(frame.drawables[0].render_order > frame.drawables[1].render_order);
}

#[test]
fn opacity_edits_preserve_prepared_topology_and_reject_invalid_values() {
    let mut doc = document(2);
    let mut before = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &PreviewValues::default(), &mut before).is_ok());
    let edit = doc.set_mesh_opacity(&id(3), 0.5);
    assert!(edit.status.is_ok());
    assert_eq!(edit.changes.kind, ChangeKind::Appearance);
    let mut after = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &PreviewValues::default(), &mut after).is_ok());
    assert_eq!(after.drawables[0].opacity, 0.5);
    assert_eq!(before.drawables[0].positions, after.drawables[0].positions);
    assert!(std::sync::Arc::ptr_eq(
        &before.drawables[0].indices,
        &after.drawables[0].indices
    ));
    assert!(std::sync::Arc::ptr_eq(
        &before.drawables[0].uvs,
        &after.drawables[0].uvs
    ));
    let revision = doc.revision();
    for value in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
        assert!(!doc.set_mesh_opacity(&id(3), value).status.is_ok());
        assert_eq!(doc.revision(), revision);
    }
    assert!(!doc.set_mesh_opacity(&id(999), 0.5).status.is_ok());
    assert_eq!(
        doc.set_mesh_opacity(&id(3), 0.5).changes.kind,
        ChangeKind::None
    );
}

#[test]
fn prepared_selection_tracks_precision_axes_and_restored_documents() {
    let mut doc = document(2);
    let parameter = id(10);
    assert!(doc
        .create_parameter(Parameter {
            id: parameter.clone(),
            minimum: -1.,
            maximum: 1.,
            decimal_places: 6,
            ..Default::default()
        })
        .status
        .is_ok());
    for mesh in [3, 4] {
        let base = doc.get_mesh(&id(mesh)).unwrap().base_positions.clone();
        assert!(doc
            .create_binding(MeshBinding {
                id: id(mesh + 10),
                mesh_id: id(mesh),
                axes: vec![BindingAxis {
                    id: Default::default(),
                    samples: Default::default(),
                    parameter_id: parameter.clone(),
                    keys: vec![-1., 0., 1.]
                }],
                keyforms: [-1., 0., 1.]
                    .into_iter()
                    .map(|key| MeshKeyform {
                        keys: vec![key],
                        positions: base
                            .iter()
                            .map(|p| Vec2::new(p.x + 20. * key, p.y))
                            .collect(),
                        ..Default::default()
                    })
                    .collect(),
            })
            .status
            .is_ok());
    }
    let original = doc.clone();
    let values = [(parameter.clone(), 0.049)].into();
    let mut evaluator = FrameEvaluator::default();
    let mut frame = DrawableFrame::default();
    assert!(evaluator.evaluate(&doc, &values, &mut frame).is_ok());
    let interpolated = frame.clone();
    assert_eq!(frame.drawables[0].positions, frame.drawables[1].positions);
    let mut p = doc.get_parameter(&parameter).unwrap().clone();
    p.decimal_places = 1;
    assert!(doc.replace_parameter(p).status.is_ok());
    assert!(evaluator.evaluate(&doc, &values, &mut frame).is_ok());
    let snapped = frame.drawables[0].positions.clone();
    assert_ne!(snapped, interpolated.drawables[0].positions);
    let mut zero = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &[(parameter.clone(), 0.)].into(), &mut zero).is_ok());
    assert_eq!(snapped, zero.drawables[0].positions);
    // One binding diverges from the shared grid; the other must stay snapped.
    let mut b = doc.get_binding(&id(13)).unwrap().clone();
    b.axes[0].keys.remove(1);
    b.axes[0].samples.remove(1);
    b.keyforms.remove(1);
    assert!(doc.replace_binding(b).status.is_ok());
    assert!(evaluator.evaluate(&doc, &values, &mut frame).is_ok());
    assert_ne!(frame.drawables[0].positions, snapped);
    assert_eq!(frame.drawables[1].positions, snapped);
    doc.restore_from(&original);
    assert!(evaluator.evaluate(&doc, &values, &mut frame).is_ok());
    assert_eq!(frame.drawables, interpolated.drawables);
}

#[test]
fn animated_workspace_reuses_deformers_and_invalidates_compiled_blend_constraints() {
    let mut doc = document(2);
    assert!(doc
        .create_transform(Transform {
            id: id(30),
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .create_transform(Transform {
            id: id(31),
            parent_id: Some(id(30).into()),
            data: TransformData::Warp(WarpTransform {
                rows: 1,
                columns: 1,
                quad: false,
                bezier: None,
                points: vec![
                    Vec2::new(0., 0.),
                    Vec2::new(1., 0.),
                    Vec2::new(0., 1.),
                    Vec2::new(1., 1.)
                ],
            }),
            ..Default::default()
        })
        .status
        .is_ok());
    for (n, kind) in [(20, ParameterKind::BlendShape), (21, ParameterKind::Normal)] {
        assert!(doc
            .create_parameter(Parameter {
                id: id(n),
                kind,
                minimum: 0.,
                maximum: 1.,
                ..Default::default()
            })
            .status
            .is_ok());
    }
    assert!(doc
        .create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: id(22),
            parameter_id: id(20),
            keys: vec![0., 1.],
            base_key_idx: 0,
        })
        .status
        .is_ok());
    let mut constraint = BlendShapeConstraint {
        samples: Default::default(),
        id: id(23),
        parameter_id: id(21),
        keys: vec![0., 1.],
        weights: vec![0., 1.],
    };
    assert!(doc
        .create_blend_constraint(constraint.clone())
        .status
        .is_ok());
    for n in [3, 4] {
        assert!(doc
            .create_blend_binding(BlendShapeBinding {
                id: id(n + 40),
                target_id: id(n),
                target_kind: BlendShapeTargetKind::Mesh,
                key_table_id: id(22),
                constraint_ids: vec![id(23)],
                keyforms: DeltaKeyforms::Mesh(vec![
                    DeltaMeshKeyform {
                        positions: vec![Vec2::default(); 3],
                        ..Default::default()
                    },
                    DeltaMeshKeyform {
                        positions: vec![Vec2::new(100., 0.); 3],
                        ..Default::default()
                    },
                ]),
            })
            .status
            .is_ok());
    }
    let original = doc.clone();
    let mut evaluator = FrameEvaluator::default();
    let mut frame = DrawableFrame::default();
    let samples: Vec<PreviewValues> = [0., 0.25, 0.5, 1.]
        .into_iter()
        .map(|v| [(id(20), v), (id(21), 0.5)].into())
        .collect();
    for _ in 0..3 {
        for sample in &samples {
            assert!(evaluator.evaluate(&doc, sample, &mut frame).is_ok());
        }
    }
    let count = allocations(|| {
        for sample in &samples {
            assert!(evaluator.evaluate(&doc, sample, &mut frame).is_ok());
        }
    });
    assert_eq!(
        count, 0,
        "Warmed Warp/Rotation and constrained BlendShapes must reuse storage"
    );
    let initial = frame.clone();
    // Same table/constraint shared by both meshes; a constraint edit must
    // invalidate compiled samples even when reusing the same evaluator.
    constraint.weights = vec![1., 1.];
    assert!(doc.replace_blend_constraint(constraint).status.is_ok());
    assert!(evaluator.evaluate(&doc, &samples[3], &mut frame).is_ok());
    assert!(
        (frame.drawables[0].positions[0].x - initial.drawables[0].positions[0].x - 50.).abs()
            < 1e-6
    );
    assert_eq!(frame.drawables[0].positions, frame.drawables[1].positions);
    doc.restore_from(&original);
    assert!(evaluator.evaluate(&doc, &samples[3], &mut frame).is_ok());
    assert_eq!(frame.drawables, initial.drawables);
    assert_eq!(frame.deformers, initial.deformers);
    // Reusing the evaluator for another document must not retain graph slots.
    let other = document(1);
    assert!(evaluator
        .evaluate(&other, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert!(frame.deformers.is_empty());
    assert_eq!(frame.drawables.len(), 1);
}

mod support;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use support::{image, prepare};
use zipzi_core::state::{AssetContent, DocumentState, RecordDomain};
use zipzi_core::*;

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

fn document(count: usize) -> DocumentState {
    let base = DocumentState::new(
        id(1),
        Canvas::new(640., 480., Vec2::new(320., 240.), 100.).into(),
    )
    .unwrap();
    let mut builder = base.builder();
    builder.create_asset(image(id(2), "", 64, 64)).unwrap();
    for n in 0..count {
        builder
            .create_mesh(Mesh {
                id: id(n + 3),
                runtime_id: format!("Mesh{n}"),
                texture_asset_id: (id(2)).into(),
                vertex_ids: vec![1, 2, 3],
                base_positions: vec![Vec2::new(0., 0.), Vec2::new(10., 0.), Vec2::new(0., 10.)],
                uvs: vec![Vec2::new(0., 0.); 3],
                triangles: vec![[1, 2, 3]],
                ..Default::default()
            })
            .unwrap();
    }
    builder.freeze().unwrap().state
}

#[test]
fn snapshot_identity_checks_and_clones_do_not_allocate() {
    let original = document(100);
    let content = original.content_id();
    assert_eq!(
        allocations(|| assert_eq!(original.clone().content_id(), content)),
        0
    );
    let mut mesh = original.mesh(&id(3)).unwrap().clone();
    mesh.name = "changed".into();
    let renamed = prepare(&original, |b| b.replace_mesh(mesh)).unwrap().state;
    assert_ne!(renamed.content_id(), content);
    assert_eq!(original.content_id(), content);
}

#[test]
fn workspace_reuses_geometry_and_preserves_output_on_late_failure() {
    let doc = document(100);
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

    let mut overflow = doc.mesh(&id(3)).unwrap().clone();
    overflow.base_positions[0].x = f32::MAX;
    let rotation_id = id(9999);
    let mut builder = doc.builder();
    builder
        .create_transform(Transform {
            id: rotation_id.clone(),
            runtime_id: "OverflowRotation".into(),
            data: TransformData::Rotation(RotationTransform {
                pose: RotationPose {
                    scale: 2.,
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        })
        .unwrap();
    overflow.deformer_id = rotation_id;
    builder.replace_mesh(overflow).unwrap();
    let overflow_state = builder.freeze().unwrap().state;
    assert_eq!(
        evaluator
            .evaluate(&overflow_state, &preview, &mut output)
            .code,
        "NON_FINITE"
    );
    assert_eq!(output, fresh);
    assert!(evaluator.evaluate(&doc, &preview, &mut output).is_ok());
    assert!(evaluate_frame(&doc, &preview, &mut fresh).is_ok());
    assert_eq!(output, fresh);
}

// A renderer that retains the previous published frame through the next
// evaluation prevents PreviewState::invalidate from reclaiming its buffers.
// Measure that ownership cost before adopting persistent Arc frame caching.
#[test]
fn published_frame_ownership_preserves_snapshots_and_recycles_released_buffers() {
    use zipzi_core::preview::PreviewState;

    let doc = document(100);
    let parameter = id(200);
    let doc = prepare(&doc, |b| {
        b.create_parameter(Parameter {
            id: parameter.clone(),
            minimum: -1.0,
            maximum: 1.0,
            ..Default::default()
        })
    })
    .unwrap()
    .state;

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
    let doc = document(2);
    let parameter = id(10);
    let doc = prepare(&doc, |b| {
        b.create_parameter(Parameter {
            id: parameter.clone(),
            minimum: -1.,
            maximum: 1.,
            ..Default::default()
        })
    })
    .unwrap()
    .state;
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
            positions: doc.mesh(&id(3)).unwrap().base_positions.clone(),
            ..Default::default()
        }],
    };
    let doc = prepare(&doc, |b| b.create_binding(binding.clone()))
        .unwrap()
        .state;
    assert_eq!(doc.binding_for_mesh(&id(3)).unwrap().id, binding.id);
    let saved = doc;
    let mut moved = saved.binding(&binding.id).unwrap().clone();
    moved.mesh_id = id(4);
    let doc = prepare(&saved, |b| b.replace_binding(moved)).unwrap().state;
    assert!(doc.binding_for_mesh(&id(3)).is_none());
    assert_eq!(doc.binding_for_mesh(&id(4)).unwrap().id, binding.id);
    let deleted = prepare(&doc, |b| b.remove_binding(&binding.id))
        .unwrap()
        .state;
    assert!(deleted.binding_for_mesh(&id(4)).is_none());
    assert_eq!(saved.binding_for_mesh(&id(3)).unwrap().id, binding.id);
    assert!(saved.binding_for_mesh(&id(4)).is_none());
}

#[test]
fn static_geometry_tracks_snapshot_edits_and_topology_changes() {
    use std::sync::Arc;
    let doc = document(1);
    let mut evaluator = FrameEvaluator::default();
    let mut frame = DrawableFrame::default();
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    let indices = frame.drawables[0].indices.clone();
    let uvs = frame.drawables[0].uvs.clone();
    let mut mesh = doc.mesh(&id(3)).unwrap().clone();
    mesh.name = "renamed".into();
    mesh.base_positions[0] = Vec2::new(2., 3.);
    let doc = prepare(&doc, |b| b.replace_mesh(mesh)).unwrap().state;
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert_eq!(indices, frame.drawables[0].indices);
    assert_eq!(uvs, frame.drawables[0].uvs);
    let warmed_uvs = frame.drawables[0].uvs.clone();
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert!(Arc::ptr_eq(&warmed_uvs, &frame.drawables[0].uvs));
    let mut mesh = doc.mesh(&id(3)).unwrap().clone();
    mesh.uvs[0] = Vec2::new(0.5, 0.75);
    let doc = prepare(&doc, |b| b.replace_mesh(mesh)).unwrap().state;
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert_ne!(uvs, frame.drawables[0].uvs);
    assert_eq!(frame.drawables[0].uvs[0], Vec2::new(0.5, 0.75));
    assert_eq!(uvs[0], Vec2::default());
    // Reusing an evaluator on independent frozen content is safe.
    let other = document(2);
    assert!(evaluator
        .evaluate(&other, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert_eq!(frame.drawables.len(), 2);
}

#[test]
fn preview_rekeys_on_metadata_resources_and_previous_snapshots() {
    use std::sync::Arc;
    use zipzi_core::preview::PreviewState;
    let saved = document(1);
    let mut preview = PreviewState::default();
    let first = preview.frame(&saved, 1).unwrap();
    let mut mesh = saved.mesh(&id(3)).unwrap().clone();
    mesh.name = "renamed".into();
    let doc = prepare(&saved, |b| b.replace_mesh(mesh)).unwrap().state;
    let renamed = preview.frame(&doc, 1).unwrap();
    assert_eq!(preview.evaluation_count(), 2);
    assert_eq!(first.drawables[0].positions, renamed.drawables[0].positions);
    assert!(Arc::ptr_eq(&renamed, &preview.frame(&doc, 1).unwrap()));
    let mut asset = doc.asset(&id(2)).unwrap().clone();
    let AssetContent::Image { resource, .. } = &mut asset.content else {
        panic!("expected image");
    };
    *resource = Default::default();
    let changed = prepare(&doc, |b| b.replace_asset(asset)).unwrap();
    assert_eq!(changed.writes.records()[0].domain, RecordDomain::Asset);
    let replaced = preview.frame(&changed.state, 1).unwrap();
    assert!(!Arc::ptr_eq(&renamed, &replaced));
    assert_eq!(renamed.drawables[0].uvs, replaced.drawables[0].uvs);
    assert_eq!(first, preview.frame(&saved, 1).unwrap());
}

#[test]
fn keyform_order_edit_invalidates_prepared_group_bounds() {
    let doc = document(2);
    let doc = prepare(&doc, |b| {
        b.create_parameter(Parameter {
            id: id(10),
            ..Default::default()
        })
    })
    .unwrap()
    .state;
    let mut form = MeshKeyform {
        keys: vec![0.],
        positions: doc.mesh(&id(3)).unwrap().base_positions.clone(),
        draw_order: Some(0.),
        ..Default::default()
    };
    let doc = prepare(&doc, |b| {
        b.create_binding(MeshBinding {
            id: id(11),
            mesh_id: id(3),
            axes: vec![BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: id(10),
                keys: vec![0.],
            }],
            keyforms: vec![form.clone()],
        })
    })
    .unwrap()
    .state;
    let mut evaluator = FrameEvaluator::default();
    let mut frame = DrawableFrame::default();
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert!(frame.drawables[0].render_order < frame.drawables[1].render_order);
    form.draw_order = Some(10.);
    let doc = prepare(&doc, |b| b.set_mesh_keyform(&id(11), form))
        .unwrap()
        .state;
    assert!(evaluator
        .evaluate(&doc, &PreviewValues::new(), &mut frame)
        .is_ok());
    assert!(frame.drawables[0].render_order > frame.drawables[1].render_order);
}

#[test]
fn opacity_edits_preserve_geometry_and_reject_invalid_values() {
    use zipzi_core::state::{DocumentBuilder, PropertyPatch};

    let opacity = |b: &mut DocumentBuilder, n, value| {
        let id = id(n);
        let mesh = b
            .mesh(&id)
            .ok_or_else(|| Status::error("MISSING_MESH", &id))?;
        b.patch_property(PropertyPatch::Mesh {
            id,
            keyform: None,
            appearance: Appearance {
                opacity: value,
                ..mesh.appearance.clone()
            },
            draw_order: mesh.draw_order,
        })
    };
    let doc = document(2);
    let mut before = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &PreviewValues::default(), &mut before).is_ok());
    let edit = prepare(&doc, |b| opacity(b, 3, 0.5)).unwrap();
    assert_eq!(edit.writes.records()[0].domain, RecordDomain::Mesh);
    let doc = edit.state;
    let mut after = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &PreviewValues::default(), &mut after).is_ok());
    assert_eq!(after.drawables[0].opacity, 0.5);
    assert_eq!(before.drawables[0].positions, after.drawables[0].positions);
    assert_eq!(before.drawables[0].indices, after.drawables[0].indices);
    assert_eq!(before.drawables[0].uvs, after.drawables[0].uvs);
    for value in [f32::NAN, f32::INFINITY, -0.1] {
        assert!(prepare(&doc, |b| opacity(b, 3, value)).is_err());
    }
    // Persistent appearance preserves finite opacity above one for format roundtrips.
    let over = prepare(&doc, |b| opacity(b, 3, 1.1)).unwrap();
    assert_eq!(over.state.mesh(&id(3)).unwrap().appearance.opacity, 1.1);
    assert!(prepare(&doc, |b| opacity(b, 999, 0.5)).is_err());
    let unchanged = prepare(&doc, |b| opacity(b, 3, 0.5)).unwrap();
    assert!(unchanged.writes.is_empty());
    assert_eq!(unchanged.state.content_id(), doc.content_id());
}

#[test]
fn prepared_selection_tracks_precision_axes_and_restored_documents() {
    let doc = document(2);
    let parameter = id(10);
    let doc = prepare(&doc, |b| {
        b.create_parameter(Parameter {
            id: parameter.clone(),
            minimum: -1.,
            maximum: 1.,
            decimal_places: 6,
            ..Default::default()
        })
    })
    .unwrap()
    .state;
    let mut builder = doc.builder();
    for mesh in [3, 4] {
        let base = doc.mesh(&id(mesh)).unwrap().base_positions.clone();
        builder
            .create_binding(MeshBinding {
                id: id(mesh + 10),
                mesh_id: id(mesh),
                axes: vec![BindingAxis {
                    id: Default::default(),
                    samples: Default::default(),
                    parameter_id: parameter.clone(),
                    keys: vec![-1., 0., 1.],
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
            .unwrap();
    }
    let doc = builder.freeze().unwrap().state;
    let original = doc.clone();
    let values = [(parameter.clone(), 0.049)].into();
    let mut evaluator = FrameEvaluator::default();
    let mut frame = DrawableFrame::default();
    assert!(evaluator.evaluate(&doc, &values, &mut frame).is_ok());
    let interpolated = frame.clone();
    assert_eq!(frame.drawables[0].positions, frame.drawables[1].positions);
    let mut p = doc.parameter(&parameter).unwrap().clone();
    p.decimal_places = 1;
    let doc = prepare(&doc, |b| b.replace_parameter(p)).unwrap().state;
    assert!(evaluator.evaluate(&doc, &values, &mut frame).is_ok());
    let snapped = frame.drawables[0].positions.clone();
    assert_ne!(snapped, interpolated.drawables[0].positions);
    let mut zero = DrawableFrame::default();
    assert!(evaluate_frame(&doc, &[(parameter.clone(), 0.)].into(), &mut zero).is_ok());
    assert_eq!(snapped, zero.drawables[0].positions);
    // One binding diverges from the shared grid; the other must stay snapped.
    let mut b = doc.binding(&id(13)).unwrap().clone();
    b.axes[0].keys.remove(1);
    b.axes[0].samples.remove(1);
    b.keyforms.remove(1);
    let doc = prepare(&doc, |builder| builder.replace_binding(b))
        .unwrap()
        .state;
    assert!(evaluator.evaluate(&doc, &values, &mut frame).is_ok());
    assert_ne!(frame.drawables[0].positions, snapped);
    assert_eq!(frame.drawables[1].positions, snapped);
    assert!(evaluator.evaluate(&original, &values, &mut frame).is_ok());
    assert_eq!(frame.drawables, interpolated.drawables);
}

#[test]
fn animated_workspace_reuses_deformers_and_invalidates_compiled_blend_constraints() {
    let mut builder = document(2).builder();
    builder
        .create_transform(Transform {
            id: id(30),
            runtime_id: "Rotation".into(),
            ..Default::default()
        })
        .unwrap();
    builder
        .create_transform(Transform {
            id: id(31),
            runtime_id: "Warp".into(),
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
                    Vec2::new(1., 1.),
                ],
            }),
            ..Default::default()
        })
        .unwrap();
    for (n, kind) in [(20, ParameterKind::BlendShape), (21, ParameterKind::Normal)] {
        builder
            .create_parameter(Parameter {
                id: id(n),
                kind,
                minimum: 0.,
                maximum: 1.,
                ..Default::default()
            })
            .unwrap();
    }
    builder
        .create_blend_key_table(BlendShapeKeyTable {
            samples: Default::default(),
            base_sample_id: Default::default(),
            id: id(22),
            parameter_id: id(20),
            keys: vec![0., 1.],
            base_key_idx: 0,
        })
        .unwrap();
    let mut constraint = BlendShapeConstraint {
        samples: Default::default(),
        id: id(23),
        parameter_id: id(21),
        keys: vec![0., 1.],
        weights: vec![0., 1.],
    };
    builder.create_blend_constraint(constraint.clone()).unwrap();
    for n in [3, 4] {
        builder
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
            .unwrap();
    }
    let doc = builder.freeze().unwrap().state;
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
    let doc = prepare(&doc, |b| b.replace_blend_constraint(constraint))
        .unwrap()
        .state;
    assert!(evaluator.evaluate(&doc, &samples[3], &mut frame).is_ok());
    assert!(
        (frame.drawables[0].positions[0].x - initial.drawables[0].positions[0].x - 50.).abs()
            < 1e-6
    );
    assert_eq!(frame.drawables[0].positions, frame.drawables[1].positions);
    assert!(evaluator
        .evaluate(&original, &samples[3], &mut frame)
        .is_ok());
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

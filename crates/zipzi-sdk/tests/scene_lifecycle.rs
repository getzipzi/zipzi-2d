use zipzi_contract::authoring::CapabilityState;
use zipzi_contract::authoring::ObjectKind;
use zipzi_core::document::SceneCollection as C;
use zipzi_core::document::SceneCopyScope as Scope;
use zipzi_core::document::SceneCreate as New;
use zipzi_core::document::SceneEdit as E;
use zipzi_core::document::SceneField as F;
use zipzi_core::document::SceneFieldValue as V;
use zipzi_core::BindingAxisEditing;
use zipzi_core::{document::*, *};
use zipzi_sdk::{AuthoringSession, ConversionControl, ConversionLimits};
fn id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    format!(
        "10000000-0000-4000-8000-{:012x}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
fn session() -> AuthoringSession {
    AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(),
                (Canvas::new(100., 100., Vec2::default(), 1.)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    )
}

#[test]
fn scene_label_patches_avoid_global_identity_scans_through_both_entrypoints() {
    let mut s = session();
    let mesh = triangle(&mut s, None, None, None, 0.);
    for label in ["First", "Second"] {
        let before = s.read_snapshot();
        let p = crate::support::prepared(&s, |c| {
            c.edit_scene(
                E::Patch {
                    id: mesh.clone(),
                    expected: before.snapshot.document().scene_field(&mesh, F::Label)?,
                    value: V::Label(label.into()),
                    clear_glue_pairs: false,
                },
                Default::default(),
                &Default::default(),
            )
        })
        .unwrap();
        assert_eq!(
            before.snapshot.document().mesh(&mesh).unwrap().name,
            s.snapshot().document().mesh(&mesh).unwrap().name
        );
        let r = s.publish(p.1).unwrap();
        assert_eq!(r.writes.records().len(), 1);
        assert_eq!(r.writes.records()[0].id, mesh);
        assert_eq!(s.snapshot().document().mesh(&mesh).unwrap().name, label);
    }
}

#[test]
fn ensure_drawing_role_respects_the_previous_drawing_parent_lock() {
    let mut s = session();
    let parent = part(&mut s, None);
    let child = part(&mut s, None);
    run(
        &mut s,
        E::EnsureDrawingGroup {
            owner: parent.clone(),
            parent: String::new(),
            before: None,
        },
    );
    run(
        &mut s,
        E::EnsureDrawingGroup {
            owner: child.clone(),
            parent: parent.clone(),
            before: None,
        },
    );
    lock(&mut s, &parent, true);
    let history = s.history_state();
    let version = s.version();
    let result = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            E::EnsureDrawingGroup {
                owner: child,
                parent: String::new(),
                before: None,
            },
            Default::default(),
            &Default::default(),
        )
    });
    assert_eq!(result.unwrap_err().code.as_str(), "OBJECT_LOCKED");
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
}
fn run(s: &mut AuthoringSession, e: E) -> zipzi_sdk::Publication {
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(e, Default::default(), &Default::default())
    })
    .unwrap();
    s.publish(p.1).unwrap()
}
fn create(
    s: &mut AuthoringSession,
    object: New,
    part: Option<&str>,
    parent: Option<&str>,
) -> String {
    let id = id();
    run(
        s,
        E::Create {
            id: id.clone(),
            name: String::new(),
            object,
            part_id: part.map(str::to_owned),
            parent_id: parent.map(str::to_owned),
            before: None,
            deformation_before: None,
        },
    );
    id
}
fn part(s: &mut AuthoringSession, parent: Option<&str>) -> String {
    let id = id();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_part({
                    let mut value = Part {
                        id: id.clone(),
                        parent_id: parent.unwrap_or("").into(),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Part")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    id
}
fn patch(s: &mut AuthoringSession, id: &str, value: V) -> zipzi_sdk::Publication {
    let expected = s
        .snapshot()
        .document()
        .scene_field(id, value.field())
        .unwrap();
    run(
        s,
        E::Patch {
            id: id.into(),
            expected,
            value,
            clear_glue_pairs: false,
        },
    )
}
fn asset(s: &mut AuthoringSession) -> String {
    let a = id();
    let image = crate::support::image_fixture(
        &a,
        "texture",
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png"),
    )
    .unwrap();
    let (_, p) = crate::support::prepared(s, |c| crate::support::insert_image(c, image)).unwrap();
    s.publish(p).unwrap();
    a
}
fn triangle(
    s: &mut AuthoringSession,
    texture: Option<&str>,
    part_id: Option<&str>,
    parent: Option<&str>,
    x: f32,
) -> String {
    let m = create(
        s,
        New::Mesh {
            texture: texture.unwrap_or("").into(),
        },
        part_id,
        parent,
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut value = e.read_document().unwrap().mesh(&m).unwrap().clone();
                value.vertex_ids = vec![0, 1, 2];
                value.base_positions =
                    vec![Vec2::new(x, 0.), Vec2::new(x + 1., 0.), Vec2::new(x, 1.)];
                value.uvs = vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)];
                value.triangles = vec![[0, 1, 2]];
                e.replace_mesh(value)
            })
            .and_then(|value| {
                let prepared = draft.prepare("Geometry")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    m
}
fn lock(s: &mut AuthoringSession, id: &str, locked: bool) {
    let mut state = s.snapshot().document().editor_state().clone();
    state.objects.insert(
        id.into(),
        ObjectEditorState {
            locked,
            ..Default::default()
        },
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state))
            .and_then(|value| {
                let prepared = draft.prepare("Lock")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}
#[test]
fn drawing_root_and_legacy_glue_order_respect_member_locks() {
    let mut s = session();

    let mesh = triangle(&mut s, None, None, None, 0.);
    lock(&mut s, &mesh, true);
    let root = {
        let kind = ObjectKind::DrawOrderGroup;
        let object_id = (s.snapshot().document().id()).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    assert!(matches!(
        s.snapshot()
            .document()
            .scene_capabilities(root.0, &root.1)
            .unwrap()
            .patch,
        CapabilityState::Unavailable {
            code: "OBJECT_LOCKED"
        }
    ));
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_scene(
            E::SetDrawingRange {
                owner: String::new(),
                expected: [0, 0],
                range: [-10, 10]
            },
            Default::default(),
            &Default::default()
        ))
        .is_err()
    );
    lock(&mut s, &mesh, false);
    let a = create(
        &mut s,
        New::Glue {
            a: Default::default(),
            b: Default::default(),
        },
        None,
        None,
    );
    let b = create(
        &mut s,
        New::Glue {
            a: Default::default(),
            b: Default::default(),
        },
        None,
        None,
    );
    lock(&mut s, &a, true);
    let mut order = s.snapshot().document().hierarchy_order().clone();
    order.glues = vec![b.clone(), a.clone()];
    let version = s.version();
    assert_eq!(
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    e.edit_scene(
                        E::Move {
                            ids: vec![a],
                            destination: C::Glues,
                            before: None,
                        },
                        Default::default(),
                        &Default::default(),
                    )
                })
                .and_then(|value| {
                    let prepared = draft.prepare("Reorder locked Glue")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), version);
}
#[test]
fn pending_scene_objects_are_native_and_roundtrip_without_invented_targets() {
    let mut s = session();
    let p = part(&mut s, None);
    let mesh = create(
        &mut s,
        New::Mesh {
            texture: SceneReference::Unspecified,
        },
        Some(&p),
        None,
    );
    let warp = create(&mut s, New::Warp { bounds: None }, Some(&p), None);
    let rotation = create(&mut s, New::Rotation { origin: None }, None, Some(&warp));
    let os = create(&mut s, New::Offscreen, Some(&p), None);
    let glue = create(
        &mut s,
        New::Glue {
            a: mesh.clone().into(),
            b: SceneReference::Unspecified,
        },
        None,
        None,
    );
    assert_eq!(
        s.snapshot()
            .document()
            .transform(&warp)
            .unwrap()
            .warp()
            .unwrap()
            .points
            .len(),
        36
    );
    assert_eq!(
        s.snapshot()
            .document()
            .transform(&rotation)
            .unwrap()
            .parent(),
        warp
    );
    assert!(s.snapshot().document().builder().freeze().is_ok());
    let frame = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    assert_eq!(frame.drawables.len(), 1);
    assert!(!frame.drawables[0].renderable);
    assert!(s
        .snapshot()
        .document()
        .scene_projection()
        .drawing
        .is_empty());
    assert!(s.snapshot().document().scene_projection().glues.is_empty());
    let reopened = crate::support::roundtrip(s.snapshot());
    let reopened = reopened.document();
    assert_eq!(
        reopened.mesh(&mesh).unwrap().texture_asset_id,
        SceneReference::Unspecified
    );
    assert_eq!(
        reopened.glue(&glue).unwrap().mesh_b_id,
        SceneReference::Unspecified
    );
    assert_eq!(reopened.offscreen(&os).unwrap().part_id, p);
}
#[test]
fn untextured_geometry_is_evaluable_and_glue_uses_geometry_availability() {
    let mut s = session();
    let a = triangle(&mut s, None, None, None, 0.);
    let b = triangle(&mut s, None, None, None, 10.);
    let g = create(
        &mut s,
        New::Glue {
            a: a.clone().into(),
            b: b.clone().into(),
        },
        None,
        None,
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut glue = e.read_document().unwrap().glue(&g).unwrap().clone();
                glue.pairs.push(GlueVertexPair {
                    vertex_a: Some(0),
                    vertex_b: Some(0),
                    weight_a: 0.5,
                    weight_b: 0.5,
                    ..Default::default()
                });
                e.replace_glue(glue)
            })
            .and_then(|value| {
                let prepared = draft.prepare("Pair")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let projection = s.snapshot().document().scene_projection();
    assert!(projection.drawing.is_empty());
    assert!(projection.glues.contains(&g));
    let frame = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    assert!(frame.drawables.iter().all(|d| !d.renderable && d.enabled));
    assert_eq!(frame.drawables[0].positions[0].x, 5.);
    assert_eq!(
        s.snapshot().document().mesh(&a).unwrap().vertex_ids,
        vec![0, 1, 2]
    );
    assert!(frame.render_plan.is_empty());
    let texture = asset(&mut s);
    patch(&mut s, &a, V::Texture(texture.clone().into()));
    assert_eq!(
        s.snapshot().document().scene_projection().drawing,
        vec![a.clone()]
    );
    assert_eq!(
        crate::support::frame(s.snapshot(), &Default::default())
            .unwrap()
            .drawables[0]
            .positions[0]
            .x,
        5.
    );
}
#[test]
fn mask_and_offscreen_unavailability_propagates_without_unmasked_fallback() {
    let mut s = session();
    let texture = asset(&mut s);
    let p = part(&mut s, None);
    let a = triangle(&mut s, None, None, None, 0.);
    let b = triangle(&mut s, Some(&texture), None, None, 2.);
    let c = triangle(&mut s, Some(&texture), None, None, 4.);
    patch(&mut s, &b, V::Masks(vec![a.clone()]));
    patch(&mut s, &c, V::Masks(vec![b.clone()]));
    let os = create(&mut s, New::Offscreen, Some(&p), None);
    patch(&mut s, &os, V::Masks(vec![c.clone()]));
    let d = triangle(&mut s, Some(&texture), Some(&p), None, 6.);
    let projection = s.snapshot().document().scene_projection();
    assert_eq!(projection.geometry.len(), 4);
    assert!(projection.drawing.is_empty());
    assert!(projection.composites.is_empty());
    let disabled = projection.disabled.iter().find(|v| v.id == d).unwrap();
    assert_eq!(
        disabled.chain,
        vec![d.clone(), os.clone(), c.clone(), b.clone(), a.clone()]
    );
    let frame = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    assert!(frame.render_plan.is_empty());
    assert!(!frame.offscreens[0].renderable);
    assert!(!frame.drawables[3].positions.is_empty());
    patch(&mut s, &a, V::Texture(texture.into()));
    assert_eq!(s.snapshot().document().scene_projection().drawing.len(), 4);
    assert_eq!(
        s.snapshot().document().scene_projection().composites,
        vec![os]
    );
    s.undo(s.version()).unwrap();
    assert!(s
        .snapshot()
        .document()
        .scene_projection()
        .drawing
        .is_empty());
}
#[test]
fn imported_unresolved_scene_targets_are_distinct_from_unconfigured_and_strict_export_rejects() {
    let mut s = session();
    let a = create(
        &mut s,
        New::Mesh {
            texture: SceneReference::Unresolved {
                runtime_id: "TextureFromImport".into(),
            },
        },
        None,
        None,
    );
    assert!(
        matches!(&s.snapshot().document().scene_projection().disabled[0].reason,SceneDisabledReason::UnresolvedReference {runtime_id} if runtime_id=="TextureFromImport")
    );
    let version = s.version();
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            E::Patch {
                id: a.clone(),
                expected: s.snapshot().document().scene_field(&a, F::Texture).unwrap(),
                value: V::Texture(SceneReference::Resolved { object_id: id() }),
                clear_glue_pairs: false,
            },
            Default::default(),
            &Default::default(),
        )
    });
    assert_eq!(plan.unwrap_err().code.as_str(), "MISSING_ASSET");
    assert_eq!(s.version(), version);
    assert!(zipzi_project::encode_document_cmo3(
        s.snapshot().document(),
        s.snapshot().resources(),
        &Default::default()
    )
    .is_err());
    let reopened = crate::support::roundtrip(s.snapshot());
    let reopened = reopened.document();
    assert_eq!(
        reopened.mesh(&a).unwrap().texture_asset_id,
        s.snapshot().document().mesh(&a).unwrap().texture_asset_id
    );
}
#[test]
fn glue_management_reorder_preserves_execution_and_redo() {
    let mut s = session();
    let a = triangle(&mut s, None, None, None, 0.);
    let b = triangle(&mut s, None, None, None, 10.);
    let g = create(
        &mut s,
        New::Glue {
            a: a.clone().into(),
            b: b.clone().into(),
        },
        None,
        None,
    );
    let h = create(
        &mut s,
        New::Glue {
            a: a.into(),
            b: b.into(),
        },
        None,
        None,
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                for (id, wa, wb) in [(&g, 1., 0.), (&h, 0., 1.)] {
                    let mut glue = e.read_document().unwrap().glue(id).unwrap().clone();
                    glue.intensity = 0.5;
                    glue.pairs = vec![GlueVertexPair {
                        vertex_a: Some(0),
                        vertex_b: Some(0),
                        weight_a: wa,
                        weight_b: wb,
                        ..Default::default()
                    }];
                    e.replace_glue(glue)?;
                }
                Ok(())
            })
            .and_then(|value| {
                let prepared = draft.prepare("Distinct glue effects")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    let eval = s.snapshot().content_id();
    let r = run(
        &mut s,
        E::Move {
            ids: vec![h.clone()],
            destination: C::Glues,
            before: Some(g.clone()),
        },
    );
    assert!(!r.writes.is_empty());

    assert_ne!(s.snapshot().content_id(), eval);
    assert_eq!(
        s.snapshot().document().glue_order(),
        &[g.clone(), h.clone()]
    );
    assert_eq!(
        s.snapshot()
            .document()
            .scene_collection(&C::Glues)
            .unwrap()
            .iter()
            .map(|h| h.to_owned())
            .collect::<Vec<_>>(),
        vec![h.clone(), g]
    );
    assert_eq!(
        crate::support::frame(s.snapshot(), &Default::default())
            .unwrap()
            .drawables,
        before.drawables
    );
    s.undo(s.version()).unwrap();
    let history = s.history_state();
    let version = s.version();
    let r = run(
        &mut s,
        E::Move {
            ids: vec![h],
            destination: C::Glues,
            before: None,
        },
    );
    assert!(r.writes.is_empty());
    assert_eq!(s.history_state(), history);
    assert_eq!(s.version(), version);
}
#[test]
fn deformation_palette_order_is_separate_from_reparent_and_organization() {
    let mut s = session();
    let a = create(&mut s, New::Rotation { origin: None }, None, None);
    let b = create(&mut s, New::Rotation { origin: None }, None, None);
    let m = triangle(&mut s, None, None, Some(&a), 0.);
    let org = s
        .snapshot()
        .document()
        .hierarchy_order()
        .organization
        .clone();
    let eval = s.snapshot().content_id();
    run(
        &mut s,
        E::Move {
            ids: vec![b.clone()],
            destination: C::Deformation { parent_id: None },
            before: Some(a.clone()),
        },
    );
    assert_eq!(s.snapshot().document().hierarchy_order().organization, org);
    assert_ne!(s.snapshot().content_id(), eval);
    assert_eq!(s.snapshot().document().mesh(&m).unwrap().deformer_id, a);
    assert_eq!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_scene(
            E::Move {
                ids: vec![m.clone()],
                destination: C::Deformation {
                    parent_id: Some(b.clone())
                },
                before: None,
            },
            Default::default(),
            &Default::default()
        ))
        .unwrap_err()
        .code
        .as_str(),
        "WRONG_SCENE_COLLECTION"
    );
    run(
        &mut s,
        E::Reparent {
            edit: ReparentEdit::keep_local(vec![m.clone()], b.clone()),
        },
    );
    assert_eq!(s.snapshot().document().mesh(&m).unwrap().deformer_id, b);
}
#[test]
fn selected_and_deformation_subtree_copies_have_explicit_distinct_closures() {
    let mut s = session();
    let parent = create(&mut s, New::Rotation { origin: None }, None, None);
    let child = create(&mut s, New::Warp { bounds: None }, None, Some(&parent));
    let mesh = triangle(&mut s, None, None, Some(&child), 0.);
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            E::Duplicate {
                ids: vec![parent.clone()],
                scope: Scope::Selected,
                destination: None,
                before: None,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    assert_eq!(p.0.copied_ids.len(), 1);
    s.publish(p.1).unwrap();
    assert_eq!(
        s.snapshot().document().transform(&child).unwrap().parent(),
        parent
    );
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            E::Duplicate {
                ids: vec![parent.clone()],
                scope: Scope::DeformationSubtree,
                destination: None,
                before: None,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    let ids = p.0.copied_ids.clone();
    s.publish(p.1).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .transform(&ids[&child])
            .unwrap()
            .parent(),
        ids[&parent]
    );
    assert_eq!(
        s.snapshot()
            .document()
            .mesh(&ids[&mesh])
            .unwrap()
            .deformer_id,
        ids[&child]
    );
}
#[test]
fn copy_remaps_internal_masks_glue_and_owned_axis_sample_identities() {
    let mut s = session();
    let a = triangle(&mut s, None, None, None, 0.);
    let b = triangle(&mut s, None, None, None, 2.);
    patch(&mut s, &b, V::Masks(vec![a.clone()]));
    let g = create(
        &mut s,
        New::Glue {
            a: a.clone().into(),
            b: b.clone().into(),
        },
        None,
        None,
    );
    let parameter = id();
    let binding = id();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: parameter.clone(),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.create_binding(MeshBinding {
                    id: binding.clone(),
                    mesh_id: a.clone(),
                    axes: vec![BindingAxis::new(parameter.clone(), vec![-1., 1.])],
                    keyforms: vec![
                        MeshKeyform {
                            keys: vec![-1.],
                            positions: e
                                .read_document()
                                .unwrap()
                                .mesh(&a)
                                .unwrap()
                                .base_positions
                                .clone(),
                            ..Default::default()
                        },
                        MeshKeyform {
                            keys: vec![1.],
                            positions: e
                                .read_document()
                                .unwrap()
                                .mesh(&a)
                                .unwrap()
                                .base_positions
                                .clone(),
                            ..Default::default()
                        },
                    ],
                })?;
                let mut glue = e.read_document().unwrap().glue(&g).unwrap().clone();
                glue.binding = Some(GlueBinding {
                    axes: vec![BindingAxis::new(parameter.clone(), vec![-1., 1.])],
                    keyforms: vec![
                        GlueKeyform {
                            samples: vec![],
                            intensity: 0.,
                        },
                        GlueKeyform {
                            samples: vec![],
                            intensity: 1.,
                        },
                    ],
                });
                e.replace_glue(glue)
            })
            .and_then(|value| {
                let prepared = draft.prepare("Bindings")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let old = s.snapshot().document().binding(&binding).unwrap().clone();
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            E::Duplicate {
                ids: vec![g.clone(), b.clone(), a.clone()],
                scope: Scope::Selected,
                destination: None,
                before: None,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    let ids = p.0.copied_ids.clone();
    s.publish(p.1).unwrap();
    assert_eq!(
        s.snapshot().document().mesh(&ids[&b]).unwrap().masks,
        vec![ids[&a].clone()]
    );
    assert_eq!(
        s.snapshot()
            .document()
            .glue(&ids[&g])
            .unwrap()
            .mesh_a_id
            .resolved_id(),
        Some(ids[&a].as_str())
    );
    let copy = s.snapshot().document().binding(&ids[&binding]).unwrap();
    assert_eq!(copy.axes[0].parameter_id, parameter);
    assert_eq!(copy.axes[0].id, ids[&old.axes[0].id]);
    assert_eq!(
        copy.keyforms[0].samples[0],
        ids[&old.keyforms[0].samples[0]]
    );
    assert!(s.snapshot().document().builder().freeze().is_ok());
}
#[test]
fn field_addresses_do_not_revive_after_edit_undo_or_context_roundtrips() {
    let mut s = session();
    let a = create(&mut s, New::Rotation { origin: None }, None, None);
    let b = create(&mut s, New::Rotation { origin: None }, None, None);
    let mesh = triangle(&mut s, None, None, Some(&a), 0.);
    let stale = s.begin_draft();
    patch(&mut s, &mesh, V::Enabled(false));
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.publish(stale.prepare("old query").unwrap())
            .unwrap_err()
            .code
            .as_str(),
        "STALE_SOURCE"
    );
    let stale = s.begin_draft();
    run(
        &mut s,
        E::Reparent {
            edit: ReparentEdit::keep_local(vec![mesh.clone()], b),
        },
    );
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().mesh(&mesh).unwrap().deformer_id, a);
    assert_eq!(
        s.publish(stale.prepare("old context").unwrap())
            .unwrap_err()
            .code
            .as_str(),
        "STALE_SOURCE"
    );
}

#[test]
fn topology_context_expires_addresses_without_reporting_reference_target_changes() {
    let mut s = session();
    let mesh = triangle(&mut s, None, None, None, 0.);
    let stale = s.begin_draft();
    let context = s
        .snapshot()
        .document()
        .scene_field(&mesh, F::Context)
        .unwrap();
    let mut value = s.snapshot().document().mesh(&mesh).unwrap().clone();
    value.triangles[0].rotate_left(1);
    let (_, p) = crate::support::prepared(&s, |c| c.replace_mesh(value)).unwrap();
    let r = s.publish(p).unwrap();
    assert_eq!(r.writes.records().len(), 1);
    assert_eq!(r.writes.records()[0].id, mesh);
    assert_eq!(
        s.snapshot()
            .document()
            .scene_field(&mesh, F::Context)
            .unwrap(),
        context
    );
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.publish(stale.prepare("old topology").unwrap())
            .unwrap_err()
            .code
            .as_str(),
        "STALE_SOURCE"
    );
}

#[test]
fn deletion_is_atomic_for_external_users_and_retires_owned_handles() {
    let mut s = session();
    let root = create(&mut s, New::Rotation { origin: None }, None, None);
    let mesh = triangle(&mut s, None, None, Some(&root), 0.);
    let error = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            E::Delete {
                ids: vec![root.clone()],
                scope: Scope::Selected,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap_err();
    assert_eq!(error.code.as_str(), "OBJECT_REFERENCED");
    assert!(s.snapshot().document().references_to(&root).contains(&mesh));
    let handle = {
        let kind = ObjectKind::Mesh;
        let object_id = (&mesh).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    run(
        &mut s,
        E::Delete {
            ids: vec![root.clone()],
            scope: Scope::DeformationSubtree,
        },
    );
    assert!(s.snapshot().document().mesh(&mesh).is_none());
    s.undo(s.version()).unwrap();
    assert!((s.read_snapshot().source != handle.2));
    assert_eq!(
        s.snapshot().document().mesh(&mesh).unwrap().deformer_id,
        root
    );
}
#[test]
fn lock_destinations_reject_create_copy_and_stale_or_cancelled_plans() {
    let mut s = session();
    let owner = part(&mut s, None);
    let mesh = triangle(&mut s, None, None, None, 0.);
    lock(&mut s, &owner, true);
    let version = s.version();
    assert_eq!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_scene(
            E::Create {
                id: id(),
                name: "".into(),
                object: New::Mesh {
                    texture: Default::default()
                },
                part_id: Some(owner.clone()),
                parent_id: None,
                before: None,
                deformation_before: None,
            },
            Default::default(),
            &Default::default()
        ))
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    assert_eq!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_scene(
            E::Duplicate {
                ids: vec![mesh.clone()],
                scope: Scope::Selected,
                destination: Some(OrganizationCollection::PartMembers {
                    part_id: Some(owner)
                }),
                before: None,
            },
            Default::default(),
            &Default::default()
        ))
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), version);
    let control = ConversionControl::default();
    let p = crate::support::prepared(&s, |c| {
        c.edit_scene(
            E::Delete {
                ids: vec![mesh],
                scope: Scope::Selected,
            },
            Default::default(),
            &control,
        )
    })
    .unwrap();
    control.cancel();
    assert_eq!(
        s.publish_with_control(p.1, || !control.is_cancelled())
            .unwrap_err()
            .code
            .as_str(),
        "CANCELLED"
    );
    assert_eq!(s.version(), version);
    let mut other = session();
    let p = s.begin_draft().prepare("cross session").unwrap();
    assert_eq!(other.publish(p).unwrap_err().code.as_str(), "STALE_SOURCE");
}
#[test]
fn offscreen_transfer_requires_explicit_mapping_and_keeps_its_identity() {
    let mut s = session();
    let a = part(&mut s, None);
    let b = part(&mut s, None);
    let parameter = id();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: parameter.clone(),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                for (owner, keys) in [(&a, vec![-1., 1.]), (&b, vec![-1., 0., 1.])] {
                    e.create_scene_binding(SceneBinding {
                        id: id(),
                        axes: vec![BindingAxis::new(parameter.clone(), keys.clone())],
                        track: SceneTrack::Part {
                            target_id: owner.clone().into(),
                            keyforms: keys
                                .into_iter()
                                .map(|key| PartKeyform {
                                    keys: vec![key],
                                    draw_order: key,
                                    ..Default::default()
                                })
                                .collect(),
                        },
                    })?;
                }
                Ok(())
            })
            .and_then(|value| {
                let prepared = draft.prepare("Driven Parts")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let os = create(&mut s, New::Offscreen, Some(&a), None);
    patch(
        &mut s,
        &os,
        V::Offscreen {
            blend_mode: 0,
            flags: 4,
            keyforms: vec![
                OffscreenKeyform {
                    opacity: 0.3,
                    ..Default::default()
                },
                OffscreenKeyform {
                    opacity: 0.9,
                    ..Default::default()
                },
            ],
            part_keyform_indices: vec![0, 1],
        },
    );
    let handle = {
        let kind = ObjectKind::Offscreen;
        let object_id = (&os).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let stale = s.begin_draft();
    let mut raw = s.snapshot().document().offscreen(&os).unwrap().clone();
    raw.part_id = b.clone();
    raw.part_keyform_indices = vec![0, -1, 1];
    let (_, prepared) = crate::support::prepared(&s, |c| c.replace_offscreen(raw)).unwrap();
    assert_eq!(
        prepared
            .snapshot()
            .document()
            .offscreen(&os)
            .unwrap()
            .part_id,
        b
    );
    assert_eq!(s.snapshot().document().offscreen(&os).unwrap().part_id, a);
    let version = s.version();
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_scene(
            E::TransferOffscreen {
                id: os.clone(),
                part_id: b.clone(),
                part_keyform_indices: vec![0, 1]
            },
            Default::default(),
            &Default::default()
        ))
        .is_err()
    );
    assert_eq!(s.version(), version);
    let r = run(
        &mut s,
        E::TransferOffscreen {
            id: os.clone(),
            part_id: b.clone(),
            part_keyform_indices: vec![0, -1, 1],
        },
    );
    assert!(r.writes.records().iter().any(|r| r.id == os));
    assert_eq!(s.snapshot().document().offscreen(&os).unwrap().part_id, b);
    s.snapshot()
        .document()
        .require_object(handle.0, &handle.1)
        .unwrap();
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().offscreen(&os).unwrap().part_id, a);
    assert_eq!(
        s.publish(stale.prepare("old field").unwrap())
            .unwrap_err()
            .code
            .as_str(),
        "STALE_SOURCE"
    );
}
#[test]
fn drawing_moves_are_explicit_and_independent_of_organization_and_deformation() {
    let mut s = session();
    let texture = asset(&mut s);
    let a = part(&mut s, None);
    let b = part(&mut s, None);
    let m = triangle(&mut s, Some(&texture), Some(&a), None, 0.);
    let n = triangle(&mut s, Some(&texture), Some(&b), None, 2.);
    let order = s.snapshot().document().hierarchy_order().clone();
    let owner = s.snapshot().document().mesh(&m).unwrap().part_id.clone();
    let before = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    let r = run(
        &mut s,
        E::Move {
            ids: vec![m.clone()],
            destination: C::Drawing { owner: b.clone() },
            before: Some(n.clone()),
        },
    );
    assert!(!r.writes.is_empty());
    assert_eq!(s.snapshot().document().hierarchy_order(), &order);
    assert_eq!(s.snapshot().document().mesh(&m).unwrap().part_id, owner);
    assert!(r
        .writes
        .root_changed(zipzi_core::state::RootDomain::DrawOrderGroups));
    for (a, b) in before.drawables.iter().zip(
        crate::support::frame(s.snapshot(), &Default::default())
            .unwrap()
            .drawables
            .iter(),
    ) {
        assert_eq!(a.positions, b.positions);
    }
    let version = s.version();
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_scene(
            E::Move {
                ids: vec![b.clone()],
                destination: C::Drawing { owner: b },
                before: None,
            },
            Default::default(),
            &Default::default()
        ))
        .is_err()
    );
    assert_eq!(s.version(), version);
    assert_eq!(
        s.snapshot().document().organization_owner(&m),
        Some(a.as_str())
    );
}
#[test]
fn drawing_role_delete_undo_and_direct_replacement_do_not_revive_old_handles() {
    let mut s = session();
    let owner = part(&mut s, None);
    let mesh = triangle(&mut s, None, Some(&owner), None, 0.);
    let stale = s.begin_draft();
    run(
        &mut s,
        E::DissolveDrawingGroup {
            owner: owner.clone(),
        },
    );
    assert_eq!(s.snapshot().document().mesh(&mesh).unwrap().part_id, owner);
    s.snapshot()
        .document()
        .require_object(ObjectKind::Part, &owner)
        .unwrap();
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.publish(stale.prepare("old drawing role").unwrap())
            .unwrap_err()
            .code
            .as_str(),
        "STALE_SOURCE"
    );
}

#[test]
fn drawing_group_copy_creates_fresh_content_for_a_distinct_empty_part_role() {
    let mut s = session();
    let source = part(&mut s, None);
    let target = part(&mut s, None);
    let child = part(&mut s, Some(&source));
    let m = triangle(&mut s, None, Some(&source), None, 0.);
    let n = triangle(&mut s, None, Some(&child), None, 2.);
    let os = create(&mut s, New::Offscreen, Some(&child), None);
    let original = s.snapshot().document().mesh(&m).unwrap().clone();
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            E::DuplicateDrawingGroup {
                source: source.clone(),
                target: target.clone(),
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    let ids = p.0.copied_ids.clone();
    assert!(!ids.contains_key(&source));
    s.publish(p.1).unwrap();
    assert_eq!(
        s.snapshot().document().mesh(&ids[&m]).unwrap().part_id,
        target
    );
    assert_eq!(
        s.snapshot()
            .document()
            .part(&ids[&child])
            .unwrap()
            .parent_id,
        target
    );
    assert_eq!(
        s.snapshot().document().mesh(&ids[&n]).unwrap().part_id,
        ids[&child]
    );
    assert_eq!(
        s.snapshot()
            .document()
            .offscreen(&ids[&os])
            .unwrap()
            .part_id,
        ids[&child]
    );
    assert_eq!(s.snapshot().document().mesh(&m).unwrap(), &original);
    assert!({
        let kind = ObjectKind::DrawOrderGroup;
        let object_id = (&ids[&child]).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .is_ok());
    assert!(s.snapshot().document().builder().freeze().is_ok());
    let version = s.version();
    assert_eq!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_scene(
            E::DuplicateDrawingGroup { source, target },
            Default::default(),
            &Default::default()
        ))
        .unwrap_err()
        .code
        .as_str(),
        "DRAWING_COPY_TARGET_NOT_EMPTY"
    );
    assert_eq!(s.version(), version);
}

#[test]
fn drawing_copy_rejects_locked_owned_offscreen() {
    let mut s = session();
    let source = part(&mut s, None);
    let target = part(&mut s, None);
    let child = part(&mut s, Some(&source));
    triangle(&mut s, None, Some(&child), None, 0.);
    let os = create(&mut s, New::Offscreen, Some(&child), None);
    lock(&mut s, &os, true);
    let version = s.version();
    let history = s.history_state();
    assert_eq!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_scene(
            E::DuplicateDrawingGroup { source, target },
            Default::default(),
            &Default::default()
        ))
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
}

#[test]
fn drawing_root_copy_preserves_absent_deformation_parent() {
    let mut s = session();
    let mesh = triangle(&mut s, None, None, None, 0.);
    let target = part(&mut s, None);
    let mut builder = s.snapshot().document().builder();
    builder
        .set_draw_order_groups(Some(vec![zipzi_core::draw_order::DrawOrderGroup {
            owner: String::new(),
            items: vec![mesh.clone()],
            min_order: 0,
            max_order: 0,
        }]))
        .unwrap();
    s.replace(
        s.version(),
        zipzi_sdk::Snapshot::new(
            builder.freeze().unwrap().state,
            s.snapshot().resources().clone(),
        )
        .unwrap(),
    )
    .unwrap();
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            E::DuplicateDrawingGroup {
                source: String::new(),
                target: target.clone(),
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    let copy = p.0.copied_ids[&mesh].clone();
    s.publish(p.1).unwrap();
    let copied = s.snapshot().document().mesh(&copy).unwrap();
    assert_eq!(copied.part_id, target);
    assert!(copied.deformer_id.is_empty());
    assert!(s.snapshot().document().builder().freeze().is_ok());
    s.undo(s.version()).unwrap();
    assert!(s.snapshot().document().mesh(&copy).is_none());
    s.redo(s.version()).unwrap();
    assert!(s
        .snapshot()
        .document()
        .mesh(&copy)
        .unwrap()
        .deformer_id
        .is_empty());
}
#[test]
fn drawing_range_addresses_track_membership_and_preserve_noop_redo() {
    let mut s = session();
    let p = part(&mut s, None);
    let mesh = triangle(&mut s, None, Some(&p), None, 0.);
    let groups = zipzi_core::draw_order::resolved_groups(s.snapshot().document());
    let g = groups.iter().find(|g| g.owner == p).unwrap();
    let range = [g.min_order, g.max_order];
    let stale = s.begin_draft();
    run(
        &mut s,
        E::SetDrawingRange {
            owner: p.clone(),
            expected: range,
            range: [-10, 10],
        },
    );
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.publish(stale.prepare("old range").unwrap())
            .unwrap_err()
            .code
            .as_str(),
        "STALE_SOURCE"
    );
    let h = s.history_state();
    let v = s.version();
    let r = run(
        &mut s,
        E::SetDrawingRange {
            owner: p,
            expected: range,
            range,
        },
    );
    assert!(r.writes.is_empty());
    assert_eq!(s.history_state(), h);
    assert_eq!(s.version(), v);
    run(
        &mut s,
        E::Move {
            ids: vec![mesh],
            destination: C::Drawing {
                owner: String::new(),
            },
            before: None,
        },
    );
    s.undo(s.version()).unwrap();
}

#[test]
fn endpoint_changes_validate_existing_pairs_or_explicitly_clear_them() {
    let mut s = session();
    let a = triangle(&mut s, None, None, None, 0.);
    let b = triangle(&mut s, None, None, None, 2.);
    let empty = create(
        &mut s,
        New::Mesh {
            texture: Default::default(),
        },
        None,
        None,
    );
    let g = create(
        &mut s,
        New::Glue {
            a: a.clone().into(),
            b: b.into(),
        },
        None,
        None,
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut glue = e.read_document().unwrap().glue(&g).unwrap().clone();
                glue.pairs = vec![GlueVertexPair {
                    vertex_a: Some(0),
                    vertex_b: Some(0),
                    weight_a: 1.,
                    weight_b: 1.,
                    ..Default::default()
                }];
                e.replace_glue(glue)
            })
            .and_then(|value| {
                let prepared = draft.prepare("Pair")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let original = s.snapshot().document().glue(&g).unwrap().clone();
    let expected = s
        .snapshot()
        .document()
        .scene_field(&g, F::GlueEndpoints)
        .unwrap();
    let before = s.read_snapshot().source;
    let (_, p) = crate::support::prepared(&s, |c| {
        c.edit_scene(
            E::Patch {
                id: g.clone(),
                expected: expected.clone(),
                value: expected.clone(),
                clear_glue_pairs: true,
            },
            Default::default(),
            &Default::default(),
        )?;
        c.replace_glue(original)
    })
    .unwrap();
    assert!(s.publish(p).unwrap().writes.is_empty());
    assert_eq!(s.read_snapshot().source, before);
    let expected = s
        .snapshot()
        .document()
        .scene_field(&g, F::GlueEndpoints)
        .unwrap();
    let stale = s.begin_draft();
    let value = V::GlueEndpoints {
        a: a.into(),
        b: empty.into(),
    };
    let version = s.version();
    assert_eq!(
        crate::support::prepared(&s, |c| c.edit_scene(
            E::Patch {
                id: g.clone(),
                expected: expected.clone(),
                value: value.clone(),
                clear_glue_pairs: false
            },
            Default::default(),
            &Default::default()
        ))
        .unwrap_err()
        .code
        .as_str(),
        "MISSING_VERTEX"
    );
    assert_eq!(s.version(), version);
    let p = crate::support::prepared(&s, |c| {
        c.edit_scene(
            E::Patch {
                id: g.clone(),
                expected,
                value,
                clear_glue_pairs: true,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    s.publish(p.1).unwrap();
    assert!(s.snapshot().document().glue(&g).unwrap().pairs.is_empty());
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().glue(&g).unwrap().pairs.len(), 1);
    assert_eq!(
        s.publish(stale.prepare("old endpoints").unwrap())
            .unwrap_err()
            .code
            .as_str(),
        "STALE_SOURCE"
    );
}
#[test]
fn failed_scene_batches_and_reduced_copy_budgets_are_atomic() {
    let mut s = session();
    let mesh = triangle(&mut s, None, None, None, 0.);
    let version = s.version();
    let new = id();
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_scene(
                    E::Create {
                        id: new.clone(),
                        name: "".into(),
                        object: New::Rotation { origin: None },
                        part_id: None,
                        parent_id: None,
                        before: None,
                        deformation_before: None,
                    },
                    Default::default(),
                    &Default::default(),
                )?;
                e.edit_scene(
                    E::Reparent {
                        edit: ReparentEdit::keep_local(vec![new.clone()], new),
                    },
                    Default::default(),
                    &Default::default(),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Bad scene batch")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
    assert_eq!(s.version(), version);
    assert_eq!(
        crate::support::prepared(&s, |c| c.edit_scene(
            E::Duplicate {
                ids: vec![mesh],
                scope: Scope::Selected,
                destination: None,
                before: None
            },
            ConversionLimits {
                max_workspace_bytes: 1,
                ..Default::default()
            },
            &Default::default()
        ))
        .unwrap_err()
        .code
        .as_str(),
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(s.version(), version);
}

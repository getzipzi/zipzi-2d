use zipzi_core::{document::*, *};
use zipzi_sdk::{
    AuthoringSession, CapabilityState, ChangeCategory, ObjectKind, SceneCollection as C,
    SceneCopyScope as Scope, SceneCreate as New, SceneEdit as E, SceneField as F,
    SceneFieldValue as V,
};
fn id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    format!(
        "10000000-0000-4000-8000-{:012x}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
fn session() -> AuthoringSession {
    AuthoringSession::new(&id(), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap()
}

#[test]
fn scene_label_patches_avoid_global_identity_scans_through_both_entrypoints() {
    let mut s = session();
    let mesh = triangle(&mut s, None, None, None, 0.);
    let handle = s.handle(ObjectKind::Mesh, &mesh).unwrap();
    for planned in [false, true] {
        let intent = E::Patch {
            id: mesh.clone(),
            expected: s.document().scene_field(&mesh, F::Label).unwrap(),
            value: V::Label(format!("Label {planned}")),
            clear_glue_pairs: false,
        };
        assert!(s
            .document()
            .plan_scene_edit(intent.clone())
            .unwrap()
            .preserves_identities());
        let plan = s.plan_scene_edit(intent.clone()).unwrap();
        #[cfg(feature = "bench-instrumentation")]
        let capture = zipzi_core::benchmark::Capture::start();
        let (_, receipt) = s
            .edit("Label", None, |e| {
                if planned {
                    e.apply_scene_plan(&plan)
                } else {
                    e.edit_scene(intent)
                }
            })
            .unwrap();
        #[cfg(feature = "bench-instrumentation")]
        {
            let stages = capture.finish();
            assert_eq!(stages["sdk.publication.local"].calls, 1);
            assert!(
                !stages.contains_key("sdk.keys_in"),
                "Label enumerated all identities: {stages:?}"
            );
            for stage in ["core.authoring_identity_scan", "sdk.summarize.fallback"] {
                assert!(
                    !stages.contains_key(stage),
                    "Label repeated {stage}: {stages:?}"
                );
            }
        }
        assert!(receipt.changed);
        assert!(receipt.changes.created.is_empty());
        assert!(receipt.changes.deleted.is_empty());
        s.resolve_handle(&handle).unwrap();
    }
    let deletion = E::Delete {
        ids: vec![mesh.clone()],
        scope: Default::default(),
    };
    assert!(!s
        .document()
        .plan_scene_edit(deletion)
        .unwrap()
        .preserves_identities());
    let parent = create(&mut s, New::Rotation, None, None);
    assert!(!s
        .document()
        .plan_scene_edit(E::Reparent {
            edit: ReparentEdit::keep_local(vec![mesh], parent)
        })
        .unwrap()
        .preserves_identities());
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
    let result = s.plan_scene_edit(E::EnsureDrawingGroup {
        owner: child,
        parent: String::new(),
        before: None,
    });
    assert_eq!(result.unwrap_err().code.as_ref(), "OBJECT_LOCKED");
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
}
fn run(s: &mut AuthoringSession, e: E) -> zipzi_sdk::EditReceipt {
    let p = s.plan_scene_edit(e).unwrap();
    s.execute_scene_plan(&p).unwrap()
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
    s.edit("Part", None, |e| {
        e.create_part(Part {
            id: id.clone(),
            parent_id: parent.unwrap_or("").into(),
            ..Default::default()
        })
    })
    .unwrap();
    id
}
fn patch(s: &mut AuthoringSession, id: &str, value: V) -> zipzi_sdk::EditReceipt {
    let expected = s.document().scene_field(id, value.field()).unwrap();
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
    s.edit("Asset", None, |e| {
        e.create_asset(ImageAsset {
            unresolved_source: None,
            id: a.clone(),
            name: String::new(),
            content: Some(zipzi_core::ImageContent {
                source: "assets/texture.png".into(),
                sha256: "0".repeat(64),
                width: 1,
                height: 1,
            }),
        })
    })
    .unwrap();
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
    s.edit("Geometry", None, |e| {
        let mut value = e.candidate_document().get_mesh(&m).unwrap().clone();
        value.vertex_ids = vec![0, 1, 2];
        value.base_positions = vec![Vec2::new(x, 0.), Vec2::new(x + 1., 0.), Vec2::new(x, 1.)];
        value.uvs = vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)];
        value.triangles = vec![[0, 1, 2]];
        e.replace_mesh(value)
    })
    .unwrap();
    m
}
fn lock(s: &mut AuthoringSession, id: &str, locked: bool) {
    let mut state = s.document().editor_state().clone();
    state.objects.insert(
        id.into(),
        ObjectEditorState {
            locked,
            ..Default::default()
        },
    );
    s.edit("Lock", None, |e| e.replace_editor_state(state))
        .unwrap();
}
#[test]
fn drawing_root_and_legacy_glue_order_respect_member_locks() {
    let mut s = session();
    assert_eq!(
        s.drawing_group_address(s.document().id()).unwrap().id(),
        s.drawing_group_address("").unwrap().id()
    );
    let mesh = triangle(&mut s, None, None, None, 0.);
    lock(&mut s, &mesh, true);
    let root = s
        .handle(ObjectKind::DrawOrderGroup, s.document().id())
        .unwrap();
    assert!(matches!(
        s.scene_capabilities(&root).unwrap().patch,
        CapabilityState::Unavailable {
            code: "OBJECT_LOCKED"
        }
    ));
    assert!(s
        .plan_scene_edit(E::SetDrawingRange {
            owner: String::new(),
            expected: [0, 0],
            range: [-10, 10]
        })
        .is_err());
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
    let mut order = s.document().hierarchy_order().clone();
    order.glues = vec![b, a];
    let version = s.version();
    assert_eq!(
        s.edit("Reorder locked Glue", None, |e| e
            .replace_hierarchy_order(order))
            .unwrap_err()
            .code
            .as_ref(),
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
    let rotation = create(&mut s, New::Rotation, None, Some(&warp));
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
        s.document()
            .get_transform(&warp)
            .unwrap()
            .warp()
            .unwrap()
            .points
            .len(),
        4
    );
    assert_eq!(
        s.document().get_transform(&rotation).unwrap().parent(),
        warp
    );
    assert!(s.document().validate_structure().is_empty());
    let frame = s.preview_frame().unwrap();
    assert_eq!(frame.drawables.len(), 1);
    assert!(!frame.drawables[0].renderable);
    assert!(s.scene_projection().drawing.is_empty());
    assert!(s.scene_projection().glues.is_empty());
    let encoded = zipzi_project::encode_project(s.document()).unwrap();
    let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value["format_version"], 21);
    let reopened = zipzi_project::decode_project(&encoded).unwrap();
    assert_eq!(
        reopened.get_mesh(&mesh).unwrap().texture_asset_id,
        SceneReference::Unspecified
    );
    assert_eq!(
        reopened.get_glue(&glue).unwrap().mesh_b_id,
        SceneReference::Unspecified
    );
    assert_eq!(reopened.get_offscreen(&os).unwrap().part_id, p);
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
    s.edit("Pair", None, |e| {
        let mut glue = e.candidate_document().get_glue(&g).unwrap().clone();
        glue.pairs.push(GlueVertexPair {
            vertex_a: Some(0),
            vertex_b: Some(0),
            weight_a: 0.5,
            weight_b: 0.5,
            ..Default::default()
        });
        e.replace_glue(glue)
    })
    .unwrap();
    let projection = s.scene_projection();
    assert!(projection.drawing.is_empty());
    assert!(projection.glues.contains(&g));
    let frame = s.preview_frame().unwrap();
    assert!(frame.drawables.iter().all(|d| !d.renderable && d.enabled));
    assert_eq!(frame.drawables[0].positions[0].x, 5.);
    assert_eq!(s.geometry(&a).unwrap().vertex_ids, vec![0, 1, 2]);
    assert!(frame.render_plan.is_empty());
    let texture = asset(&mut s);
    patch(&mut s, &a, V::Texture(texture.clone().into()));
    assert_eq!(s.scene_projection().drawing, vec![a.clone()]);
    assert_eq!(s.preview_frame().unwrap().drawables[0].positions[0].x, 5.);
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
    let projection = s.scene_projection();
    assert_eq!(projection.geometry.len(), 4);
    assert!(projection.drawing.is_empty());
    assert!(projection.composites.is_empty());
    let disabled = projection.disabled.iter().find(|v| v.id == d).unwrap();
    assert_eq!(
        disabled.chain,
        vec![d.clone(), os.clone(), c.clone(), b.clone(), a.clone()]
    );
    let frame = s.preview_frame().unwrap();
    assert!(frame.render_plan.is_empty());
    assert!(!frame.offscreens[0].renderable);
    assert!(!frame.drawables[3].positions.is_empty());
    patch(&mut s, &a, V::Texture(texture.into()));
    assert_eq!(s.scene_projection().drawing.len(), 4);
    assert_eq!(s.scene_projection().composites, vec![os]);
    s.undo().unwrap();
    assert!(s.scene_projection().drawing.is_empty());
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
        matches!(&s.scene_projection().disabled[0].reason,SceneDisabledReason::UnresolvedReference {runtime_id} if runtime_id=="TextureFromImport")
    );
    let version = s.version();
    let plan = s.plan_scene_edit(E::Patch {
        id: a.clone(),
        expected: s.document().scene_field(&a, F::Texture).unwrap(),
        value: V::Texture(SceneReference::Resolved { object_id: id() }),
        clear_glue_pairs: false,
    });
    assert_eq!(plan.unwrap_err().code.as_ref(), "MISSING_ASSET");
    assert_eq!(s.version(), version);
    let error = s
        .encode_cmo3(&zipzi_sdk::Cmo3Options::default())
        .unwrap_err();
    assert!(
        error.to_string().contains("SCENE_NEEDS_CONFIGURATION")
            || matches!(error, zipzi_sdk::Cmo3Error::Unsupported(_))
    );
    let reopened =
        zipzi_project::decode_project(&zipzi_project::encode_project(s.document()).unwrap())
            .unwrap();
    assert_eq!(
        reopened.get_mesh(&a).unwrap().texture_asset_id,
        s.document().get_mesh(&a).unwrap().texture_asset_id
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
    s.edit("Distinct glue effects", None, |e| {
        for (id, wa, wb) in [(&g, 1., 0.), (&h, 0., 1.)] {
            let mut glue = e.candidate_document().get_glue(id).unwrap().clone();
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
    .unwrap();
    let before = s.preview_frame().unwrap();
    let eval = s.evaluation_revision();
    let r = run(
        &mut s,
        E::Move {
            ids: vec![h.clone()],
            destination: C::Glues,
            before: Some(g.clone()),
        },
    );
    assert!(r.changed);
    assert!(!r
        .changes
        .categories
        .contains(&ChangeCategory::EvaluationInputs));
    assert_eq!(s.evaluation_revision(), eval);
    assert_eq!(s.document().glue_order(), &[g.clone(), h.clone()]);
    assert_eq!(
        s.scene_collection(&C::Glues)
            .unwrap()
            .iter()
            .map(|h| h.id().to_owned())
            .collect::<Vec<_>>(),
        vec![h.clone(), g]
    );
    assert_eq!(s.preview_frame().unwrap().drawables, before.drawables);
    s.undo().unwrap();
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
    assert!(!r.changed);
    assert_eq!(s.history_state(), history);
    assert_eq!(s.version(), version);
}
#[test]
fn deformation_palette_order_is_separate_from_reparent_and_organization() {
    let mut s = session();
    let a = create(&mut s, New::Rotation, None, None);
    let b = create(&mut s, New::Rotation, None, None);
    let m = triangle(&mut s, None, None, Some(&a), 0.);
    let org = s.document().hierarchy_order().organization.clone();
    let eval = s.evaluation_revision();
    run(
        &mut s,
        E::Move {
            ids: vec![b.clone()],
            destination: C::Deformation { parent_id: None },
            before: Some(a.clone()),
        },
    );
    assert_eq!(s.document().hierarchy_order().organization, org);
    assert_eq!(s.evaluation_revision(), eval);
    assert_eq!(s.document().get_mesh(&m).unwrap().deformer_id, a);
    assert_eq!(
        s.plan_scene_edit(E::Move {
            ids: vec![m.clone()],
            destination: C::Deformation {
                parent_id: Some(b.clone())
            },
            before: None,
        })
        .unwrap_err()
        .code
        .as_ref(),
        "WRONG_SCENE_COLLECTION"
    );
    run(
        &mut s,
        E::Reparent {
            edit: ReparentEdit::keep_local(vec![m.clone()], b.clone()),
        },
    );
    assert_eq!(s.document().get_mesh(&m).unwrap().deformer_id, b);
}
#[test]
fn selected_and_deformation_subtree_copies_have_explicit_distinct_closures() {
    let mut s = session();
    let parent = create(&mut s, New::Rotation, None, None);
    let child = create(&mut s, New::Warp { bounds: None }, None, Some(&parent));
    let mesh = triangle(&mut s, None, None, Some(&child), 0.);
    let p = s
        .plan_scene_edit(E::Duplicate {
            ids: vec![parent.clone()],
            scope: Scope::Selected,
            destination: None,
            before: None,
        })
        .unwrap();
    assert_eq!(p.copied_ids().len(), 1);
    s.execute_scene_plan(&p).unwrap();
    assert_eq!(s.document().get_transform(&child).unwrap().parent(), parent);
    let p = s
        .plan_scene_edit(E::Duplicate {
            ids: vec![parent.clone()],
            scope: Scope::DeformationSubtree,
            destination: None,
            before: None,
        })
        .unwrap();
    let ids = p.copied_ids().clone();
    s.execute_scene_plan(&p).unwrap();
    assert_eq!(
        s.document().get_transform(&ids[&child]).unwrap().parent(),
        ids[&parent]
    );
    assert_eq!(
        s.document().get_mesh(&ids[&mesh]).unwrap().deformer_id,
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
    s.edit("Bindings", None, |e| {
        e.create_parameter(Parameter {
            id: parameter.clone(),
            ..Default::default()
        })?;
        e.create_binding(MeshBinding {
            id: binding.clone(),
            mesh_id: a.clone(),
            axes: vec![BindingAxis::new(parameter.clone(), vec![-1., 1.])],
            keyforms: vec![
                MeshKeyform {
                    keys: vec![-1.],
                    positions: e
                        .candidate_document()
                        .get_mesh(&a)
                        .unwrap()
                        .base_positions
                        .clone(),
                    ..Default::default()
                },
                MeshKeyform {
                    keys: vec![1.],
                    positions: e
                        .candidate_document()
                        .get_mesh(&a)
                        .unwrap()
                        .base_positions
                        .clone(),
                    ..Default::default()
                },
            ],
        })?;
        let mut glue = e.candidate_document().get_glue(&g).unwrap().clone();
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
    .unwrap();
    let old = s.document().get_binding(&binding).unwrap().clone();
    let p = s
        .plan_scene_edit(E::Duplicate {
            ids: vec![g.clone(), b.clone(), a.clone()],
            scope: Scope::Selected,
            destination: None,
            before: None,
        })
        .unwrap();
    let ids = p.copied_ids().clone();
    s.execute_scene_plan(&p).unwrap();
    assert_eq!(
        s.document().get_mesh(&ids[&b]).unwrap().masks,
        vec![ids[&a].clone()]
    );
    assert_eq!(
        s.document()
            .get_glue(&ids[&g])
            .unwrap()
            .mesh_a_id
            .resolved_id(),
        Some(ids[&a].as_str())
    );
    let copy = s.document().get_binding(&ids[&binding]).unwrap();
    assert_eq!(copy.axes[0].parameter_id, parameter);
    assert_eq!(copy.axes[0].id, ids[&old.axes[0].id]);
    assert_eq!(
        copy.keyforms[0].samples[0],
        ids[&old.keyforms[0].samples[0]]
    );
    assert!(s.document().validate_structure().is_empty());
}
#[test]
fn field_addresses_do_not_revive_after_edit_undo_or_context_roundtrips() {
    let mut s = session();
    let a = create(&mut s, New::Rotation, None, None);
    let b = create(&mut s, New::Rotation, None, None);
    let mesh = triangle(&mut s, None, None, Some(&a), 0.);
    let handle = s.handle(ObjectKind::Mesh, &mesh).unwrap();
    let address = s.scene_field_address(&mesh, F::Label).unwrap();
    patch(&mut s, &mesh, V::Enabled(false));
    let p = s
        .plan_scene_field_patch(&address, V::Label("After".into()), false)
        .unwrap();
    s.execute_scene_plan(&p).unwrap();
    s.undo().unwrap();
    assert!(s
        .plan_scene_field_patch(&address, V::Label("Other".into()), false)
        .is_err());
    s.resolve_handle(&handle).unwrap();
    let address = s.scene_field_address(&mesh, F::Label).unwrap();
    run(
        &mut s,
        E::Reparent {
            edit: ReparentEdit::keep_local(vec![mesh.clone()], b),
        },
    );
    s.undo().unwrap();
    assert_eq!(s.document().get_mesh(&mesh).unwrap().deformer_id, a);
    assert!(s
        .plan_scene_field_patch(&address, address.expected().clone(), false)
        .is_err());
}

#[test]
fn topology_context_expires_addresses_without_reporting_reference_target_changes() {
    let mut s = session();
    let mesh = triangle(&mut s, None, None, None, 0.);
    let address = s.scene_field_address(&mesh, F::Label).unwrap();
    let context = s.document().scene_field(&mesh, F::Context).unwrap();
    let mut replacement = s.document().get_mesh(&mesh).unwrap().clone();
    replacement.triangles[0].rotate_left(1);
    let (_, receipt) = s
        .edit("Reindex triangle", None, |e| e.replace_mesh(replacement))
        .unwrap();
    assert!(receipt
        .changes
        .scene_fields
        .iter()
        .any(|c| c.entity.id == mesh && c.field == F::Context));
    assert!(!receipt
        .changes
        .scene_references
        .iter()
        .any(|c| c.entity.id == mesh && c.field == F::Context));
    assert_eq!(
        s.document().scene_field(&mesh, F::Context).unwrap(),
        context
    );
    assert!(s
        .plan_scene_field_patch(&address, V::Label("Stale".into()), false)
        .is_err());
    s.undo().unwrap();
    assert!(s
        .plan_scene_field_patch(&address, V::Label("Still stale".into()), false)
        .is_err());
}
#[test]
fn deletion_is_atomic_for_external_users_and_retires_owned_handles() {
    let mut s = session();
    let root = create(&mut s, New::Rotation, None, None);
    let mesh = triangle(&mut s, None, None, Some(&root), 0.);
    let error = s
        .plan_scene_edit(E::Delete {
            ids: vec![root.clone()],
            scope: Scope::Selected,
        })
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "OBJECT_REFERENCED");
    assert!(error.referrers.contains(&mesh));
    let handle = s.handle(ObjectKind::Mesh, &mesh).unwrap();
    run(
        &mut s,
        E::Delete {
            ids: vec![root.clone()],
            scope: Scope::DeformationSubtree,
        },
    );
    assert!(s.document().get_mesh(&mesh).is_none());
    s.undo().unwrap();
    assert!(s.resolve_handle(&handle).is_err());
    assert_eq!(s.document().get_mesh(&mesh).unwrap().deformer_id, root);
}
#[test]
fn lock_destinations_reject_create_copy_and_stale_or_cancelled_plans() {
    let mut s = session();
    let owner = part(&mut s, None);
    let mesh = triangle(&mut s, None, None, None, 0.);
    lock(&mut s, &owner, true);
    let version = s.version();
    assert_eq!(
        s.plan_scene_edit(E::Create {
            id: id(),
            name: "".into(),
            object: New::Mesh {
                texture: Default::default()
            },
            part_id: Some(owner.clone()),
            parent_id: None,
            before: None,
            deformation_before: None,
        })
        .unwrap_err()
        .code
        .as_ref(),
        "OBJECT_LOCKED"
    );
    assert_eq!(
        s.plan_scene_edit(E::Duplicate {
            ids: vec![mesh.clone()],
            scope: Scope::Selected,
            destination: Some(OrganizationCollection::PartMembers {
                part_id: Some(owner)
            }),
            before: None,
        })
        .unwrap_err()
        .code
        .as_ref(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), version);
    let control = ConversionControl::default();
    let p = s
        .plan_scene_edit_with_control(
            E::Delete {
                ids: vec![mesh],
                scope: Scope::Selected,
            },
            &control,
        )
        .unwrap();
    control.cancel();
    assert_eq!(
        s.execute_scene_plan(&p).unwrap_err().code.as_ref(),
        "AUTHORING_CANCELLED"
    );
    assert_eq!(s.version(), version);
    let mut other = session();
    assert!(other.execute_scene_plan(&p).is_err());
}
#[test]
fn offscreen_transfer_requires_explicit_mapping_and_keeps_its_identity() {
    let mut s = session();
    let a = part(&mut s, None);
    let b = part(&mut s, None);
    let parameter = id();
    s.edit("Driven Parts", None, |e| {
        e.create_parameter(Parameter {
            id: parameter.clone(),
            ..Default::default()
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
    let handle = s.handle(ObjectKind::Offscreen, &os).unwrap();
    let address = s.scene_field_address(&os, F::Offscreen).unwrap();
    let mut raw = s.document().get_offscreen(&os).unwrap().clone();
    raw.part_id = b.clone();
    raw.part_keyform_indices = vec![0, -1, 1];
    assert_eq!(
        s.edit("Unsafe legacy rebind", None, |e| e.replace_offscreen(raw))
            .unwrap_err()
            .code
            .as_ref(),
        "OFFSCREEN_REBIND_REQUIRES_CONVERSION"
    );
    let version = s.version();
    assert!(s
        .plan_scene_edit(E::TransferOffscreen {
            id: os.clone(),
            part_id: b.clone(),
            part_keyform_indices: vec![0, 1]
        })
        .is_err());
    assert_eq!(s.version(), version);
    let r = run(
        &mut s,
        E::TransferOffscreen {
            id: os.clone(),
            part_id: b.clone(),
            part_keyform_indices: vec![0, -1, 1],
        },
    );
    assert!(r
        .changes
        .organization_references
        .iter()
        .any(|r| r.entity.id == os && r.after.as_deref() == Some(b.as_str())));
    s.resolve_handle(&handle).unwrap();
    s.undo().unwrap();
    assert_eq!(s.document().get_offscreen(&os).unwrap().part_id, a);
    assert!(s
        .plan_scene_field_patch(&address, address.expected().clone(), false)
        .is_err());
}
#[test]
fn drawing_moves_are_explicit_and_independent_of_organization_and_deformation() {
    let mut s = session();
    let texture = asset(&mut s);
    let a = part(&mut s, None);
    let b = part(&mut s, None);
    let m = triangle(&mut s, Some(&texture), Some(&a), None, 0.);
    let n = triangle(&mut s, Some(&texture), Some(&b), None, 2.);
    let order = s.document().hierarchy_order().clone();
    let owner = s.document().get_mesh(&m).unwrap().part_id.clone();
    let before = s.preview_frame().unwrap();
    let r = run(
        &mut s,
        E::Move {
            ids: vec![m.clone()],
            destination: C::Drawing { owner: b.clone() },
            before: Some(n.clone()),
        },
    );
    assert!(r.changed);
    assert_eq!(s.document().hierarchy_order(), &order);
    assert_eq!(s.document().get_mesh(&m).unwrap().part_id, owner);
    assert!(r.changes.collections.iter().any(|c|matches!(c,zipzi_sdk::CollectionRef::Scene {collection:C::Drawing {owner},..} if owner==&b)));
    for (a, b) in before
        .drawables
        .iter()
        .zip(s.preview_frame().unwrap().drawables.iter())
    {
        assert_eq!(a.positions, b.positions);
    }
    let version = s.version();
    assert!(s
        .plan_scene_edit(E::Move {
            ids: vec![b.clone()],
            destination: C::Drawing { owner: b },
            before: None,
        })
        .is_err());
    assert_eq!(s.version(), version);
    assert_eq!(s.document().organization_owner(&m), Some(a.as_str()));
}
#[test]
fn drawing_role_delete_undo_and_direct_replacement_do_not_revive_old_handles() {
    let mut s = session();
    let owner = part(&mut s, None);
    let mesh = triangle(&mut s, None, Some(&owner), None, 0.);
    let part_handle = s.handle(ObjectKind::Part, &owner).unwrap();
    let role = s.handle(ObjectKind::DrawOrderGroup, &owner).unwrap();
    let root = s
        .handle(ObjectKind::DrawOrderGroup, s.document().id())
        .unwrap();
    let address = s.drawing_group_address(&owner).unwrap();
    assert!(matches!(
        s.scene_capabilities(&root).unwrap().delete,
        CapabilityState::Unsupported {
            code: "SYSTEM_DRAWING_ROOT"
        }
    ));
    run(
        &mut s,
        E::DissolveDrawingGroup {
            owner: owner.clone(),
        },
    );
    assert!(s.resolve_handle(&role).is_err());
    s.resolve_handle(&part_handle).unwrap();
    assert_eq!(s.document().get_mesh(&mesh).unwrap().part_id, owner);
    s.undo().unwrap();
    assert!(s.resolve_handle(&role).is_err());
    assert!(s.plan_drawing_range_patch(&address, [-5, 5]).is_err());
    s.resolve_handle(&root).unwrap();
    let role = s.handle(ObjectKind::DrawOrderGroup, &owner).unwrap();
    let original = zipzi_core::draw_order::resolved_groups(s.document());
    let mut removed = original.clone();
    let children = removed
        .iter()
        .find(|g| g.owner == owner)
        .unwrap()
        .items
        .clone();
    removed.retain(|g| g.owner != owner);
    let root = removed.iter_mut().find(|g| g.owner.is_empty()).unwrap();
    root.items.retain(|id| id != &owner);
    root.items.extend(children);
    s.edit("Remove and restore role", None, |e| {
        e.replace_draw_order_groups(removed)?;
        e.replace_draw_order_groups(original)
    })
    .unwrap();
    assert!(s.resolve_handle(&role).is_err());
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
    let original = s.document().get_mesh(&m).unwrap().clone();
    let p = s
        .plan_scene_edit(E::DuplicateDrawingGroup {
            source: source.clone(),
            target: target.clone(),
        })
        .unwrap();
    let ids = p.copied_ids().clone();
    assert!(!ids.contains_key(&source));
    s.execute_scene_plan(&p).unwrap();
    assert_eq!(s.document().get_mesh(&ids[&m]).unwrap().part_id, target);
    assert_eq!(
        s.document().get_part(&ids[&child]).unwrap().parent_id,
        target
    );
    assert_eq!(
        s.document().get_mesh(&ids[&n]).unwrap().part_id,
        ids[&child]
    );
    assert_eq!(
        s.document().get_offscreen(&ids[&os]).unwrap().part_id,
        ids[&child]
    );
    assert_eq!(s.document().get_mesh(&m).unwrap(), &original);
    assert!(s.handle(ObjectKind::DrawOrderGroup, &ids[&child]).is_ok());
    assert!(s.document().validate_structure().is_empty());
    let version = s.version();
    assert_eq!(
        s.plan_scene_edit(E::DuplicateDrawingGroup { source, target })
            .unwrap_err()
            .code
            .as_ref(),
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
        s.plan_scene_edit(E::DuplicateDrawingGroup { source, target })
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
}

#[test]
fn drawing_root_copy_preserves_absent_deformation_parent() {
    let mut s = session();
    let target = part(&mut s, None);
    let mesh = triangle(&mut s, None, None, None, 0.);
    s.edit("Explicit root", None, |e| {
        e.replace_draw_order_groups(vec![zipzi_core::draw_order::DrawOrderGroup {
            owner: String::new(),
            items: vec![mesh.clone()],
            min_order: 0,
            max_order: 0,
        }])
    })
    .unwrap();
    let p = s
        .plan_scene_edit(E::DuplicateDrawingGroup {
            source: String::new(),
            target: target.clone(),
        })
        .unwrap();
    let copy = p.copied_ids()[&mesh].clone();
    s.execute_scene_plan(&p).unwrap();
    let copied = s.document().get_mesh(&copy).unwrap();
    assert_eq!(copied.part_id, target);
    assert!(copied.deformer_id.is_empty());
    assert!(s.document().validate_structure().is_empty());
    s.undo().unwrap();
    assert!(s.document().get_mesh(&copy).is_none());
    s.redo().unwrap();
    assert!(s.document().get_mesh(&copy).unwrap().deformer_id.is_empty());
}
#[test]
fn drawing_range_addresses_track_membership_and_preserve_noop_redo() {
    let mut s = session();
    let p = part(&mut s, None);
    let mesh = triangle(&mut s, None, Some(&p), None, 0.);
    let address = s.drawing_group_address(&p).unwrap();
    let range = address.range();
    let plan = s.plan_drawing_range_patch(&address, [-10, 10]).unwrap();
    s.execute_scene_plan(&plan).unwrap();
    s.undo().unwrap();
    assert!(s.plan_drawing_range_patch(&address, range).is_err());
    let current = s.drawing_group_address(&p).unwrap();
    let history = s.history_state();
    let version = s.version();
    let plan = s
        .plan_drawing_range_patch(&current, current.range())
        .unwrap();
    assert!(plan.is_noop());
    s.execute_scene_plan(&plan).unwrap();
    assert_eq!(s.history_state(), history);
    assert_eq!(s.version(), version);
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
    s.undo().unwrap();
    assert!(s.plan_drawing_range_patch(&current, [-2, 2]).is_err());
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
    s.edit("Pair", None, |e| {
        let mut glue = e.candidate_document().get_glue(&g).unwrap().clone();
        glue.pairs = vec![GlueVertexPair {
            vertex_a: Some(0),
            vertex_b: Some(0),
            weight_a: 1.,
            weight_b: 1.,
            ..Default::default()
        }];
        e.replace_glue(glue)
    })
    .unwrap();
    // Clearing and restoring identical pair content inside one batch must still
    // retire the original handle. Exercise direct and preplanned application.
    for planned in [false, true] {
        let original = s.document().get_glue(&g).unwrap().clone();
        let pair = s
            .handle(ObjectKind::GlueVertexPair, &original.pairs[0].id)
            .unwrap();
        let expected = s.document().scene_field(&g, F::GlueEndpoints).unwrap();
        let intent = E::Patch {
            id: g.clone(),
            expected: expected.clone(),
            value: expected,
            clear_glue_pairs: true,
        };
        assert!(!s
            .document()
            .plan_scene_edit(intent.clone())
            .unwrap()
            .preserves_identities());
        let plan = s.plan_scene_edit(intent.clone()).unwrap();
        let (_, receipt) = s
            .edit("Clear and restore pairs", None, |e| {
                if planned {
                    e.apply_scene_plan(&plan)?;
                } else {
                    e.edit_scene(intent)?;
                }
                e.replace_glue(original)
            })
            .unwrap();
        assert!(receipt.changed);
        assert!(s.resolve_handle(&pair).is_err());
        s.undo().unwrap();
        s.redo().unwrap();
        assert!(s.resolve_handle(&pair).is_err());
    }
    let address = s.scene_field_address(&g, F::GlueEndpoints).unwrap();
    let value = V::GlueEndpoints {
        a: a.into(),
        b: empty.into(),
    };
    let version = s.version();
    assert_eq!(
        s.plan_scene_field_patch(&address, value.clone(), false)
            .unwrap_err()
            .code
            .as_ref(),
        "MISSING_VERTEX"
    );
    assert_eq!(s.version(), version);
    let p = s.plan_scene_field_patch(&address, value, true).unwrap();
    s.execute_scene_plan(&p).unwrap();
    assert!(s.document().get_glue(&g).unwrap().pairs.is_empty());
    s.undo().unwrap();
    assert_eq!(s.document().get_glue(&g).unwrap().pairs.len(), 1);
    assert!(s
        .plan_scene_field_patch(&address, address.expected().clone(), false)
        .is_err());
}
#[test]
fn failed_scene_batches_and_reduced_copy_budgets_are_atomic() {
    let mut s = session();
    let mesh = triangle(&mut s, None, None, None, 0.);
    let version = s.version();
    let new = id();
    assert!(s
        .edit("Bad scene batch", None, |e| {
            e.edit_scene(E::Create {
                id: new.clone(),
                name: "".into(),
                object: New::Rotation,
                part_id: None,
                parent_id: None,
                before: None,
                deformation_before: None,
            })?;
            e.edit_scene(E::Reparent {
                edit: ReparentEdit::keep_local(vec![new.clone()], new),
            })
        })
        .is_err());
    assert_eq!(s.version(), version);
    let p = s
        .plan_scene_edit(E::Duplicate {
            ids: vec![mesh],
            scope: Scope::Selected,
            destination: None,
            before: None,
        })
        .unwrap();
    s.set_authoring_limits(AuthoringLimits {
        max_conversion_bytes: 1,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        s.execute_scene_plan(&p).unwrap_err().code.as_ref(),
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(s.version(), version);
}

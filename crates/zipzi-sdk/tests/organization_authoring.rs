use zipzi_core::BindingAxisEditing;
use zipzi_core::{
    document::*, BindingAxis, Canvas, ImageAsset, Mesh, MeshBinding, MeshKeyform, Parameter, Part,
    Transform, Vec2,
};
use zipzi_sdk::{
    AuthoringSession, CapabilityState, ChangeCategory, ObjectKind, OrganizationCollection as C,
    OrganizationDeletePolicy as D, OrganizationEdit as E,
};
fn id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    format!(
        "00000000-0000-4000-8000-{:012x}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
fn run(s: &mut AuthoringSession, e: E) -> zipzi_sdk::EditReceipt {
    let plan = s.plan_organization_edit(e).unwrap();
    s.execute_organization_plan(&plan).unwrap()
}
fn session() -> AuthoringSession {
    AuthoringSession::new(&id(), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap()
}

#[test]
fn dissolve_keeps_parent_position_with_non_preorder_group_storage() {
    let mut s = session();
    let parent = group(&mut s, None);
    let sibling = group(&mut s, None);
    let first = group(&mut s, Some(&parent));
    let second = group(&mut s, Some(&parent));
    let mut info = s.document().display_info().clone();
    let groups = info.parameter_groups.as_mut().unwrap();
    let order = [&first, &second, &parent, &sibling];
    groups.sort_by_key(|g| order.iter().position(|id| **id == g.id).unwrap());
    s.edit("Imported group storage", None, |e| {
        e.replace_display_info(info)
    })
    .unwrap();
    run(
        &mut s,
        E::Delete {
            ids: vec![parent.clone()],
            policy: D::Dissolve,
        },
    );
    assert_eq!(
        contents(&s, C::ParameterGroups { parent_id: None }),
        vec![first.clone(), second.clone(), sibling.clone()]
    );
    s.undo().unwrap();
    assert_eq!(
        contents(&s, C::ParameterGroups { parent_id: None }),
        vec![parent, sibling.clone()]
    );
    s.redo().unwrap();
    assert_eq!(
        contents(&s, C::ParameterGroups { parent_id: None }),
        vec![first, second, sibling]
    );
}
fn group(s: &mut AuthoringSession, parent: Option<&str>) -> String {
    let g = id();
    run(
        s,
        E::CreateGroup {
            group: CdiParameterGroup {
                id: g.clone(),
                runtime_id: format!("G{g}"),
                name: "Group".into(),
                parent_id: parent.map(str::to_owned),
                extensions: Default::default(),
            },
            before: None,
        },
    );
    g
}
fn parameter(s: &mut AuthoringSession, group: Option<&str>) -> String {
    let p = id();
    run(
        s,
        E::CreateParameter {
            parameter: Parameter {
                id: p.clone(),
                runtime_id: format!("P{p}"),
                name: "Parameter".into(),
                ..Default::default()
            },
            group_id: group.map(str::to_owned),
            before: None,
        },
    );
    p
}
fn part(s: &mut AuthoringSession, parent: Option<&str>) -> String {
    let p = id();
    run(
        s,
        E::CreatePart {
            part: Part {
                id: p.clone(),
                parent_id: parent.unwrap_or("").into(),
                ..Default::default()
            },
            before: None,
        },
    );
    p
}
fn lock(s: &mut AuthoringSession, id: &str, value: bool) {
    let mut state = s.document().editor_state().clone();
    state.objects.insert(
        id.into(),
        ObjectEditorState {
            locked: value,
            ..Default::default()
        },
    );
    s.edit("Lock", None, |e| e.replace_editor_state(state))
        .unwrap();
}
fn contents(s: &AuthoringSession, c: C) -> Vec<String> {
    s.organization_collection(c)
        .unwrap()
        .into_iter()
        .map(|h| h.id().to_owned())
        .collect()
}
#[test]
fn groups_are_empty_persistent_nested_and_stable_through_history() {
    let mut s = session();
    let a = group(&mut s, None);
    let b = group(&mut s, Some(&a));
    let p = parameter(&mut s, Some(&b));
    let before = s.evaluation_revision();
    let handle = s.handle(ObjectKind::CdiParameterGroup, &b).unwrap();
    let address = s.organization_address(&b).unwrap();
    run(
        &mut s,
        E::Move {
            ids: vec![b.clone()],
            destination: C::ParameterGroups { parent_id: None },
            before: Some(a.clone()),
        },
    );
    assert_eq!(
        contents(&s, C::ParameterGroups { parent_id: None }),
        vec![b.clone(), a.clone()]
    );
    assert_eq!(s.evaluation_revision(), before);
    s.resolve_handle(&handle).unwrap();
    s.undo().unwrap();
    assert_eq!(s.document().organization_owner(&b), Some(a.as_str()));
    assert!(s
        .plan_organization_edit_at(
            &address,
            E::Delete {
                ids: vec![b.clone()],
                policy: D::Contents
            }
        )
        .is_err());
    s.redo().unwrap();
    let text = zipzi_project::encode_project(s.document()).unwrap();
    let reopened = zipzi_project::decode_project(&text).unwrap();
    assert_eq!(reopened.display_info(), s.document().display_info());
    assert_eq!(reopened.organization_owner(&p), Some(b.as_str()));
    let cdi = s.export_cdi3().unwrap();
    assert_ne!(
        serde_json::from_str::<serde_json::Value>(&cdi).unwrap()["ParameterGroups"][0]["Id"],
        b
    );
    assert!(cdi.contains(&format!("G{b}")));
}
#[test]
fn multiselect_parameter_moves_follow_source_order_and_do_not_change_preview_or_evaluation() {
    let mut s = session();
    let a = group(&mut s, None);
    let b = group(&mut s, None);
    let x = parameter(&mut s, Some(&a));
    let y = parameter(&mut s, Some(&a));
    let z = parameter(&mut s, Some(&b));
    s.set_preview_parameter(&x, 0.6).unwrap();
    let eval = s.evaluation_revision();
    let handle = s.handle(ObjectKind::Parameter, &x).unwrap();
    let address = s.organization_address(&x).unwrap();
    let r = run(
        &mut s,
        E::Move {
            ids: vec![y.clone(), x.clone()],
            destination: C::Parameters {
                group_id: Some(b.clone()),
            },
            before: Some(z.clone()),
        },
    );
    assert_eq!(
        contents(
            &s,
            C::Parameters {
                group_id: Some(b.clone())
            }
        ),
        vec![x.clone(), y.clone(), z]
    );
    assert_eq!(s.preview_values()[&x], 0.6);
    assert_eq!(s.evaluation_revision(), eval);
    assert!(r
        .changes
        .organization_references
        .iter()
        .any(|v| v.entity.id == x
            && v.before.as_deref() == Some(&a)
            && v.after.as_deref() == Some(&b)));
    s.resolve_handle(&handle).unwrap();
    s.undo().unwrap();
    assert!(s
        .plan_organization_edit_at(
            &address,
            E::Move {
                ids: vec![x.clone()],
                destination: C::Parameters { group_id: None },
                before: None
            }
        )
        .is_err());
}
#[test]
fn cycles_failed_batches_and_stale_or_foreign_plans_are_atomic() {
    let mut s = session();
    let a = group(&mut s, None);
    let b = group(&mut s, Some(&a));
    let v = s.version();
    let history = s.history_state();
    assert_eq!(
        s.plan_organization_edit(E::Move {
            ids: vec![a.clone()],
            destination: C::ParameterGroups { parent_id: Some(b) },
            before: None
        })
        .unwrap_err()
        .code
        .as_ref(),
        "CDI_GROUP_CYCLE"
    );
    assert_eq!(s.version(), v);
    assert_eq!(s.history_state(), history);
    let plan = s
        .plan_organization_edit(E::Delete {
            ids: vec![a.clone()],
            policy: D::Contents,
        })
        .unwrap();
    parameter(&mut s, None);
    assert!(s.execute_organization_plan(&plan).is_err());
    let mut other = session();
    assert!(other.execute_organization_plan(&plan).is_err());
    let v = s.version();
    assert!(s
        .edit("Invalid joint", None, |e| {
            e.edit_organization(E::CreateGroup {
                group: CdiParameterGroup {
                    id: id(),
                    runtime_id: "New".into(),
                    name: "New".into(),
                    parent_id: None,
                    extensions: Default::default(),
                },
                before: None,
            })?;
            e.edit_organization(E::Move {
                ids: vec![a.clone()],
                destination: C::ParameterGroups {
                    parent_id: Some(a.clone()),
                },
                before: None,
            })
        })
        .is_err());
    assert_eq!(s.version(), v);
}
#[test]
fn copies_remap_group_and_parameter_ids_but_keep_external_binding_users_on_originals() {
    let mut s = session();
    let a = group(&mut s, None);
    let child = group(&mut s, Some(&a));
    let p = parameter(&mut s, Some(&child));
    let plan = s
        .plan_organization_edit(E::Duplicate {
            ids: vec![a.clone(), child.clone()],
            destination: C::ParameterGroups { parent_id: None },
            before: None,
        })
        .unwrap();
    let map = plan.copied_ids().clone();
    assert_eq!(map.len(), 3);
    s.execute_organization_plan(&plan).unwrap();
    assert_eq!(
        s.document().organization_owner(&map[&child]),
        Some(map[&a].as_str())
    );
    assert_eq!(
        s.document().organization_owner(&map[&p]),
        Some(map[&child].as_str())
    );
    assert_ne!(
        s.document().get_parameter(&p).unwrap().runtime_id,
        s.document().get_parameter(&map[&p]).unwrap().runtime_id
    );
    assert!(s.document().validate_structure().is_empty());
    let h = s.handle(ObjectKind::Parameter, &map[&p]).unwrap();
    s.undo().unwrap();
    s.redo().unwrap();
    assert!(s.resolve_handle(&h).is_err());
}
#[test]
fn dissolve_promotes_direct_members_and_child_groups_without_deleting_parameters() {
    let mut s = session();
    let root = group(&mut s, None);
    let a = group(&mut s, Some(&root));
    let child = group(&mut s, Some(&a));
    let sibling = group(&mut s, Some(&root));
    let p = parameter(&mut s, Some(&a));
    let handle = s.handle(ObjectKind::Parameter, &p).unwrap();
    run(
        &mut s,
        E::Delete {
            ids: vec![a.clone()],
            policy: D::Dissolve,
        },
    );
    assert_eq!(
        contents(
            &s,
            C::ParameterGroups {
                parent_id: Some(root.clone())
            }
        ),
        vec![child.clone(), sibling]
    );
    assert_eq!(s.document().organization_owner(&p), Some(root.as_str()));
    s.resolve_handle(&handle).unwrap();
    assert!(s.document().get_parameter_group(&a).is_none());
    s.undo().unwrap();
    assert_eq!(s.document().organization_owner(&child), Some(a.as_str()));
}
#[test]
fn delete_contents_removes_owned_metadata_and_rejects_external_references() {
    let mut s = session();
    let a = group(&mut s, None);
    let p = parameter(&mut s, Some(&a));
    let expression = id();
    let entry = id();
    s.edit("User", None, |e| {
        e.edit_expression(ExpressionEdit::Create {
            id: expression.clone(),
            name: "Expression".into(),
        })?;
        e.edit_expression(ExpressionEdit::CreateEntry {
            owner: expression.clone(),
            id: entry.clone(),
            before: None,
        })?;
        e.edit_expression(ExpressionEdit::PatchEntry {
            id: entry.clone(),
            expected: ExpressionEntryFields {
                target: ExpressionTarget::Unspecified,
                value: 0.,
                blend: None,
            },
            fields: ExpressionEntryFields {
                target: ExpressionTarget::Resolved {
                    parameter_id: p.clone(),
                },
                value: 1.,
                blend: None,
            },
        })
    })
    .unwrap();
    let v = s.version();
    assert_eq!(
        s.plan_organization_edit(E::Delete {
            ids: vec![a.clone()],
            policy: D::Contents
        })
        .unwrap_err()
        .code
        .as_ref(),
        "OBJECT_REFERENCED"
    );
    assert_eq!(s.version(), v);
    s.edit("Remove external user", None, |e| {
        e.erase_object(&expression)
    })
    .unwrap();
    let handle = s.handle(ObjectKind::Parameter, &p).unwrap();
    run(
        &mut s,
        E::Delete {
            ids: vec![a.clone()],
            policy: D::Contents,
        },
    );
    assert!(s.document().get_parameter(&p).is_none());
    assert!(s.document().validate_structure().is_empty());
    s.undo().unwrap();
    assert!(s.resolve_handle(&handle).is_err());
    assert_eq!(s.document().organization_owner(&p), Some(a.as_str()));
}
#[test]
fn locks_apply_to_descendants_destinations_and_legacy_metadata_writes() {
    let mut s = session();
    let a = group(&mut s, None);
    let b = group(&mut s, None);
    let p = parameter(&mut s, Some(&a));
    lock(&mut s, &p, true);
    assert!(matches!(
        s.organization_capabilities(&s.handle(ObjectKind::CdiParameterGroup, &a).unwrap())
            .unwrap()
            .delete,
        CapabilityState::Unavailable {
            code: "OBJECT_LOCKED"
        }
    ));
    assert!(s
        .plan_organization_edit(E::Move {
            ids: vec![a.clone()],
            destination: C::ParameterGroups {
                parent_id: Some(b.clone())
            },
            before: None
        })
        .is_err());
    let mut info = s.display_info();
    if let Some(CdiParameterEntry::Resolved { group_id, .. }) = info
        .parameters
        .as_mut()
        .unwrap()
        .iter_mut()
        .find(|e| matches!(e,CdiParameterEntry::Resolved {parameter_id,..} if parameter_id==&p))
    {
        *group_id = Some(b.clone());
    }
    assert!(s
        .edit("Bypass", None, |e| e.replace_display_info(info))
        .is_err());
    lock(&mut s, &p, false);
    lock(&mut s, &b, true);
    assert!(s
        .plan_organization_edit(E::Move {
            ids: vec![p.clone()],
            destination: C::Parameters {
                group_id: Some(b.clone())
            },
            before: None
        })
        .is_err());
    assert!(s
        .organization_collection(C::Parameters {
            group_id: Some(b.clone())
        })
        .is_ok());
    assert!(s
        .edit("Bypass membership", None, |e| e
            .set_parameter_group(&p, Some(&b)))
        .is_err());
}
fn scene() -> (
    AuthoringSession,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
) {
    let mut s = session();
    let p = part(&mut s, None);
    let q = part(&mut s, None);
    let c = part(&mut s, Some(&p));
    let (asset, t, m, n) = (id(), id(), id(), id());
    s.edit("Scene", None, |e| {
        e.create_asset(ImageAsset {
            unresolved_source: None,
            id: asset.clone(),
            name: String::new(),
            content: Some(zipzi_core::ImageContent {
                source: "assets/texture.png".into(),
                sha256: "0".repeat(64),
                width: 1,
                height: 1,
            }),
        })?;
        e.create_transform(Transform {
            id: t.clone(),
            part_id: Some(p.clone().into()),
            ..Default::default()
        })?;
        for (id, owner, parent) in [(&m, &p, t.as_str()), (&n, &c, "")] {
            e.create_mesh(Mesh {
                id: id.clone(),
                texture_asset_id: (asset.clone()).into(),
                part_id: owner.clone(),
                deformer_id: parent.into(),
                draw_order: Some(0.),
                ..Default::default()
            })?;
        }
        Ok(())
    })
    .unwrap();
    (s, p, q, c, t, m, n, asset)
}
#[test]
fn part_moves_preserve_mixed_order_deformation_and_drawing_and_reject_cycles() {
    let (mut s, p, q, c, t, m, _, _) = scene();
    let before = s.preview_frame().unwrap();
    let h = s.handle(ObjectKind::Part, &p).unwrap();
    let binding_parent = s.document().get_mesh(&m).unwrap().deformer_id.clone();
    let r = run(
        &mut s,
        E::Move {
            ids: vec![p.clone(), c.clone()],
            destination: C::PartMembers {
                part_id: Some(q.clone()),
            },
            before: None,
        },
    );
    assert!(r
        .changes
        .organization_references
        .iter()
        .any(|v| v.entity.id == p));
    s.resolve_handle(&h).unwrap();
    assert_eq!(
        s.document().get_mesh(&m).unwrap().deformer_id,
        binding_parent
    );
    assert_eq!(s.document().get_transform(&t).unwrap().parent(), "");
    assert_eq!(s.document().get_part(&c).unwrap().parent_id, p);
    assert!(s.document().draw_order_groups().is_some());
    let after = s.preview_frame().unwrap();
    for (a, b) in before.drawables.iter().zip(&after.drawables) {
        assert_eq!(a.positions, b.positions);
        assert_eq!(a.render_order, b.render_order);
    }
    assert_eq!(
        s.plan_organization_edit(E::Move {
            ids: vec![q],
            destination: C::PartMembers { part_id: Some(c) },
            before: None
        })
        .unwrap_err()
        .code
        .as_ref(),
        "RELATION_CYCLE"
    );
}
#[test]
fn part_copies_remap_owned_scene_bindings_axes_samples_masks_and_internal_deformation() {
    let (mut s, p, _, c, t, m, n, asset) = scene();
    let parameter = parameter(&mut s, None);
    let binding = id();
    s.edit("Binding", None, |e| {
        let mut mesh = e.candidate_document().get_mesh(&n).unwrap().clone();
        mesh.masks = vec![m.clone()];
        e.replace_mesh(mesh)?;
        e.create_binding(MeshBinding {
            id: binding.clone(),
            mesh_id: m.clone(),
            axes: vec![BindingAxis::new(parameter.clone(), vec![-1., 1.])],
            keyforms: vec![
                MeshKeyform {
                    keys: vec![-1.],
                    ..Default::default()
                },
                MeshKeyform {
                    keys: vec![1.],
                    ..Default::default()
                },
            ],
        })
    })
    .unwrap();
    let old = s.document().get_binding(&binding).unwrap().clone();
    let plan = s
        .plan_organization_edit(E::Duplicate {
            ids: vec![p.clone()],
            destination: C::PartMembers { part_id: None },
            before: None,
        })
        .unwrap();
    let map = plan.copied_ids().clone();
    s.execute_organization_plan(&plan).unwrap();
    assert_eq!(s.document().get_part(&map[&c]).unwrap().parent_id, map[&p]);
    assert_eq!(s.document().get_mesh(&map[&m]).unwrap().part_id, map[&p]);
    assert_eq!(
        s.document().get_mesh(&map[&m]).unwrap().deformer_id,
        map[&t]
    );
    assert_eq!(
        s.document().get_mesh(&map[&m]).unwrap().texture_asset_id,
        asset
    );
    assert_eq!(
        s.document().get_mesh(&map[&n]).unwrap().masks,
        vec![map[&m].clone()]
    );
    let copied = s.document().get_binding(&map[&binding]).unwrap();
    assert_eq!(copied.axes[0].parameter_id, parameter);
    assert_ne!(copied.axes[0].id, old.axes[0].id);
    assert_eq!(
        copied.keyforms[0].samples[0],
        map[&old.keyforms[0].samples[0]]
    );
    assert!(s.document().validate_structure().is_empty());
    let bytes = zipzi_project::encode_project(s.document()).unwrap();
    let reopened = zipzi_project::decode_project(&bytes).unwrap();
    assert!(reopened.validate_structure().is_empty());
}
#[test]
fn part_delete_closure_excludes_deformer_descendants_from_another_organization() {
    let (mut s, p, q, _, t, _, n, _) = scene();
    s.edit("External deformer child", None, |e| {
        let mut m = e.candidate_document().get_mesh(&n).unwrap().clone();
        m.part_id = q.clone();
        m.deformer_id = t.clone();
        e.replace_mesh(m)
    })
    .unwrap();
    let v = s.version();
    assert_eq!(
        s.plan_organization_edit(E::Delete {
            ids: vec![p],
            policy: D::Contents
        })
        .unwrap_err()
        .code
        .as_ref(),
        "OBJECT_REFERENCED"
    );
    assert_eq!(s.version(), v);
    assert!(s.document().get_mesh(&n).is_some());
}
#[test]
fn part_dissolve_preserves_compatible_drawing_and_rejects_effectful_parts() {
    let (mut s, p, _, c, _, m, _, _) = scene();
    assert!(matches!(
        s.organization_capabilities(&s.handle(ObjectKind::Part, &p).unwrap())
            .unwrap()
            .dissolve,
        CapabilityState::Available
    ));
    let before = s.preview_frame().unwrap();
    let h = s.handle(ObjectKind::Part, &c).unwrap();
    run(
        &mut s,
        E::Delete {
            ids: vec![p.clone()],
            policy: D::Dissolve,
        },
    );
    assert_eq!(s.document().get_part(&c).unwrap().parent_id, "");
    assert_eq!(s.document().get_mesh(&m).unwrap().part_id, "");
    s.resolve_handle(&h).unwrap();
    let after = s.preview_frame().unwrap();
    for (a, b) in before.drawables.iter().zip(&after.drawables) {
        assert_eq!(a.render_order, b.render_order);
        assert_eq!(a.positions, b.positions);
    }
    s.undo().unwrap();
    let mut changed = s.document().get_part(&p).unwrap().clone();
    changed.enabled = false;
    s.edit("Disable", None, |e| e.replace_part(changed))
        .unwrap();
    assert!(matches!(
        s.organization_capabilities(&s.handle(ObjectKind::Part, &p).unwrap())
            .unwrap()
            .dissolve,
        CapabilityState::Unavailable {
            code: "PART_DISSOLVE_STATE_UNREPRESENTABLE"
        }
    ));
    let v = s.version();
    assert_eq!(
        s.plan_organization_edit(E::Delete {
            ids: vec![p],
            policy: D::Dissolve
        })
        .unwrap_err()
        .code
        .as_ref(),
        "PART_DISSOLVE_STATE_UNREPRESENTABLE"
    );
    assert_eq!(s.version(), v);
}
#[test]
fn labels_noops_and_direct_group_replacement_preserve_redo_and_retire_deleted_handles() {
    let mut s = session();
    let a = group(&mut s, None);
    let p = parameter(&mut s, Some(&a));
    s.undo().unwrap();
    let history = s.history_state();
    let version = s.version();
    let r = run(
        &mut s,
        E::Move {
            ids: vec![a.clone()],
            destination: C::ParameterGroups { parent_id: None },
            before: None,
        },
    );
    assert!(!r.changed);
    assert_eq!(s.history_state(), history);
    assert_eq!(s.version(), version);
    s.redo().unwrap();
    assert!(s.document().get_parameter(&p).is_some());
    let eval = s.evaluation_revision();
    let expected = s.document().get_parameter_group(&a).unwrap().clone();
    let mut group = expected.clone();
    group.name = "Renamed".into();
    let r = run(&mut s, E::PatchGroup { expected, group });
    assert!(r.changes.categories.contains(&ChangeCategory::Labels));
    assert!(!r
        .changes
        .categories
        .contains(&ChangeCategory::EvaluationInputs));
    assert_eq!(s.evaluation_revision(), eval);
    let handle = s.handle(ObjectKind::CdiParameterGroup, &a).unwrap();
    let original = s.display_info();
    let mut without = original.clone();
    without.parameter_groups = Some(vec![]);
    for e in without.parameters.iter_mut().flatten() {
        if let CdiParameterEntry::Resolved { group_id, .. } = e {
            *group_id = None;
        }
    }
    s.edit("Remove and restore", None, |e| {
        e.replace_display_info(without)?;
        e.replace_display_info(original)
    })
    .unwrap();
    assert!(s.resolve_handle(&handle).is_err());
}

#[test]
fn copy_destinations_and_owned_binding_children_obey_locks() {
    let mut s = session();
    let a = group(&mut s, None);
    let b = group(&mut s, None);
    let p = parameter(&mut s, Some(&a));
    lock(&mut s, &b, true);
    let before = s.version();
    for (ids, destination) in [
        (
            vec![a.clone()],
            C::ParameterGroups {
                parent_id: Some(b.clone()),
            },
        ),
        (vec![p], C::Parameters { group_id: Some(b) }),
    ] {
        assert_eq!(
            s.plan_organization_edit(E::Duplicate {
                ids,
                destination,
                before: None
            })
            .unwrap_err()
            .code
            .as_ref(),
            "OBJECT_LOCKED"
        );
    }
    assert_eq!(s.version(), before);
    let (mut s, p, q, _, _, m, _, _) = scene();
    let parameter = parameter(&mut s, None);
    let binding = id();
    s.edit("Binding", None, |e| {
        e.create_binding(MeshBinding {
            id: binding.clone(),
            mesh_id: m.clone(),
            axes: vec![BindingAxis::new(parameter, vec![-1., 1.])],
            keyforms: vec![
                MeshKeyform {
                    keys: vec![-1.],
                    ..Default::default()
                },
                MeshKeyform {
                    keys: vec![1.],
                    ..Default::default()
                },
            ],
        })
    })
    .unwrap();
    let axis = s.document().get_binding(&binding).unwrap().axes[0]
        .id
        .clone();
    lock(&mut s, &axis, true);
    assert_eq!(
        s.plan_organization_edit(E::Move {
            ids: vec![p.clone()],
            destination: C::PartMembers {
                part_id: Some(q.clone())
            },
            before: None
        })
        .unwrap_err()
        .code
        .as_ref(),
        "OBJECT_LOCKED"
    );
    lock(&mut s, &axis, false);
    // Hidden child flags must disappear together with their binding owner.
    let mut state = s.document().editor_state().clone();
    state.objects.insert(
        axis.clone(),
        ObjectEditorState {
            hidden_in_editor: true,
            locked: false,
        },
    );
    s.edit("Hide axis", None, |e| e.replace_editor_state(state))
        .unwrap();
    lock(&mut s, &q, true);
    assert_eq!(
        s.plan_organization_edit(E::Duplicate {
            ids: vec![p.clone()],
            destination: C::PartMembers {
                part_id: Some(q.clone())
            },
            before: None
        })
        .unwrap_err()
        .code
        .as_ref(),
        "OBJECT_LOCKED"
    );
    let mut mesh = s.document().get_mesh(&m).unwrap().clone();
    mesh.part_id = q;
    assert!(s
        .edit("Raw destination", None, |e| e.replace_mesh(mesh))
        .is_err());
    let handle = s.handle(ObjectKind::BindingAxis, &axis).unwrap();
    run(
        &mut s,
        E::Delete {
            ids: vec![p],
            policy: D::Contents,
        },
    );
    assert!(s.document().get_binding(&binding).is_none());
    assert!(s.document().editor_state().objects.get(&axis).is_none());
    assert!(s.document().validate_structure().is_empty());
    s.undo().unwrap();
    assert!(s.resolve_handle(&handle).is_err());
    assert!(s.document().editor_state().get(&axis).hidden_in_editor);
}

#[test]
fn dissolve_proves_static_draw_order_and_removes_dormant_bindings() {
    let (mut s, p, _, _, _, m, _, _) = scene();
    let dormant = id();
    s.edit("Static ranks and pending binding", None, |e| {
        let mut mesh = e.candidate_document().get_mesh(&m).unwrap().clone();
        mesh.draw_order = Some(-2.);
        e.replace_mesh(mesh)?;
        e.create_scene_binding(zipzi_core::SceneBinding {
            id: dormant.clone(),
            axes: vec![],
            track: zipzi_core::SceneTrack::Part {
                target_id: p.clone().into(),
                keyforms: vec![],
            },
        })
    })
    .unwrap();
    let before = s.preview_frame().unwrap();
    run(
        &mut s,
        E::Delete {
            ids: vec![p.clone()],
            policy: D::Dissolve,
        },
    );
    assert!(s.document().get_scene_binding(&dormant).is_none());
    for (a, b) in before
        .drawables
        .iter()
        .zip(s.preview_frame().unwrap().drawables.iter())
    {
        assert_eq!(a.render_order, b.render_order);
    }
    s.undo().unwrap();
    // The sibling would interleave with the promoted children, changing drawing.
    let outside = id();
    let asset = s.document().get_mesh(&m).unwrap().texture_asset_id.clone();
    s.edit("Interleave", None, |e| {
        e.create_mesh(Mesh {
            id: outside,
            texture_asset_id: asset,
            draw_order: Some(-1.),
            ..Default::default()
        })
    })
    .unwrap();
    let version = s.version();
    assert_eq!(
        s.plan_organization_edit(E::Delete {
            ids: vec![p],
            policy: D::Dissolve
        })
        .unwrap_err()
        .code
        .as_ref(),
        "PART_DISSOLVE_DRAW_ORDER_UNPROVEN"
    );
    assert_eq!(s.version(), version);
}

#[test]
fn unresolved_cdi_rows_are_preserved_on_dissolve_and_copy_conflicts_are_explicit() {
    let mut s = session();
    let a = group(&mut s, None);
    let b = group(&mut s, Some(&a));
    let mut info = s.display_info();
    info.parameters = Some(vec![CdiParameterEntry::Unresolved {
        runtime_id: "ImportedMissing".into(),
        name: "Missing".into(),
        group_id: Some(b.clone()),
        extensions: std::collections::BTreeMap::from_iter([(
            "Custom".into(),
            serde_json::json!({"value":42}),
        )]),
    }]);
    s.edit("Imported row", None, |e| e.replace_display_info(info))
        .unwrap();
    let before = s.version();
    assert_eq!(
        s.plan_organization_edit(E::Duplicate {
            ids: vec![a.clone()],
            destination: C::ParameterGroups { parent_id: None },
            before: None
        })
        .unwrap_err()
        .code
        .as_ref(),
        "UNRESOLVED_PARAMETER_COPY_CONFLICT"
    );
    assert_eq!(s.version(), before);
    run(
        &mut s,
        E::Delete {
            ids: vec![b],
            policy: D::Dissolve,
        },
    );
    let row = &s.document().display_info().parameters.as_ref().unwrap()[0];
    assert_eq!(row.group_id(), Some(a.as_str()));
    assert_eq!(row.extensions()["Custom"]["value"], 42);
}

#[test]
fn part_copy_remaps_scene_axes_and_offscreen_slots_but_shares_blend_tables() {
    use zipzi_core::{
        BlendShapeBinding, BlendShapeKeyTable, BlendShapeTargetKind, DeltaKeyforms,
        DeltaPartKeyform, Offscreen, OffscreenKeyform, PartKeyform, SceneBinding, SceneTrack,
    };
    let (mut s, p, q, _, _, m, _, asset) = scene();
    let driver = parameter(&mut s, None);
    let (ordinary, os, table, blend) = (id(), id(), id(), id());
    let blend_parameter = id();
    let samples = [id(), id()];
    s.edit("Composite subtree", None, |e| {
        e.create_parameter(Parameter {
            id: blend_parameter.clone(),
            kind: zipzi_core::ParameterKind::BlendShape,
            ..Default::default()
        })?;
        let mut groups = zipzi_core::draw_order::resolved_groups(e.candidate_document());
        groups[0].items.retain(|item| item != &q);
        groups
            .iter_mut()
            .find(|group| group.owner == p)
            .unwrap()
            .items
            .push(q.clone());
        e.replace_draw_order_groups(groups)?;
        e.create_scene_binding(SceneBinding {
            id: ordinary.clone(),
            axes: vec![BindingAxis::new(driver.clone(), vec![-1., 1.])],
            track: SceneTrack::Part {
                target_id: p.clone().into(),
                keyforms: vec![
                    PartKeyform {
                        keys: vec![-1.],
                        draw_order: 0.,
                        ..Default::default()
                    },
                    PartKeyform {
                        keys: vec![1.],
                        draw_order: 2.,
                        ..Default::default()
                    },
                ],
            },
        })?;
        e.create_offscreen(Offscreen {
            id: os.clone(),
            runtime_id: "Layer".into(),
            name: "Layer".into(),
            part_id: p.clone(),
            enabled: true,
            blend_mode: 0,
            flags: 0,
            masks: vec![m.clone()],
            part_keyform_indices: vec![0, 1],
            keyforms: vec![
                OffscreenKeyform {
                    opacity: 0.5,
                    ..Default::default()
                },
                OffscreenKeyform {
                    opacity: 1.,
                    ..Default::default()
                },
            ],
        })?;
        e.create_blend_key_table(BlendShapeKeyTable {
            id: table.clone(),
            parameter_id: blend_parameter.clone(),
            samples: vec![
                zipzi_core::BindingSample {
                    id: samples[0].clone(),
                    value: -1.,
                },
                zipzi_core::BindingSample {
                    id: samples[1].clone(),
                    value: 1.,
                },
            ],
            base_sample_id: samples[0].clone(),
            keys: vec![],
            base_key_idx: 0,
        })?;
        e.create_blend_binding(BlendShapeBinding {
            id: blend.clone(),
            target_id: p.clone(),
            target_kind: BlendShapeTargetKind::Part,
            key_table_id: table.clone(),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Part(vec![
                DeltaPartKeyform {
                    sample_id: samples[0].clone(),
                    draw_order: 0.,
                },
                DeltaPartKeyform {
                    sample_id: samples[1].clone(),
                    draw_order: 1.,
                },
            ]),
        })
    })
    .unwrap();
    let original = s.document().get_scene_binding(&ordinary).unwrap().clone();
    let plan = s
        .plan_organization_edit(E::Duplicate {
            ids: vec![p.clone()],
            destination: C::PartMembers { part_id: None },
            before: None,
        })
        .unwrap();
    assert_eq!(plan.effects().external_drawing_members, vec![q.clone()]);
    let map = plan.copied_ids().clone();
    s.execute_organization_plan(&plan).unwrap();
    let copied = s.document().get_scene_binding(&map[&ordinary]).unwrap();
    assert_eq!(copied.target_id(), map[&p]);
    assert_eq!(copied.axes[0].parameter_id, driver);
    assert_eq!(copied.axes[0].id, map[&original.axes[0].id]);
    let SceneTrack::Part { keyforms, .. } = &copied.track else {
        panic!()
    };
    let SceneTrack::Part { keyforms: old, .. } = &original.track else {
        panic!()
    };
    assert_eq!(keyforms[1].samples[0], map[&old[1].samples[0]]);
    let layer = s.document().get_offscreen(&map[&os]).unwrap();
    assert_eq!(layer.part_id, map[&p]);
    assert_eq!(layer.masks, vec![map[&m].clone()]);
    assert_eq!(layer.part_keyform_indices, vec![0, 1]);
    assert_eq!(
        s.document()
            .get_blend_binding(&map[&blend])
            .unwrap()
            .key_table_id,
        table
    );
    assert!(!map.contains_key(&table));
    assert!(!map.contains_key(&samples[0]));
    assert_eq!(
        s.document()
            .get_blend_binding(&map[&blend])
            .unwrap()
            .keyforms,
        s.document().get_blend_binding(&blend).unwrap().keyforms
    );
    assert!(!s
        .document()
        .draw_order_groups()
        .unwrap()
        .iter()
        .find(|g| g.owner == map[&p])
        .unwrap()
        .items
        .contains(&q));
    let copy = map[&p].clone();
    run(
        &mut s,
        E::Delete {
            ids: vec![copy],
            policy: D::Contents,
        },
    );
    assert!(s.document().get_asset(&asset).is_some());
    assert!(s.document().get_blend_key_table(&table).is_some());
    assert!(s.document().get_scene_binding(&map[&ordinary]).is_none());
    let error = s
        .plan_organization_edit(E::Delete {
            ids: vec![p],
            policy: D::Contents,
        })
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "OBJECT_REFERENCED");
    assert!(error.referrers.contains(&q));
    assert!(s.document().validate_structure().is_empty());
}

#[test]
fn legacy_palette_replacement_cannot_reorder_locked_owned_contents() {
    let (mut s, p, q, _, _, _, _, _) = scene();
    lock(&mut s, &p, true);
    let before = s.version();
    let mut order = s.document().hierarchy_order().clone();
    order.organization = vec![q, p];
    assert_eq!(
        s.edit("Raw palette", None, |e| e.replace_hierarchy_order(order))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), before);
    let mut s = session();
    let a = group(&mut s, None);
    let _b = group(&mut s, None);
    let p = parameter(&mut s, Some(&a));
    lock(&mut s, &p, true);
    let mut info = s.display_info();
    info.parameter_groups.as_mut().unwrap().reverse();
    assert_eq!(
        s.edit("Raw group order", None, |e| e.replace_display_info(info))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
}

#[test]
fn dissolving_multiple_parts_preserves_each_promoted_sibling_position() {
    let (mut s, p, q, c, t, m, n, _) = scene();
    let child = part(&mut s, Some(&q));
    run(
        &mut s,
        E::Delete {
            ids: vec![q, p],
            policy: D::Dissolve,
        },
    );
    let root = contents(&s, C::PartMembers { part_id: None });
    for id in [&c, &t, &m, &n, &child] {
        assert!(root.contains(id) || s.document().organization_owner(id).is_some());
    }
    assert!(
        root.iter().position(|id| id == &m).unwrap()
            < root.iter().position(|id| id == &child).unwrap()
    );
    assert!(s.document().validate_structure().is_empty());
}

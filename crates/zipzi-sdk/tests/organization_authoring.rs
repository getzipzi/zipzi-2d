use zipzi_contract::authoring::CapabilityState;
use zipzi_contract::authoring::ObjectKind;
use zipzi_core::state::RootDomain;
use zipzi_core::BindingAxisEditing;
use zipzi_core::{
    document::*, BindingAxis, Canvas, Mesh, MeshBinding, MeshKeyform, Parameter, Part, Transform,
    Vec2,
};
use zipzi_sdk::{
    AuthoringSession, OrganizationCollection as C, OrganizationDeletePolicy as D,
    OrganizationEdit as E,
};
fn id() -> String {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    format!(
        "00000000-0000-4000-8000-{:012x}",
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}
fn run(s: &mut AuthoringSession, e: E) -> zipzi_sdk::Publication {
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_organization(e)
    })
    .unwrap();
    s.publish(plan.1).unwrap()
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
fn dissolve_keeps_parent_position_with_non_preorder_group_storage() {
    let mut s = session();
    let parent = group(&mut s, None);
    let sibling = group(&mut s, None);
    let first = group(&mut s, Some(&parent));
    let second = group(&mut s, Some(&parent));
    let mut info = s.snapshot().document().display_info().clone();
    let groups = info.parameter_groups.as_mut().unwrap();
    let order = [&first, &second, &parent, &sibling];
    groups.sort_by_key(|g| order.iter().position(|id| **id == g.id).unwrap());
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_display_info(info))
            .and_then(|value| {
                let prepared = draft.prepare("Imported group storage")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
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
    s.undo(s.version()).unwrap();
    assert_eq!(
        contents(&s, C::ParameterGroups { parent_id: None }),
        vec![parent, sibling.clone()]
    );
    s.redo(s.version()).unwrap();
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
                runtime_id: p.clone(),
                parent_id: parent.unwrap_or("").into(),
                ..Default::default()
            },
            before: None,
        },
    );
    p
}
fn lock(s: &mut AuthoringSession, id: &str, value: bool) {
    let mut state = s.snapshot().document().editor_state().clone();
    state.objects.insert(
        id.into(),
        ObjectEditorState {
            locked: value,
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
fn contents(s: &AuthoringSession, c: C) -> Vec<String> {
    s.snapshot()
        .document()
        .organization_collection(&c)
        .unwrap()
        .into_iter()
        .collect()
}
#[test]
fn groups_are_empty_persistent_nested_and_stable_through_history() {
    let mut s = session();
    let a = group(&mut s, None);
    let b = group(&mut s, Some(&a));
    let p = parameter(&mut s, Some(&b));
    let before = s.snapshot().content_id();
    let handle = {
        let kind = ObjectKind::CdiParameterGroup;
        let object_id = (&b).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let mut prior = s.begin_draft();
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
    assert_ne!(s.snapshot().content_id(), before);
    s.snapshot()
        .document()
        .require_object(handle.0, &handle.1)
        .unwrap();
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.snapshot().document().organization_owner(&b),
        Some(a.as_str())
    );
    assert!(prior
        .batch(prior.source(), |c| c.edit_organization(E::Delete {
            ids: vec![b.clone()],
            policy: D::Contents
        }))
        .and_then(|_| prior.prepare("old organization"))
        .and_then(|change| s.publish(change))
        .is_err());
    s.redo(s.version()).unwrap();
    let reopened = crate::support::roundtrip(s.snapshot()).document().clone();
    assert_eq!(
        reopened.display_info(),
        s.snapshot().document().display_info()
    );
    assert_eq!(reopened.organization_owner(&p), Some(b.as_str()));
    let cdi = zipzi_project::export_cdi3(s.snapshot().document()).unwrap();
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
    let values = zipzi_core::PreviewValues::from([(x.clone(), 0.6)]);
    let frame = crate::support::frame(s.snapshot(), &values).unwrap();
    let eval = s.snapshot().content_id();
    let handle = {
        let kind = ObjectKind::Parameter;
        let object_id = (&x).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let mut prior = s.begin_draft();
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
    assert_eq!(
        crate::support::frame(s.snapshot(), &values)
            .unwrap()
            .drawables,
        frame.drawables
    );
    assert_ne!(s.snapshot().content_id(), eval);
    assert!(r.writes.root_changed(RootDomain::DisplayInfo));
    assert_eq!(
        s.snapshot().document().parameter_group_for(&x),
        Some(b.as_str())
    );
    s.snapshot()
        .document()
        .require_object(handle.0, &handle.1)
        .unwrap();
    s.undo(s.version()).unwrap();
    assert!(prior
        .batch(prior.source(), |c| c.edit_organization(E::Move {
            ids: vec![x.clone()],
            destination: C::Parameters { group_id: None },
            before: None
        }))
        .and_then(|_| prior.prepare("old organization"))
        .and_then(|change| s.publish(change))
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
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_organization(E::Move {
                ids: vec![a.clone()],
                destination: C::ParameterGroups { parent_id: Some(b) },
                before: None
            }))
        .unwrap_err()
        .code
        .as_str(),
        "CDI_GROUP_CYCLE"
    );
    assert_eq!(s.version(), v);
    assert_eq!(s.history_state(), history);
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_organization(E::Delete {
            ids: vec![a.clone()],
            policy: D::Contents,
        })
    })
    .unwrap();
    parameter(&mut s, None);
    assert!(s.publish(plan.1).is_err());
    let mut other = session();
    let foreign = s.begin_draft().prepare("foreign").unwrap();
    assert!(other.publish(foreign).is_err());
    let v = s.version();
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
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
            .and_then(|value| {
                let prepared = draft.prepare("Invalid joint")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
    assert_eq!(s.version(), v);
}
#[test]
fn copies_remap_group_and_parameter_ids_but_keep_external_binding_users_on_originals() {
    let mut s = session();
    let a = group(&mut s, None);
    let child = group(&mut s, Some(&a));
    let p = parameter(&mut s, Some(&child));
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_organization(E::Duplicate {
            ids: vec![a.clone(), child.clone()],
            destination: C::ParameterGroups { parent_id: None },
            before: None,
        })
    })
    .unwrap();
    let map = plan.0.copied_ids.clone();
    assert_eq!(map.len(), 3);
    s.publish(plan.1).unwrap();
    assert_eq!(
        s.snapshot().document().organization_owner(&map[&child]),
        Some(map[&a].as_str())
    );
    assert_eq!(
        s.snapshot().document().organization_owner(&map[&p]),
        Some(map[&child].as_str())
    );
    assert_ne!(
        s.snapshot().document().parameter(&p).unwrap().runtime_id,
        s.snapshot()
            .document()
            .parameter(&map[&p])
            .unwrap()
            .runtime_id
    );
    assert!(s.snapshot().document().builder().freeze().is_ok());
    let h = {
        let kind = ObjectKind::Parameter;
        let object_id = (&map[&p]).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    s.undo(s.version()).unwrap();
    s.redo(s.version()).unwrap();
    assert!((s.read_snapshot().source != h.2));
}
#[test]
fn dissolve_promotes_direct_members_and_child_groups_without_deleting_parameters() {
    let mut s = session();
    let root = group(&mut s, None);
    let a = group(&mut s, Some(&root));
    let child = group(&mut s, Some(&a));
    let sibling = group(&mut s, Some(&root));
    let p = parameter(&mut s, Some(&a));
    let handle = {
        let kind = ObjectKind::Parameter;
        let object_id = (&p).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
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
    assert_eq!(
        s.snapshot().document().organization_owner(&p),
        Some(root.as_str())
    );
    s.snapshot()
        .document()
        .require_object(handle.0, &handle.1)
        .unwrap();
    assert!(s.snapshot().document().parameter_group(&a).is_none());
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.snapshot().document().organization_owner(&child),
        Some(a.as_str())
    );
}
#[test]
fn delete_contents_removes_owned_metadata_and_rejects_external_references() {
    let mut s = session();
    let a = group(&mut s, None);
    let p = parameter(&mut s, Some(&a));
    let expression = id();
    let entry = id();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
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
            .and_then(|value| {
                let prepared = draft.prepare("User")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let v = s.version();
    assert_eq!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_organization(E::Delete {
                ids: vec![a.clone()],
                policy: D::Contents
            }))
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_REFERENCED"
    );
    assert_eq!(s.version(), v);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_expression(ExpressionEdit::Delete {
                    id: expression.clone(),
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Remove external user")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let handle = {
        let kind = ObjectKind::Parameter;
        let object_id = (&p).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    run(
        &mut s,
        E::Delete {
            ids: vec![a.clone()],
            policy: D::Contents,
        },
    );
    assert!(s.snapshot().document().parameter(&p).is_none());
    assert!(s.snapshot().document().builder().freeze().is_ok());
    s.undo(s.version()).unwrap();
    assert!((s.read_snapshot().source != handle.2));
    assert_eq!(
        s.snapshot().document().organization_owner(&p),
        Some(a.as_str())
    );
}
#[test]
fn locks_apply_to_descendants_destinations_and_legacy_metadata_writes() {
    let mut s = session();
    let a = group(&mut s, None);
    let b = group(&mut s, None);
    let p = parameter(&mut s, Some(&a));
    lock(&mut s, &p, true);
    assert!(matches!(
        {
            let address = {
                let kind = ObjectKind::CdiParameterGroup;
                let object_id = (&a).to_owned();
                s.snapshot()
                    .document()
                    .require_object(kind, &object_id)
                    .map(|()| (kind, object_id, s.read_snapshot().source))
            }
            .unwrap();
            s.snapshot()
                .document()
                .organization_capabilities(address.0, &address.1)
        }
        .unwrap()
        .delete,
        CapabilityState::Unavailable {
            code: "OBJECT_LOCKED"
        }
    ));
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_organization(E::Move {
                ids: vec![a.clone()],
                destination: C::ParameterGroups {
                    parent_id: Some(b.clone())
                },
                before: None
            }))
        .is_err()
    );
    let mut info = s.snapshot().document().display_info().clone();
    if let Some(CdiParameterEntry::Resolved { group_id, .. }) = info
        .parameters
        .as_mut()
        .unwrap()
        .iter_mut()
        .find(|e| matches!(e,CdiParameterEntry::Resolved {parameter_id,..} if parameter_id==&p))
    {
        *group_id = Some(b.clone());
    }
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_display_info(info))
            .and_then(|value| {
                let prepared = draft.prepare("Bypass")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
    lock(&mut s, &p, false);
    lock(&mut s, &b, true);
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_organization(E::Move {
                ids: vec![p.clone()],
                destination: C::Parameters {
                    group_id: Some(b.clone())
                },
                before: None
            }))
        .is_err()
    );
    assert!(s
        .snapshot()
        .document()
        .organization_collection(&C::Parameters {
            group_id: Some(b.clone())
        })
        .is_ok());
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_organization(E::SetParameterGroup {
                    parameter_id: p.clone(),
                    group_id: Some(b.clone()),
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Bypass membership")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
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
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_empty_asset(asset.clone(), String::new())?;
                e.replace_image(
                    &asset,
                    zipzi_project::ResourceStore::default().prepare_rgba(
                        1,
                        1,
                        std::sync::Arc::from([255u8; 4]),
                    )?,
                )?;
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: t.clone(),
                        part_id: Some(p.clone().into()),
                        ..Default::default()
                    },
                )?;
                for (id, owner, parent) in [(&m, &p, t.as_str()), (&n, &c, "")] {
                    e.create_mesh({
                        let mut value = Mesh {
                            id: id.clone(),
                            texture_asset_id: (asset.clone()).into(),
                            part_id: owner.clone(),
                            deformer_id: parent.into(),
                            draw_order: Some(0.),
                            ..Default::default()
                        };
                        value.runtime_id = value.id.clone();
                        value
                    })?;
                }
                Ok(())
            })
            .and_then(|value| {
                let prepared = draft.prepare("Scene")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    (s, p, q, c, t, m, n, asset)
}
#[test]
fn part_moves_preserve_mixed_order_deformation_and_drawing_and_reject_cycles() {
    let (mut s, p, q, c, t, m, _, _) = scene();
    let before = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    let h = {
        let kind = ObjectKind::Part;
        let object_id = (&p).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let binding_parent = s
        .snapshot()
        .document()
        .mesh(&m)
        .unwrap()
        .deformer_id
        .clone();
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
    assert!(!r.writes.is_empty());
    assert_eq!(s.snapshot().document().part(&p).unwrap().parent_id, q);
    s.snapshot().document().require_object(h.0, &h.1).unwrap();
    assert_eq!(
        s.snapshot().document().mesh(&m).unwrap().deformer_id,
        binding_parent
    );
    assert_eq!(s.snapshot().document().transform(&t).unwrap().parent(), "");
    assert_eq!(s.snapshot().document().part(&c).unwrap().parent_id, p);
    assert!(s.snapshot().document().draw_order_groups().is_some());
    let after = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    for (a, b) in before.drawables.iter().zip(&after.drawables) {
        assert_eq!(a.positions, b.positions);
        assert_eq!(a.render_order, b.render_order);
    }
    assert_eq!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_organization(E::Move {
                ids: vec![q],
                destination: C::PartMembers { part_id: Some(c) },
                before: None
            }))
        .unwrap_err()
        .code
        .as_str(),
        "RELATION_CYCLE"
    );
}
#[test]
fn part_copies_remap_owned_scene_bindings_axes_samples_masks_and_internal_deformation() {
    let (mut s, p, _, c, t, m, n, asset) = scene();
    let parameter = parameter(&mut s, None);
    let binding = id();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut mesh = e.read_document().unwrap().mesh(&n).unwrap().clone();
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
            .and_then(|value| {
                let prepared = draft.prepare("Binding")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let old = s.snapshot().document().binding(&binding).unwrap().clone();
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_organization(E::Duplicate {
            ids: vec![p.clone()],
            destination: C::PartMembers { part_id: None },
            before: None,
        })
    })
    .unwrap();
    let map = plan.0.copied_ids.clone();
    s.publish(plan.1).unwrap();
    assert_eq!(
        s.snapshot().document().part(&map[&c]).unwrap().parent_id,
        map[&p]
    );
    assert_eq!(
        s.snapshot().document().mesh(&map[&m]).unwrap().part_id,
        map[&p]
    );
    assert_eq!(
        s.snapshot().document().mesh(&map[&m]).unwrap().deformer_id,
        map[&t]
    );
    assert_eq!(
        s.snapshot()
            .document()
            .mesh(&map[&m])
            .unwrap()
            .texture_asset_id,
        asset
    );
    assert_eq!(
        s.snapshot().document().mesh(&map[&n]).unwrap().masks,
        vec![map[&m].clone()]
    );
    let copied = s.snapshot().document().binding(&map[&binding]).unwrap();
    assert_eq!(copied.axes[0].parameter_id, parameter);
    assert_ne!(copied.axes[0].id, old.axes[0].id);
    assert_eq!(
        copied.keyforms[0].samples[0],
        map[&old.keyforms[0].samples[0]]
    );
    assert!(s.snapshot().document().builder().freeze().is_ok());
    let reopened = crate::support::roundtrip(s.snapshot()).document().clone();
    assert!(reopened.builder().freeze().is_ok());
}
#[test]
fn part_delete_closure_excludes_deformer_descendants_from_another_organization() {
    let (mut s, p, q, _, t, _, n, _) = scene();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut m = e.read_document().unwrap().mesh(&n).unwrap().clone();
                m.part_id = q.clone();
                m.deformer_id = t.clone();
                e.replace_mesh(m)
            })
            .and_then(|value| {
                let prepared = draft.prepare("External deformer child")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let v = s.version();
    assert_eq!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_organization(E::Delete {
                ids: vec![p],
                policy: D::Contents
            }))
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_REFERENCED"
    );
    assert_eq!(s.version(), v);
    assert!(s.snapshot().document().mesh(&n).is_some());
}
#[test]
fn part_dissolve_preserves_compatible_drawing_and_rejects_effectful_parts() {
    let (mut s, p, _, c, _, m, _, _) = scene();
    assert!(matches!(
        {
            let address = {
                let kind = ObjectKind::Part;
                let object_id = (&p).to_owned();
                s.snapshot()
                    .document()
                    .require_object(kind, &object_id)
                    .map(|()| (kind, object_id, s.read_snapshot().source))
            }
            .unwrap();
            s.snapshot()
                .document()
                .organization_capabilities(address.0, &address.1)
        }
        .unwrap()
        .dissolve,
        CapabilityState::Available
    ));
    let before = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    let h = {
        let kind = ObjectKind::Part;
        let object_id = (&c).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    run(
        &mut s,
        E::Delete {
            ids: vec![p.clone()],
            policy: D::Dissolve,
        },
    );
    assert_eq!(s.snapshot().document().part(&c).unwrap().parent_id, "");
    assert_eq!(s.snapshot().document().mesh(&m).unwrap().part_id, "");
    s.snapshot().document().require_object(h.0, &h.1).unwrap();
    let after = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    for (a, b) in before.drawables.iter().zip(&after.drawables) {
        assert_eq!(a.render_order, b.render_order);
        assert_eq!(a.positions, b.positions);
    }
    s.undo(s.version()).unwrap();
    let mut changed = s.snapshot().document().part(&p).unwrap().clone();
    changed.enabled = false;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_part(changed))
            .and_then(|value| {
                let prepared = draft.prepare("Disable")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(matches!(
        {
            let address = {
                let kind = ObjectKind::Part;
                let object_id = (&p).to_owned();
                s.snapshot()
                    .document()
                    .require_object(kind, &object_id)
                    .map(|()| (kind, object_id, s.read_snapshot().source))
            }
            .unwrap();
            s.snapshot()
                .document()
                .organization_capabilities(address.0, &address.1)
        }
        .unwrap()
        .dissolve,
        CapabilityState::Unavailable {
            code: "PART_DISSOLVE_STATE_UNREPRESENTABLE"
        }
    ));
    let v = s.version();
    assert_eq!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_organization(E::Delete {
                ids: vec![p],
                policy: D::Dissolve
            }))
        .unwrap_err()
        .code
        .as_str(),
        "PART_DISSOLVE_STATE_UNREPRESENTABLE"
    );
    assert_eq!(s.version(), v);
}
#[test]
fn labels_noops_and_direct_group_replacement_preserve_redo_and_retire_deleted_handles() {
    let mut s = session();
    let a = group(&mut s, None);
    let p = parameter(&mut s, Some(&a));
    s.undo(s.version()).unwrap();
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
    assert!(r.writes.is_empty());
    assert_eq!(s.history_state(), history);
    assert_eq!(s.version(), version);
    s.redo(s.version()).unwrap();
    assert!(s.snapshot().document().parameter(&p).is_some());
    let eval = s.snapshot().content_id();
    let expected = s.snapshot().document().parameter_group(&a).unwrap().clone();
    let mut group = expected.clone();
    group.name = "Renamed".into();
    let r = run(&mut s, E::PatchGroup { expected, group });
    assert!(r.writes.root_changed(RootDomain::DisplayInfo));
    assert_eq!(
        s.snapshot().document().parameter_group(&a).unwrap().name,
        "Renamed"
    );
    assert_ne!(s.snapshot().content_id(), eval);
    let handle = {
        let kind = ObjectKind::CdiParameterGroup;
        let object_id = (&a).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let original = s.snapshot().document().display_info().clone();
    let mut without = original.clone();
    without.parameter_groups = Some(vec![]);
    for e in without.parameters.iter_mut().flatten() {
        if let CdiParameterEntry::Resolved { group_id, .. } = e {
            *group_id = None;
        }
    }
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.replace_display_info(without)?;
                e.replace_display_info(original)
            })
            .and_then(|value| {
                let prepared = draft.prepare("Remove and restore")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(s.read_snapshot().source, handle.2);
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
            crate::support::prepared(&s, |candidate_commands| candidate_commands
                .edit_organization(E::Duplicate {
                    ids,
                    destination,
                    before: None
                }))
            .unwrap_err()
            .code
            .as_str(),
            "OBJECT_LOCKED"
        );
    }
    assert_eq!(s.version(), before);
    let (mut s, p, q, _, _, m, _, _) = scene();
    let parameter = parameter(&mut s, None);
    let binding = id();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
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
            .and_then(|value| {
                let prepared = draft.prepare("Binding")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let axis = s.snapshot().document().binding(&binding).unwrap().axes[0]
        .id
        .clone();
    lock(&mut s, &axis, true);
    assert_eq!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_organization(E::Move {
                ids: vec![p.clone()],
                destination: C::PartMembers {
                    part_id: Some(q.clone())
                },
                before: None
            }))
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    lock(&mut s, &axis, false);
    // Hidden child flags must disappear together with their binding owner.
    let mut state = s.snapshot().document().editor_state().clone();
    state.objects.insert(
        axis.clone(),
        ObjectEditorState {
            hidden_in_editor: true,
            locked: false,
        },
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state))
            .and_then(|value| {
                let prepared = draft.prepare("Hide axis")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    lock(&mut s, &q, true);
    assert_eq!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_organization(E::Duplicate {
                ids: vec![p.clone()],
                destination: C::PartMembers {
                    part_id: Some(q.clone())
                },
                before: None
            }))
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    let mut mesh = s.snapshot().document().mesh(&m).unwrap().clone();
    mesh.part_id = q;
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_mesh(mesh))
            .and_then(|value| {
                let prepared = draft.prepare("Raw destination")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
    let handle = {
        let kind = ObjectKind::BindingAxis;
        let object_id = (&axis).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    run(
        &mut s,
        E::Delete {
            ids: vec![p],
            policy: D::Contents,
        },
    );
    assert!(s.snapshot().document().binding(&binding).is_none());
    assert!(s
        .snapshot()
        .document()
        .editor_state()
        .objects
        .get(&axis)
        .is_none());
    assert!(s.snapshot().document().builder().freeze().is_ok());
    s.undo(s.version()).unwrap();
    assert!((s.read_snapshot().source != handle.2));
    assert!(
        s.snapshot()
            .document()
            .editor_state()
            .get(&axis)
            .hidden_in_editor
    );
}

#[test]
fn dissolve_proves_static_draw_order_and_removes_dormant_bindings() {
    let (mut s, p, _, _, _, m, _, _) = scene();
    let dormant = id();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut mesh = e.read_document().unwrap().mesh(&m).unwrap().clone();
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
            .and_then(|value| {
                let prepared = draft.prepare("Static ranks and pending binding")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    run(
        &mut s,
        E::Delete {
            ids: vec![p.clone()],
            policy: D::Dissolve,
        },
    );
    assert!(s.snapshot().document().scene_binding(&dormant).is_none());
    for (a, b) in before.drawables.iter().zip(
        crate::support::frame(s.snapshot(), &Default::default())
            .unwrap()
            .drawables
            .iter(),
    ) {
        assert_eq!(a.render_order, b.render_order);
    }
    s.undo(s.version()).unwrap();
    // The sibling would interleave with the promoted children, changing drawing.
    let outside = id();
    let asset = s
        .snapshot()
        .document()
        .mesh(&m)
        .unwrap()
        .texture_asset_id
        .clone();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_mesh({
                    let mut value = Mesh {
                        id: outside,
                        texture_asset_id: asset,
                        draw_order: Some(-1.),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Interleave")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let version = s.version();
    assert_eq!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_organization(E::Delete {
                ids: vec![p],
                policy: D::Dissolve
            }))
        .unwrap_err()
        .code
        .as_str(),
        "PART_DISSOLVE_DRAW_ORDER_UNPROVEN"
    );
    assert_eq!(s.version(), version);
}

#[test]
fn unresolved_cdi_rows_are_preserved_on_dissolve_and_copy_conflicts_are_explicit() {
    let mut s = session();
    let a = group(&mut s, None);
    let b = group(&mut s, Some(&a));
    let mut info = s.snapshot().document().display_info().clone();
    info.parameters = Some(vec![CdiParameterEntry::Unresolved {
        runtime_id: "ImportedMissing".into(),
        name: "Missing".into(),
        group_id: Some(b.clone()),
        extensions: std::collections::BTreeMap::from_iter([(
            "Custom".into(),
            serde_json::json!({"value":42}),
        )]),
    }]);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_display_info(info))
            .and_then(|value| {
                let prepared = draft.prepare("Imported row")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.version();
    assert_eq!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_organization(E::Duplicate {
                ids: vec![a.clone()],
                destination: C::ParameterGroups { parent_id: None },
                before: None
            }))
        .unwrap_err()
        .code
        .as_str(),
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
    let row = &s
        .snapshot()
        .document()
        .display_info()
        .parameters
        .as_ref()
        .unwrap()[0];
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
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: blend_parameter.clone(),
                        kind: zipzi_core::ParameterKind::BlendShape,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.edit_scene(
                    SceneEdit::Move {
                        ids: vec![q.clone()],
                        destination: SceneCollection::Drawing { owner: p.clone() },
                        before: None,
                    },
                    Default::default(),
                    &Default::default(),
                )?;
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
                crate::support::insert_offscreen(
                    e,
                    Offscreen {
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
                    },
                )?;
                e.edit_blend_shape(
                    zipzi_sdk::BlendAuthoringEdit::CreateTable {
                        table: BlendShapeKeyTable {
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
                        },
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())?;
                {
                    let binding = BlendShapeBinding {
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
                    };
                    e.edit_blend_shape(
                        zipzi_sdk::BlendAuthoringEdit::CreateBinding {
                            id: binding.id.clone(),
                            target_id: binding.target_id.clone(),
                            target_kind: binding.target_kind,
                            key_table_id: binding.key_table_id.clone(),
                        },
                        Default::default(),
                        &Default::default(),
                    )?;
                    e.replace_blend_binding(binding)
                }
            })
            .and_then(|value| {
                let prepared = draft.prepare("Composite subtree")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let original = s
        .snapshot()
        .document()
        .scene_binding(&ordinary)
        .unwrap()
        .clone();
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_organization(E::Duplicate {
            ids: vec![p.clone()],
            destination: C::PartMembers { part_id: None },
            before: None,
        })
    })
    .unwrap();
    assert_eq!(plan.0.effects.external_drawing_members, vec![q.clone()]);
    let map = plan.0.copied_ids.clone();
    s.publish(plan.1).unwrap();
    let copied = s
        .snapshot()
        .document()
        .scene_binding(&map[&ordinary])
        .unwrap();
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
    let layer = s.snapshot().document().offscreen(&map[&os]).unwrap();
    assert_eq!(layer.part_id, map[&p]);
    assert_eq!(layer.masks, vec![map[&m].clone()]);
    assert_eq!(layer.part_keyform_indices, vec![0, 1]);
    assert_eq!(
        s.snapshot()
            .document()
            .blend_binding(&map[&blend])
            .unwrap()
            .key_table_id,
        table
    );
    assert!(!map.contains_key(&table));
    assert!(!map.contains_key(&samples[0]));
    assert_eq!(
        s.snapshot()
            .document()
            .blend_binding(&map[&blend])
            .unwrap()
            .keyforms,
        s.snapshot()
            .document()
            .blend_binding(&blend)
            .unwrap()
            .keyforms
    );
    assert!(!s
        .snapshot()
        .document()
        .draw_order_groups()
        .as_ref()
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
    assert!(s.snapshot().document().asset(&asset).is_some());
    assert!(s.snapshot().document().blend_key_table(&table).is_some());
    assert!(s
        .snapshot()
        .document()
        .scene_binding(&map[&ordinary])
        .is_none());
    let error = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_organization(E::Delete {
            ids: vec![p],
            policy: D::Contents,
        })
    })
    .unwrap_err();
    assert_eq!(error.code.as_str(), "OBJECT_REFERENCED");
    assert!(s
        .snapshot()
        .document()
        .draw_order_groups()
        .as_ref()
        .unwrap()
        .iter()
        .any(|group| group.items.contains(&q)));
    assert!(s.snapshot().document().builder().freeze().is_ok());
}

#[test]
fn legacy_palette_replacement_cannot_reorder_locked_owned_contents() {
    let (mut s, p, q, _, _, _, _, _) = scene();
    lock(&mut s, &p, true);
    let before = s.version();
    let move_ids = vec![q];
    let before_id = p;
    assert_eq!(
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    e.edit_organization(E::Move {
                        ids: move_ids,
                        destination: C::PartMembers { part_id: None },
                        before: Some(before_id),
                    })
                })
                .and_then(|value| {
                    let prepared = draft.prepare("Raw palette")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), before);
    let mut s = session();
    let a = group(&mut s, None);
    let _b = group(&mut s, None);
    let p = parameter(&mut s, Some(&a));
    lock(&mut s, &p, true);
    let mut info = s.snapshot().document().display_info().clone();
    info.parameter_groups.as_mut().unwrap().reverse();
    assert_eq!(
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_display_info(info))
                .and_then(|value| {
                    let prepared = draft.prepare("Raw group order")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap_err()
        .code
        .as_str(),
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
        assert!(root.contains(id) || s.snapshot().document().organization_owner(id).is_some());
    }
    assert!(
        root.iter().position(|id| id == &m).unwrap()
            < root.iter().position(|id| id == &child).unwrap()
    );
    assert!(s.snapshot().document().builder().freeze().is_ok());
}

use super::{document_fixture, setup_with_document};
use crate::authoring::{ActionAvailability, AuthoringContext, AuthoringPresentation};
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};
use zipzi_editor_core::{AuthoringAction as A, CommandId, Document, ObjectKind as K, ObjectRef};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[gpui_kit::test]
fn locating_a_later_registration_expands_the_whole_group_before_rendering(cx: &mut TestAppContext) {
    let (d, assets) = document_fixture::animation_document().unwrap();
    let entry = ObjectRef::new(
        K::MotionRegistration,
        d.animation_catalog().motions[1]
            .selection
            .entry
            .as_ref()
            .unwrap(),
    );
    let h = setup_with_document(cx, d, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.click(("animation-motion-group", 0usize), cx);
        assert!(window.try_find(("animation-motion", 0usize)).is_none());
        let panel = h
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .animation_motions
            .clone();
        let document = h.view.read(cx).editor.active_id().unwrap();
        panel.update(cx, |panel, cx| panel.locate_object(document, &entry, cx));
        window.render_frame(cx);
        assert!(window.find(("animation-motion", 0usize)).visible());
        assert!(window.find(("animation-motion", 1usize)).visible());
        assert_eq!(
            window
                .find(("animation-motion-group-header", 0usize))
                .expanded(),
            Some(true)
        );
    })
    .unwrap();
}
#[gpui_kit::test]
fn locked_folder_toolbar_command_and_row_share_domain_reason_but_inspection_stays_available(
    cx: &mut TestAppContext,
) {
    let assets = tempfile::tempdir().unwrap();
    let mut s = zipzi_sdk::AuthoringSession::new(&id(), zipzi_core::Canvas::default()).unwrap();
    let group = ObjectRef::new(K::ParameterGroup, id());
    let plan = s
        .plan_organization_edit(zipzi_sdk::OrganizationEdit::CreateGroup {
            group: zipzi_core::document::CdiParameterGroup {
                id: group.id.clone(),
                runtime_id: "Group".into(),
                name: "Group".into(),
                parent_id: None,
                extensions: Default::default(),
            },
            before: None,
        })
        .unwrap();
    s.execute_organization_plan(&plan).unwrap();
    s.edit("Lock", None, |e| {
        let mut state = e.candidate_document().editor_state().clone();
        state.objects.entry(group.id.clone()).or_default().locked = true;
        e.replace_editor_state(state)
    })
    .unwrap();
    let saved = s
        .save_project(&assets.path().join("locked.zipzi"), None)
        .unwrap();
    let mut d = Document::open(&saved.manifest).unwrap();
    d.navigate(group.clone()).unwrap();
    let h = setup_with_document(cx, d, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let history = h.view.read(cx).editor.document().unwrap().history_lengths();
        window.click("new-parameter", cx);
        window.click("new-parameter-group-action", cx);
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            history
        );
        h.view.update(cx, |v, cx| {
            let doc = v.editor.document().unwrap();
            let row = ActionAvailability::for_object(doc, &group, A::CreateChild).unwrap();
            let menu = ActionAvailability::from_core(
                &v.editor
                    .authoring_command_capability(CommandId::CreateParameter)
                    .unwrap(),
            )
            .unwrap();
            assert!(!row.enabled);
            assert_eq!(row.reason, menu.reason);
            assert!(
                ActionAvailability::for_object(doc, &group, A::Inspect)
                    .unwrap()
                    .enabled
            );
            let history = doc.history_lengths();
            v.run_command(CommandId::EditParameterGroup, window, cx);
            assert_eq!(
                v.editor.active_inspection().unwrap().object(),
                Some(group.clone())
            );
            assert_eq!(v.editor.document().unwrap().history_lengths(), history);
            assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        });
    })
    .unwrap();
}
#[gpui_kit::test]
fn created_business_object_uses_workspace_inspector_without_activating_animation(
    cx: &mut TestAppContext,
) {
    let (d, assets) = document_fixture::combined_animation_document().unwrap();
    let h = setup_with_document(cx, d, assets);
    let mut created = None;
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            v.run_command(CommandId::AnimationWorkspace, window, cx);
            let document = v.editor.active_id().unwrap();
            let before = {
                let a = v.editor.animation().unwrap();
                (a.motion.clone(), a.expression.clone(), a.playing, a.time)
            };
            let selection = v.editor.document().unwrap().context().selections.clone();
            let context = AuthoringContext::capture(&v.editor).unwrap();
            let entity = id();
            let result = v
                .execute_authoring(context, window, cx, |e, d| {
                    let plan = e.document().unwrap().plan_reference_authoring(
                        zipzi_sdk::ReferenceEdit::CreateModelParameterGroup {
                            id: entity,
                            name: "Behavior".into(),
                            before: None,
                        },
                    )?;
                    e.execute_reference_authoring(d, &plan)
                })
                .unwrap();
            created = v
                .present_authoring_result(
                    document,
                    &result,
                    AuthoringPresentation::Created {
                        kind: K::ModelParameterGroup,
                        projection: None,
                    },
                    window,
                    cx,
                )
                .unwrap();
            let after = v.editor.animation().unwrap();
            assert_eq!(
                (
                    after.motion.clone(),
                    after.expression.clone(),
                    after.playing,
                    after.time
                ),
                before
            );
            assert_eq!(v.editor.document().unwrap().context().selections, selection);
            assert_eq!(v.editor.active_inspection().unwrap().object(), created);
            v.redraw(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find("property-title-name").label(), Some("Behavior"));

        assert_eq!(
            h.view.read(cx).editor.active_inspection().unwrap().object(),
            created
        );
        assert!(h.view.read(cx).edit_error.is_none());
    })
    .unwrap();
}
#[gpui_kit::test]
fn deferred_created_focus_and_location_do_not_overwrite_later_inspection(cx: &mut TestAppContext) {
    let h = setup_with_document(
        cx,
        Document::new_blank().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    let mut later = None;
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            let doc = v.editor.active_id().unwrap();
            for name in ["First", "Later"] {
                let plan = v
                    .editor
                    .document()
                    .unwrap()
                    .plan_parameter_object_creation(K::Parameter, name.into(), &[])
                    .unwrap();
                let result = v.editor.execute_organization_authoring(doc, &plan).unwrap();
                let object = result.suggested_selection[0].clone();
                if name == "First" {
                    v.present_authoring_result(
                        doc,
                        &result,
                        AuthoringPresentation::Created {
                            kind: K::Parameter,
                            projection: None,
                        },
                        window,
                        cx,
                    )
                    .unwrap();
                } else {
                    v.inspect_authoring_object(doc, object.clone(), true, window, cx)
                        .unwrap();
                    later = Some(object);
                }
            }
            v.redraw(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            h.view.read(cx).editor.active_inspection().unwrap().object(),
            later
        );
        window.input("Still later", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        let d = v.editor.document().unwrap();
        assert_eq!(d.parameters()[0].name, "First");
        assert_eq!(
            d.object_index().name(later.as_ref().unwrap()),
            "Still later"
        );
        assert_eq!(d.history_lengths(), (3, 0));
    });
}

#[gpui_kit::test]
fn created_resource_focuses_name_and_locates_when_shared_collection_tab_reopens(
    cx: &mut TestAppContext,
) {
    let mut d = Document::new_blank().unwrap();
    for n in 0..48 {
        let p = d
            .plan_resource_authoring(zipzi_sdk::ResourceEdit::Create {
                id: id(),
                name: format!("Texture {n}"),
                before: None,
            })
            .unwrap();
        d.execute_resource_authoring(&p).unwrap();
    }
    let h = setup_with_document(cx, d, tempfile::tempdir().unwrap());
    let mut created = None;
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            let doc = v.editor.active_id().unwrap();
            let p = v
                .editor
                .document()
                .unwrap()
                .plan_resource_authoring(zipzi_sdk::ResourceEdit::Create {
                    id: id(),
                    name: "New texture".into(),
                    before: None,
                })
                .unwrap();
            let result = v.editor.execute_resource_authoring(doc, &p).unwrap();
            created = v
                .present_authoring_result(
                    doc,
                    &result,
                    AuthoringPresentation::Created {
                        kind: K::Texture,
                        projection: None,
                    },
                    window,
                    cx,
                )
                .unwrap();
            v.redraw(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        assert!(window.find("property-input-name").visible());
        window.input("Loaded later", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            v.dock
                .as_ref()
                .unwrap()
                .show(zipzi_editor_core::PanelKind::Resources, None, window, cx)
        });
        window.render_frame(cx);
        window.render_frame(cx);
        let row = format!(
            "{}-Texture-{}-49-Unused",
            zipzi_editor_core::PanelKind::Resources.name(),
            created.as_ref().unwrap().id
        );
        assert!(
            window.find(row.clone()).visible(),
            "row={:?}, body={:?}",
            window.find(row).bounds(),
            window.find("Resources-body").bounds()
        );
    })
    .unwrap();
    h.view.read_with(cx, |v, _| {
        assert_eq!(
            v.editor
                .document()
                .unwrap()
                .object_index()
                .name(created.as_ref().unwrap()),
            "Loaded later"
        );
        assert_eq!(v.editor.document().unwrap().history_lengths(), (50, 0));
    });
}

#[gpui_kit::test]
fn legacy_new_motion_uses_shared_result_focus_and_keeps_its_existing_activation(
    cx: &mut TestAppContext,
) {
    let (d, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, d, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click("animation-new-motion", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let v = h.view.read(cx);
        let inspected = v.editor.active_inspection().unwrap().object().unwrap();
        assert_eq!(inspected.kind, K::Motion);
        assert_eq!(
            v.editor
                .animation()
                .unwrap()
                .motion
                .as_ref()
                .unwrap()
                .clip_id,
            inspected.id
        );
        assert_eq!(v.editor.document().unwrap().history_lengths(), (1, 0));
        assert!(window.find("property-input-name").visible());
        window.input("New clip", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        let r = v.editor.active_inspection().unwrap().object().unwrap();
        assert_eq!(
            v.editor.document().unwrap().object_index().name(&r),
            "New clip"
        );
        assert_eq!(v.editor.document().unwrap().history_lengths(), (2, 0));
    });
}

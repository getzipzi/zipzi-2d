use kasane_core::{document::*, *};
use kasane_sdk::{
    AuthoringSession, DocumentAuthoringEdit as D, DocumentField as F, DocumentValue as V,
    ObjectKind, ResourceEdit as R,
};
use std::sync::Arc;
fn id(n: u32) -> String {
    format!("40000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap()
}

#[test]
fn cdi_repair_refreshes_organization_inside_the_same_candidate() {
    let mut s = session();
    s.edit("Imported CDI", None, |e| {
        e.create_parameter(Parameter {
            id: id(60),
            ..Default::default()
        })?;
        let mut info = e.candidate_document().display_info().clone();
        info.parameter_groups = Some(vec![CdiParameterGroup {
            id: id(61),
            runtime_id: "Group".into(),
            name: "Group".into(),
            parent_id: None,
            extensions: Default::default(),
        }]);
        info.parameters = Some(vec![CdiParameterEntry::Unresolved {
            runtime_id: "Lost".into(),
            name: "Imported".into(),
            group_id: Some(id(61)),
            extensions: Default::default(),
        }]);
        e.replace_display_info(info)
    })
    .unwrap();
    s.edit("Repair then query", None, |e| {
        assert_eq!(e.candidate_document().organization_owner(&id(60)), None);
        e.edit_document(D::RepairCdiParameter {
            runtime_id: "Lost".into(),
            parameter_id: Some(id(60)),
        })?;
        assert_eq!(
            e.candidate_document().organization_owner(&id(60)),
            Some(id(61).as_str())
        );
        assert_eq!(
            e.candidate_document()
                .organization_collection(&OrganizationCollection::Parameters {
                    group_id: Some(id(61))
                })
                .unwrap(),
            vec![id(60)]
        );
        Ok(())
    })
    .unwrap();
}
fn run(s: &mut AuthoringSession, e: R) -> kasane_sdk::EditReceipt {
    let p = s.plan_resource_edit(e).unwrap();
    s.execute_resource_plan(&p).unwrap()
}
fn create(s: &mut AuthoringSession, n: u32) {
    run(
        s,
        R::Create {
            id: id(n),
            name: String::new(),
            before: None,
        },
    );
}
fn content() -> ImageContent {
    kasane_sdk::prepare_png_asset(
        &id(10),
        "",
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png"),
    )
    .unwrap()
    .content
    .unwrap()
}
fn attach(s: &mut AuthoringSession, n: u32) {
    let p = s
        .plan_attachment_edit(AttachmentEdit::Create {
            id: id(n),
            path: format!("files/{n}.bin"),
        })
        .unwrap();
    s.execute_attachment_plan(&p).unwrap();
}
fn lock(s: &mut AuthoringSession, n: u32) {
    let mut flags = s.document().editor_state().clone();
    flags.objects.insert(
        id(n),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("Lock", None, |e| e.replace_editor_state(flags))
        .unwrap();
}
#[test]
fn empty_images_are_native_resources_and_save_without_fake_pixels() {
    let mut s = session();
    create(&mut s, 10);
    assert!(s.asset(&id(10)).unwrap().content.is_none());
    assert_eq!(
        s.read_texture(&id(10)).unwrap_err().code.as_ref(),
        "MISSING_IMAGE_CONTENT"
    );
    let dir = std::env::temp_dir().join(format!(
        "kasane-empty-image-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    s.save_project(&dir, None).unwrap();
    let mut reopened = session();
    reopened.open_project(&dir, None).unwrap();
    assert!(reopened.asset(&id(10)).unwrap().content.is_none());
    let text = kasane_project::encode_project(reopened.document()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["format_version"], 20);
    assert!(json["document"]["assets"][0]["content"].is_null());
    assert!(json["document"]["assets"][0].get("width").is_none());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn missing_image_content_propagates_masks_and_composites_without_losing_geometry() {
    let mut s = session();
    create(&mut s, 10);
    create(&mut s, 11);
    let t = s.image_load_token(&id(11)).unwrap();
    s.replace_image_content(&t, Some(content())).unwrap();
    s.edit("Scene", None, |e| {
        e.create_part(Part {
            id: id(30),
            ..Default::default()
        })?;
        for (n, texture, part, masks) in [
            (20, 10, String::new(), vec![]),
            (21, 11, String::new(), vec![id(20)]),
            (22, 11, id(30), vec![]),
        ] {
            let mut mesh = kasane_sdk::rectangle_mesh(
                &id(n),
                "",
                &id(texture),
                Vec2::default(),
                Vec2::new(10., 10.),
            )
            .unwrap();
            mesh.part_id = part;
            mesh.masks = masks;
            e.create_mesh(mesh)?;
        }
        e.create_offscreen(Offscreen {
            id: id(31),
            part_id: id(30),
            masks: vec![id(20)],
            ..Default::default()
        })
    })
    .unwrap();
    let p = s.scene_projection();
    assert_eq!(p.geometry.len(), 3);
    assert!(p.drawing.is_empty());
    assert!(p.composites.is_empty());
    assert_eq!(s.document().get_mesh(&id(21)).unwrap().masks, [id(20)]);
    assert!(!s.document().scene_export_omissions().is_empty());
    let t = s.image_load_token(&id(10)).unwrap();
    s.replace_image_content(&t, Some(content())).unwrap();
    assert_eq!(s.scene_projection().drawing.len(), 3);
    assert_eq!(s.scene_projection().composites, [id(31)]);
    let t = s.image_load_token(&id(10)).unwrap();
    s.replace_image_content(&t, None).unwrap();
    assert!(s.scene_projection().drawing.is_empty());
}
#[test]
fn resource_names_order_copies_and_locks_share_one_history() {
    let mut s = session();
    create(&mut s, 10);
    let t = s.image_load_token(&id(10)).unwrap();
    s.replace_image_content(&t, Some(content())).unwrap();
    let evaluation = s.evaluation_revision();
    let address = s.resource_name_address(&id(10)).unwrap();
    let p = s.plan_resource_name_patch(&address, "Face".into()).unwrap();
    let r = s.execute_resource_plan(&p).unwrap();
    assert_eq!(s.evaluation_revision(), evaluation);
    assert_eq!(
        r.changes.categories,
        std::collections::BTreeSet::from([kasane_sdk::ChangeCategory::Labels])
    );
    s.undo().unwrap();
    assert!(s.plan_resource_name_patch(&address, "Face".into()).is_err());
    let h = s.handle(ObjectKind::Asset, &id(10)).unwrap();
    run(
        &mut s,
        R::Duplicate {
            source: id(10),
            id: id(11),
            name: String::new(),
            before: Some(id(10)),
        },
    );
    assert_eq!(
        s.asset(&id(10)).unwrap().content,
        s.asset(&id(11)).unwrap().content
    );
    s.resolve_handle(&h).unwrap();
    let token = s.image_load_token(&id(11)).unwrap();
    s.replace_image_content(&token, None).unwrap();
    assert!(s.asset(&id(10)).unwrap().content.is_some());
    run(
        &mut s,
        R::Reorder {
            ids: vec![id(11)],
            before: None,
        },
    );
    assert_eq!(s.asset_ids(), [id(10), id(11)]);
    lock(&mut s, 10);
    assert!(s
        .plan_resource_edit(R::Delete {
            id: id(10),
            replacement: None
        })
        .is_err());
    let mut a = s.asset(&id(10)).unwrap();
    a.name = "bypass".into();
    assert_eq!(
        s.edit("Bypass", None, |e| e.replace_asset(a))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
}
#[test]
fn newer_cancelled_and_undone_image_loads_cannot_publish_old_results() {
    let mut s = session();
    create(&mut s, 10);
    let old = s.image_load_token(&id(10)).unwrap();
    let current = s.image_load_token(&id(10)).unwrap();
    assert_eq!(
        s.replace_image_content(&old, Some(content()))
            .unwrap_err()
            .code
            .as_ref(),
        "CANCELLED"
    );
    let a = s.resource_name_address(&id(10)).unwrap();
    let p = s.plan_resource_name_patch(&a, "Renamed".into()).unwrap();
    s.execute_resource_plan(&p).unwrap();
    s.replace_image_content(&current, Some(content())).unwrap();
    assert_eq!(s.asset(&id(10)).unwrap().name, "Renamed");
    let stale = s.image_load_token(&id(10)).unwrap();
    s.undo().unwrap();
    assert_eq!(
        s.replace_image_content(&stale, None)
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_CONTENT"
    );
    let cancelled = s.image_load_token(&id(10)).unwrap();
    cancelled.cancel();
    let v = s.version();
    assert!(s.replace_image_content(&cancelled, None).is_err());
    assert_eq!(s.version(), v);
    let stale = s.image_load_token(&id(10)).unwrap();
    run(
        &mut s,
        R::Delete {
            id: id(10),
            replacement: None,
        },
    );
    s.undo().unwrap();
    assert!(s.replace_image_content(&stale, None).is_err());
}
#[test]
fn texture_deletion_requires_explicit_reference_replacement_and_locks_every_user() {
    let mut s = session();
    create(&mut s, 10);
    create(&mut s, 11);
    s.edit("Mesh", None, |e| {
        e.create_mesh(Mesh {
            id: id(20),
            texture_asset_id: id(10).into(),
            ..Default::default()
        })
    })
    .unwrap();
    assert!(s
        .plan_resource_edit(R::Delete {
            id: id(10),
            replacement: None
        })
        .is_err());
    run(
        &mut s,
        R::Delete {
            id: id(10),
            replacement: Some(id(11).into()),
        },
    );
    assert_eq!(
        s.document()
            .get_mesh(&id(20))
            .unwrap()
            .texture_asset_id
            .resolved_id(),
        Some(id(11).as_str())
    );
    s.undo().unwrap();
    lock(&mut s, 20);
    assert!(s
        .plan_resource_edit(R::Delete {
            id: id(10),
            replacement: Some(SceneReference::Unspecified)
        })
        .is_err());
}
#[test]
fn attachments_distinguish_empty_bytes_preserve_references_and_cancel_old_loads() {
    let mut s = session();
    attach(&mut s, 50);
    let t = s.attachment_load_token(&id(50)).unwrap();
    let newer = s.attachment_load_token(&id(50)).unwrap();
    assert_eq!(
        s.replace_attachment_content(&t, Some(Arc::new(vec![1])))
            .unwrap_err()
            .code
            .as_ref(),
        "CANCELLED"
    );
    let evaluation = s.evaluation_revision();
    s.replace_attachment_content(&newer, Some(Arc::new(vec![])))
        .unwrap();
    assert_eq!(s.evaluation_revision(), evaluation);
    assert_eq!(
        s.document()
            .get_attachment(&id(50))
            .unwrap()
            .bytes
            .as_deref(),
        Some(&vec![])
    );
    let p = s
        .plan_attachment_edit(AttachmentEdit::Duplicate {
            source: id(50),
            id: id(51),
            path: "files/copy.bin".into(),
        })
        .unwrap();
    s.execute_attachment_plan(&p).unwrap();
    assert!(Arc::ptr_eq(
        s.document()
            .get_attachment(&id(50))
            .unwrap()
            .bytes
            .as_ref()
            .unwrap(),
        s.document()
            .get_attachment(&id(51))
            .unwrap()
            .bytes
            .as_ref()
            .unwrap()
    ));
    let p = s
        .plan_attachment_edit(AttachmentEdit::SetUserData {
            reference: Some(AttachmentRef::Resolved {
                attachment_id: id(50),
            }),
        })
        .unwrap();
    s.execute_attachment_plan(&p).unwrap();
    let address = s.attachment_path_address(&id(50)).unwrap();
    let p = s
        .plan_attachment_path_patch(&address, "data/renamed.bin".into())
        .unwrap();
    s.execute_attachment_plan(&p).unwrap();
    assert_eq!(
        s.document().model3_settings().user_data,
        Some(AttachmentRef::Resolved {
            attachment_id: id(50)
        })
    );
    s.undo().unwrap();
    assert!(s
        .plan_attachment_path_patch(&address, "data/new.bin".into())
        .is_err());
    let p = s
        .plan_attachment_edit(AttachmentEdit::DeleteWithReplacement {
            id: id(50),
            replacement: Some(id(51)),
        })
        .unwrap();
    s.execute_attachment_plan(&p).unwrap();
    assert_eq!(
        s.document().model3_settings().user_data,
        Some(AttachmentRef::Resolved {
            attachment_id: id(51)
        })
    );
    assert!(s.document().get_attachment(&id(50)).is_none());
}
#[test]
fn document_fields_are_precise_nonvisual_and_do_not_revive_after_undo() {
    let mut s = session();
    let eval = s.evaluation_revision();
    let a = s.document_field_address(F::Layout).unwrap();
    let p = s
        .plan_document_field_patch(
            &a,
            V::Layout(Some(std::collections::BTreeMap::from([(
                "CenterX".into(),
                1.,
            )]))),
        )
        .unwrap();
    let r = s.execute_document_plan(&p).unwrap();
    assert_eq!(r.changes.document_fields, [F::Layout]);
    assert_eq!(s.evaluation_revision(), eval);
    s.undo().unwrap();
    assert!(s.plan_document_field_patch(&a, V::Layout(None)).is_err());
    let a = s.document_field_address(F::ExportSettings).unwrap();
    let mut export = s.document().export_settings();
    export.padding_pixels = 42.;
    let p = s
        .plan_document_field_patch(&a, V::ExportSettings(export))
        .unwrap();
    s.execute_document_plan(&p).unwrap();
    assert_eq!(s.evaluation_revision(), eval);
    let a = s.document_field_address(F::SourceCanvas).unwrap();
    assert!(s
        .plan_document_field_patch(&a, V::SourceCanvas(None))
        .is_err());
    let p = s.plan_document_edit(D::Patch {
        expected: V::ExportSettings(export),
        value: V::ExportSettings(ExportSettings {
            pixels_per_unit: 0.,
            ..export
        }),
    });
    assert!(p.is_err());
}
#[test]
fn missing_userdata_and_cdi_repair_are_explicit_and_preserve_target_identity() {
    let mut s = session();
    attach(&mut s, 50);
    let mut settings = s.document().model3_settings().clone();
    settings.user_data = Some(AttachmentRef::Unresolved {
        path: "lost/data.json".into(),
    });
    s.edit("Missing metadata", None, |e| {
        e.set_model3_settings(settings)?;
        e.create_parameter(Parameter {
            id: id(60),
            name: "Current".into(),
            ..Default::default()
        })?;
        let mut info = e.candidate_document().display_info().clone();
        info.parameters = Some(vec![CdiParameterEntry::Unresolved {
            runtime_id: "LostParameter".into(),
            name: "Imported".into(),
            group_id: None,
            extensions: Default::default(),
        }]);
        e.replace_display_info(info)
    })
    .unwrap();
    let p = s
        .plan_document_edit(D::RepairMissingAttachment {
            path: "lost/data.json".into(),
            attachment_id: Some(id(50)),
        })
        .unwrap();
    s.execute_document_plan(&p).unwrap();
    assert!(!s
        .document()
        .attachment_available(s.document().model3_settings().user_data.as_ref().unwrap()));
    let handle = s.handle(ObjectKind::Parameter, &id(60)).unwrap();
    let p = s
        .plan_document_edit(D::RepairCdiParameter {
            runtime_id: "LostParameter".into(),
            parameter_id: Some(id(60)),
        })
        .unwrap();
    assert_eq!(p.losses().len(), 1);
    s.execute_document_plan(&p).unwrap();
    s.resolve_handle(&handle).unwrap();
    assert!(
        matches!(&s.document().display_info().parameters.as_ref().unwrap()[0],CdiParameterEntry::Resolved{parameter_id,..} if parameter_id==&id(60))
    );
}
#[test]
fn failed_resource_batches_leave_no_history_or_half_created_objects() {
    let mut s = session();
    let version = s.version();
    let r = s.edit("Invalid batch", None, |e| {
        e.edit_resource(R::Create {
            id: id(10),
            name: String::new(),
            before: None,
        })?;
        e.edit_resource(R::Reorder {
            ids: vec![id(999)],
            before: None,
        })
    });
    assert!(r.is_err());
    assert_eq!(s.version(), version);
    assert!(s.asset(&id(10)).is_none());
}

#[test]
fn broken_image_sources_roundtrip_without_dimensions_and_repair_keeps_identity() {
    let mut s = session();
    let source = "/missing/original/texture.png".to_owned();
    s.edit("Import missing image", None, |e| {
        e.create_asset(ImageAsset {
            id: id(10),
            name: "Imported".into(),
            content: None,
            unresolved_source: Some(source.clone()),
        })
    })
    .unwrap();
    let handle = s.handle(ObjectKind::Asset, &id(10)).unwrap();
    let encoded = kasane_project::encode_project(s.document()).unwrap();
    let decoded = kasane_project::decode_project(&encoded).unwrap();
    assert_eq!(
        decoded.get_asset(&id(10)).unwrap().unresolved_source,
        Some(source)
    );
    assert_eq!(
        s.read_texture(&id(10)).unwrap_err().code.as_ref(),
        "MISSING_IMAGE_SOURCE"
    );
    let token = s.image_load_token(&id(10)).unwrap();
    s.replace_image_content(&token, Some(content())).unwrap();
    assert!(s.asset(&id(10)).unwrap().unresolved_source.is_none());
    s.resolve_handle(&handle).unwrap();
}
#[test]
fn cdi_part_repair_and_explicit_missing_reference_removal_are_nonvisual() {
    let mut s = session();
    s.edit("Imported CDI Part", None, |e| {
        e.create_part(Part {
            id: id(20),
            name: "Current".into(),
            ..Default::default()
        })?;
        let mut info = e.candidate_document().display_info().clone();
        info.parts = Some(vec![CdiPartEntry::Unresolved {
            runtime_id: "LostPart".into(),
            name: "Imported".into(),
            extensions: Default::default(),
        }]);
        e.replace_display_info(info)
    })
    .unwrap();
    let eval = s.evaluation_revision();
    let p = s
        .plan_document_edit(D::RepairCdiPart {
            runtime_id: "LostPart".into(),
            part_id: Some(id(20)),
        })
        .unwrap();
    assert_eq!(p.losses().len(), 1);
    s.execute_document_plan(&p).unwrap();
    assert_eq!(s.evaluation_revision(), eval);
    s.undo().unwrap();
    let p = s
        .plan_document_edit(D::RepairCdiPart {
            runtime_id: "LostPart".into(),
            part_id: None,
        })
        .unwrap();
    s.execute_document_plan(&p).unwrap();
    assert!(s
        .document()
        .display_info()
        .parts
        .as_ref()
        .unwrap()
        .is_empty());
    assert!(s.document().get_part(&id(20)).is_some());
}

#[test]
fn invalid_load_kind_does_not_cancel_an_image_request_and_default_paths_are_unique() {
    let mut s = session();
    create(&mut s, 10);
    let request = s.image_load_token(&id(10)).unwrap();
    assert!(s.attachment_load_token(&id(10)).is_err());
    s.replace_image_content(&request, Some(content())).unwrap();
    for n in [50, 51] {
        let p = s
            .plan_attachment_edit(AttachmentEdit::Create {
                id: id(n),
                path: String::new(),
            })
            .unwrap();
        s.execute_attachment_plan(&p).unwrap();
    }
    assert_ne!(
        s.document().get_attachment(&id(50)).unwrap().path,
        s.document().get_attachment(&id(51)).unwrap().path
    );
    assert!(s
        .document()
        .get_attachment(&id(50))
        .unwrap()
        .bytes
        .is_none());
}

#[test]
fn prepared_image_loads_own_pixels_without_file_io_during_publication() {
    let path = std::env::temp_dir().join(format!(
        "kasane-prepared-image-{}.png",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png"),
        &path,
    )
    .unwrap();
    let prepared = kasane_sdk::prepare_image_load(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let mut s = session();
    create(&mut s, 10);
    let token = s.image_load_token(&id(10)).unwrap();
    s.replace_prepared_image_content(&token, &prepared).unwrap();
    let pixels = s.read_texture(&id(10)).unwrap();
    let snapshot = s.read_snapshot();
    let token = s.image_load_token(&id(10)).unwrap();
    s.replace_image_content(&token, None).unwrap();
    drop(s);
    assert_eq!(
        snapshot
            .memory_image(&id(10))
            .unwrap()
            .read_asset()
            .unwrap()
            .rgba
            .as_slice(),
        pixels.rgba.as_ref()
    );
}

#[test]
fn staged_image_load_checks_cancellation_and_version_again_at_commit() {
    let mut s = session();
    create(&mut s, 10);
    let original = s.version();
    let history = s.history_state();
    let token = s.image_load_token(&id(10)).unwrap();
    let edit = s.begin_image_content_edit(&token, Some(content())).unwrap();
    let candidate = edit.candidate_document().get_asset(&id(10)).unwrap();
    assert_eq!(s.read_texture_asset(candidate).unwrap().width, 2);
    assert!(s.asset(&id(10)).unwrap().content.is_none());
    token.cancel();
    assert_eq!(
        edit.commit_to(&mut s).unwrap_err().code.as_ref(),
        "CANCELLED"
    );
    assert_eq!(s.version(), original);
    assert_eq!(s.history_state(), history);

    let token = s.image_load_token(&id(10)).unwrap();
    let edit = s.begin_image_content_edit(&token, Some(content())).unwrap();
    let expected = s.asset(&id(10)).unwrap().name;
    run(
        &mut s,
        R::PatchName {
            id: id(10),
            expected,
            name: "Renamed".into(),
        },
    );
    assert_eq!(
        edit.commit_to(&mut s).unwrap_err().code.as_ref(),
        "STALE_VERSION"
    );
    // A rejected candidate does not consume the valid load request.
    s.replace_image_content(&token, Some(content())).unwrap();
}

#[test]
fn committing_an_unchanged_staged_load_consumes_the_request_without_history() {
    let mut s = session();
    create(&mut s, 10);
    let version = s.version();
    let history = s.history_state();
    let token = s.image_load_token(&id(10)).unwrap();
    let edit = s.begin_image_content_edit(&token, None).unwrap();
    assert!(!edit.commit_to(&mut s).unwrap().changed);
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert_eq!(
        s.begin_image_content_edit(&token, None)
            .err()
            .unwrap()
            .code
            .as_ref(),
        "CANCELLED"
    );
}

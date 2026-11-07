use std::sync::Arc;
use zipzi_contract::authoring::ObjectKind;
use zipzi_core::document::{DocumentAuthoringEdit as D, DocumentValue as V, ResourceEdit as R};
use zipzi_core::{document::*, *};
use zipzi_sdk::AuthoringSession;
fn id(n: u32) -> String {
    format!("40000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(1),
                (Canvas::new(100., 100., Vec2::default(), 1.)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    )
}

#[test]
fn cdi_repair_refreshes_organization_inside_the_same_candidate() {
    let mut s = session();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(60),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                let mut info = e.read_document().unwrap().display_info().clone();
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
            .and_then(|value| {
                let prepared = draft.prepare("Imported CDI")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                assert_eq!(e.read_document().unwrap().organization_owner(&id(60)), None);
                e.edit_document(D::RepairCdiParameter {
                    runtime_id: "Lost".into(),
                    parameter_id: Some(id(60)),
                })?;
                assert_eq!(
                    e.read_document().unwrap().organization_owner(&id(60)),
                    Some(id(61).as_str())
                );
                assert_eq!(
                    e.read_document()
                        .unwrap()
                        .organization_collection(&OrganizationCollection::Parameters {
                            group_id: Some(id(61))
                        })
                        .unwrap(),
                    vec![id(60)]
                );
                Ok(())
            })
            .and_then(|value| {
                let prepared = draft.prepare("Repair then query")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}
fn run(s: &mut AuthoringSession, e: R) -> zipzi_sdk::Publication {
    let p = crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_resource(e))
        .unwrap();
    s.publish(p.1).unwrap()
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

#[test]
fn resource_field_addresses_distinguish_changed_fields_from_recreated_objects() {
    let mut s = session();
    create(&mut s, 10);
    let (_, stale) = crate::support::prepared(&s, |c| {
        c.edit_resource(R::PatchName {
            id: id(10),
            expected: String::new(),
            name: "Late".into(),
        })
    })
    .unwrap();
    run(
        &mut s,
        R::PatchName {
            id: id(10),
            expected: String::new(),
            name: "Renamed".into(),
        },
    );
    s.undo(s.version()).unwrap();
    assert_eq!(s.publish(stale).unwrap_err().code, "STALE_SOURCE");
    run(
        &mut s,
        R::Delete {
            id: id(10),
            replacement: None,
        },
    );
    create(&mut s, 10);
    run(
        &mut s,
        R::PatchName {
            id: id(10),
            expected: String::new(),
            name: "Fresh".into(),
        },
    );
    assert_eq!(
        s.snapshot().document().asset(&id(10)).unwrap().name,
        "Fresh"
    );
}

#[test]
fn missing_image_content_propagates_masks_and_composites_without_losing_geometry() {
    let mut s = session();
    create(&mut s, 10);
    create(&mut s, 11);
    replace_image(&mut s, 11, Some(content()));
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_part({
                    let mut value = Part {
                        id: id(30),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                for (n, texture, part, masks) in [
                    (20, 10, String::new(), vec![]),
                    (21, 11, String::new(), vec![id(20)]),
                    (22, 11, id(30), vec![]),
                ] {
                    let mut mesh = zipzi_sdk::rectangle_mesh(
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
                crate::support::insert_offscreen(
                    e,
                    Offscreen {
                        id: id(31),
                        part_id: id(30),
                        masks: vec![id(20)],
                        ..Default::default()
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Scene")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let p = s.snapshot().document().scene_projection();
    assert_eq!(p.geometry.len(), 3);
    assert!(p.drawing.is_empty());
    assert!(p.composites.is_empty());
    assert_eq!(
        s.snapshot().document().mesh(&id(21)).unwrap().masks,
        [id(20)]
    );
    assert!(!s.snapshot().document().scene_export_omissions().is_empty());
    replace_image(&mut s, 10, Some(content()));
    assert_eq!(s.snapshot().document().scene_projection().drawing.len(), 3);
    assert_eq!(
        s.snapshot().document().scene_projection().composites,
        [id(31)]
    );
    replace_image(&mut s, 10, None);
    assert!(s
        .snapshot()
        .document()
        .scene_projection()
        .drawing
        .is_empty());
}
#[test]
fn resource_names_order_copies_and_locks_share_one_history() {
    let mut s = session();
    create(&mut s, 10);
    replace_image(&mut s, 10, Some(content()));
    let evaluation = s.snapshot().content_id();
    let p = crate::support::prepared(&s, |c| {
        c.edit_resource(R::PatchName {
            id: id(10),
            expected: String::new(),
            name: "Face".into(),
        })
    })
    .unwrap();
    let r = s.publish(p.1).unwrap();
    assert_ne!(s.snapshot().content_id(), evaluation);
    assert!(r.writes.records().iter().any(|w| w.id == id(10)));
    s.undo(s.version()).unwrap();
    let h = {
        let kind = ObjectKind::Asset;
        let object_id = (&id(10)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
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
        s.snapshot()
            .document()
            .asset(&id(10))
            .cloned()
            .unwrap()
            .content,
        s.snapshot()
            .document()
            .asset(&id(11))
            .cloned()
            .unwrap()
            .content
    );
    s.snapshot().document().require_object(h.0, &h.1).unwrap();
    replace_image(&mut s, 11, None);
    assert!(matches!(
        s.snapshot().document().asset(&id(10)).unwrap().content,
        zipzi_core::state::AssetContent::Image { .. }
    ));
    run(
        &mut s,
        R::Reorder {
            ids: vec![id(11)],
            before: None,
        },
    );
    assert_eq!(s.snapshot().document().asset_order(), [id(10), id(11)]);
    lock(&mut s, 10);
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_resource(
            R::Delete {
                id: id(10),
                replacement: None
            }
        ))
        .is_err()
    );
    let mut a = s.snapshot().document().asset(&id(10)).cloned().unwrap();
    a.name = "bypass".into();
    assert_eq!(
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_asset(a))
                .and_then(|value| {
                    let prepared = draft.prepare("Bypass")?;
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
fn newer_cancelled_and_undone_image_loads_cannot_publish_old_results() {
    let mut s = session();
    create(&mut s, 10);
    let (_, old) = crate::support::prepared(&s, |c| c.replace_image(&id(10), content())).unwrap();
    replace_image(&mut s, 10, Some(content()));
    s.undo(s.version()).unwrap();
    assert_eq!(s.publish(old).unwrap_err().code, "STALE_SOURCE");
    let mut cancelled = s.begin_draft();
    cancelled
        .batch(cancelled.source(), |c| c.replace_image(&id(10), content()))
        .unwrap();
    let before = s.version();
    cancelled.cancel();
    assert_eq!(s.version(), before);
    let (_, stale) = crate::support::prepared(&s, |c| c.replace_image(&id(10), content())).unwrap();
    run(
        &mut s,
        R::Delete {
            id: id(10),
            replacement: None,
        },
    );
    s.undo(s.version()).unwrap();
    assert_eq!(s.publish(stale).unwrap_err().code, "STALE_SOURCE");
}

#[test]
fn texture_deletion_requires_explicit_reference_replacement_and_locks_every_user() {
    let mut s = session();
    create(&mut s, 10);
    create(&mut s, 11);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_mesh({
                    let mut value = Mesh {
                        id: id(20),
                        texture_asset_id: id(10).into(),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Mesh")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_resource(
            R::Delete {
                id: id(10),
                replacement: None
            }
        ))
        .is_err()
    );
    run(
        &mut s,
        R::Delete {
            id: id(10),
            replacement: Some(id(11).into()),
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .mesh(&id(20))
            .unwrap()
            .texture_asset_id
            .resolved_id(),
        Some(id(11).as_str())
    );
    s.undo(s.version()).unwrap();
    lock(&mut s, 20);
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands.edit_resource(
            R::Delete {
                id: id(10),
                replacement: Some(SceneReference::Unspecified)
            }
        ))
        .is_err()
    );
}
#[test]
fn attachments_distinguish_empty_bytes_preserve_references_and_cancel_old_loads() {
    let mut s = session();
    attach(&mut s, 50);
    let expected = s.snapshot().document().attachment(&id(50)).unwrap().clone();
    let mut attachment = expected.clone();
    attachment.bytes = Some(Arc::new(vec![]));
    let (_, p) = crate::support::prepared(&s, |c| {
        c.edit_attachment(AttachmentEdit::Replace {
            expected,
            attachment,
        })
    })
    .unwrap();
    s.publish(p).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .attachment(&id(50))
            .unwrap()
            .bytes
            .as_deref(),
        Some(&vec![])
    );
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_attachment(AttachmentEdit::Duplicate {
            source: id(50),
            id: id(51),
            path: "files/copy.bin".into(),
        })
    })
    .unwrap();
    s.publish(p.1).unwrap();
    assert!(Arc::ptr_eq(
        s.snapshot()
            .document()
            .attachment(&id(50))
            .unwrap()
            .bytes
            .as_ref()
            .unwrap(),
        s.snapshot()
            .document()
            .attachment(&id(51))
            .unwrap()
            .bytes
            .as_ref()
            .unwrap()
    ));
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_attachment(AttachmentEdit::SetUserData {
            reference: Some(AttachmentRef::Resolved {
                attachment_id: id(50),
            }),
        })
    })
    .unwrap();
    s.publish(p.1).unwrap();
    let stale = s.begin_draft();
    let expected = s.snapshot().document().attachment(&id(50)).unwrap().clone();
    let mut attachment = expected.clone();
    attachment.path = "data/renamed.bin".into();
    let (_, p) = crate::support::prepared(&s, |c| {
        c.edit_attachment(AttachmentEdit::Replace {
            expected,
            attachment,
        })
    })
    .unwrap();
    s.publish(p).unwrap();
    assert_eq!(
        s.snapshot().document().model3_settings().user_data,
        Some(AttachmentRef::Resolved {
            attachment_id: id(50)
        })
    );
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.publish(stale.prepare("old path").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_attachment(AttachmentEdit::DeleteWithReplacement {
            id: id(50),
            replacement: Some(id(51)),
        })
    })
    .unwrap();
    s.publish(p.1).unwrap();
    assert_eq!(
        s.snapshot().document().model3_settings().user_data,
        Some(AttachmentRef::Resolved {
            attachment_id: id(51)
        })
    );
    assert!(s.snapshot().document().attachment(&id(50)).is_none());
}
#[test]
fn document_fields_are_precise_nonvisual_and_do_not_revive_after_undo() {
    let mut s = session();
    let eval = s.snapshot().content_id();
    let stale = s.begin_draft();
    let (_, p) = crate::support::prepared(&s, |c| {
        c.edit_document(D::Patch {
            expected: V::Layout(None),
            value: V::Layout(Some(std::collections::BTreeMap::from([(
                "CenterX".into(),
                1.,
            )]))),
        })
    })
    .unwrap();
    let r = s.publish(p).unwrap();
    assert!(r
        .writes
        .root_changed(zipzi_core::state::RootDomain::Model3Settings));
    assert_ne!(s.snapshot().content_id(), eval);
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.publish(stale.prepare("old fields").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    let expected = s.snapshot().document().setup().export;
    let mut export = expected;
    export.padding_pixels = 42.;
    let (_, p) = crate::support::prepared(&s, |c| {
        c.edit_document(D::Patch {
            expected: V::ExportSettings(expected),
            value: V::ExportSettings(export),
        })
    })
    .unwrap();
    s.publish(p).unwrap();
    assert_eq!(s.snapshot().document().setup().export.padding_pixels, 42.);
    assert!(crate::support::prepared(&s, |c| c.edit_document(D::Patch {
        expected: V::ExportSettings(export),
        value: V::ExportSettings(ExportSettings {
            pixels_per_unit: 0.,
            ..export
        })
    }))
    .is_err());
}

#[test]
fn missing_userdata_and_cdi_repair_are_explicit_and_preserve_target_identity() {
    let mut s = session();
    attach(&mut s, 50);
    let mut settings = s.snapshot().document().model3_settings().clone();
    settings.user_data = Some(AttachmentRef::Unresolved {
        path: "lost/data.json".into(),
    });
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.replace_model3_settings(settings)?;
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(60),
                        name: "Current".into(),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                let mut info = e.read_document().unwrap().display_info().clone();
                info.parameters = Some(vec![CdiParameterEntry::Unresolved {
                    runtime_id: "LostParameter".into(),
                    name: "Imported".into(),
                    group_id: None,
                    extensions: Default::default(),
                }]);
                e.replace_display_info(info)
            })
            .and_then(|value| {
                let prepared = draft.prepare("Missing metadata")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let p = crate::support::prepared(&s, |c| {
        c.edit_document(D::RepairMissingAttachment {
            path: "lost/data.json".into(),
            attachment_id: Some(id(50)),
        })
    })
    .unwrap();
    s.publish(p.1).unwrap();
    assert!(!s.snapshot().document().attachment_available(
        s.snapshot()
            .document()
            .model3_settings()
            .user_data
            .as_ref()
            .unwrap()
    ));
    let handle = {
        let kind = ObjectKind::Parameter;
        let object_id = (&id(60)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let p = crate::support::prepared(&s, |c| {
        c.edit_document(D::RepairCdiParameter {
            runtime_id: "LostParameter".into(),
            parameter_id: Some(id(60)),
        })
    })
    .unwrap();
    assert_eq!(p.0.losses.len(), 1);
    s.publish(p.1).unwrap();
    s.snapshot()
        .document()
        .require_object(handle.0, &handle.1)
        .unwrap();
    assert!(
        matches!(&s.snapshot().document().display_info().parameters.as_ref().unwrap()[0],CdiParameterEntry::Resolved{parameter_id,..} if parameter_id==&id(60))
    );
}
#[test]
fn failed_resource_batches_leave_no_history_or_half_created_objects() {
    let mut s = session();
    let version = s.version();
    let r = {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_resource(R::Create {
                    id: id(10),
                    name: String::new(),
                    before: None,
                })?;
                e.edit_resource(R::Reorder {
                    ids: vec![id(999)],
                    before: None,
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Invalid batch")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    };
    assert!(r.is_err());
    assert_eq!(s.version(), version);
    assert!(s.snapshot().document().asset(&id(10)).cloned().is_none());
}

#[test]
fn broken_image_sources_roundtrip_without_dimensions_and_repair_keeps_identity() {
    let mut s = session();
    create(&mut s, 10);
    let source = "/missing/original/texture.png".to_owned();
    let (_, p) = crate::support::prepared(&s, |c| {
        let mut a = c.read_document()?.asset(&id(10)).unwrap().clone();
        a.content = zipzi_core::state::AssetContent::Unresolved {
            source: source.clone(),
        };
        c.replace_asset(a)
    })
    .unwrap();
    s.publish(p).unwrap();
    let restored = crate::support::roundtrip(s.snapshot());
    assert_eq!(
        restored.document().asset(&id(10)).unwrap().content,
        zipzi_core::state::AssetContent::Unresolved { source }
    );
    assert_eq!(
        s.snapshot()
            .resources()
            .read_texture(s.snapshot().document().asset(&id(10)).unwrap())
            .unwrap_err()
            .code,
        "MISSING_RESOURCE"
    );
    replace_image(&mut s, 10, Some(content()));
    assert_eq!(pixels(s.snapshot(), &id(10)).width, 2);
}

#[test]
fn cdi_part_repair_and_explicit_missing_reference_removal_are_nonvisual() {
    let mut s = session();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_part({
                    let mut value = Part {
                        id: id(20),
                        name: "Current".into(),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                let mut info = e.read_document().unwrap().display_info().clone();
                info.parts = Some(vec![CdiPartEntry::Unresolved {
                    runtime_id: "LostPart".into(),
                    name: "Imported".into(),
                    extensions: Default::default(),
                }]);
                e.replace_display_info(info)
            })
            .and_then(|value| {
                let prepared = draft.prepare("Imported CDI Part")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let eval = s.snapshot().content_id();
    let p = crate::support::prepared(&s, |c| {
        c.edit_document(D::RepairCdiPart {
            runtime_id: "LostPart".into(),
            part_id: Some(id(20)),
        })
    })
    .unwrap();
    assert_eq!(p.0.losses.len(), 1);
    s.publish(p.1).unwrap();
    assert_ne!(s.snapshot().content_id(), eval);
    s.undo(s.version()).unwrap();
    let p = crate::support::prepared(&s, |c| {
        c.edit_document(D::RepairCdiPart {
            runtime_id: "LostPart".into(),
            part_id: None,
        })
    })
    .unwrap();
    s.publish(p.1).unwrap();
    assert!(s
        .snapshot()
        .document()
        .display_info()
        .parts
        .as_ref()
        .unwrap()
        .is_empty());
    assert!(s.snapshot().document().part(&id(20)).is_some());
}

#[test]
fn invalid_load_kind_does_not_cancel_an_image_request_and_default_paths_are_unique() {
    let mut s = session();
    create(&mut s, 10);
    let (_, p) = crate::support::prepared(&s, |c| c.replace_image(&id(10), content())).unwrap();
    assert!(crate::support::prepared(&s, |c| c
        .edit_attachment(AttachmentEdit::Delete { id: id(10) }))
    .is_err());
    s.publish(p).unwrap();
    for n in [50, 51] {
        let (_, p) = crate::support::prepared(&s, |c| {
            c.edit_attachment(AttachmentEdit::Create {
                id: id(n),
                path: String::new(),
            })
        })
        .unwrap();
        s.publish(p).unwrap();
    }
    assert_ne!(
        s.snapshot().document().attachment(&id(50)).unwrap().path,
        s.snapshot().document().attachment(&id(51)).unwrap().path
    );
    assert!(s
        .snapshot()
        .document()
        .attachment(&id(50))
        .unwrap()
        .bytes
        .is_none());
}

#[test]
fn prepared_image_loads_own_pixels_without_file_io_during_publication() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("image.png");
    std::fs::write(&path, content().read_asset().unwrap().bytes).unwrap();
    let image = zipzi_project::ResourceStore::default()
        .import_image(&path)
        .unwrap();
    std::fs::remove_file(path).unwrap();
    let mut s = session();
    create(&mut s, 10);
    replace_image(&mut s, 10, Some(image));
    let captured = s.read_snapshot();
    let before = pixels(s.snapshot(), &id(10));
    replace_image(&mut s, 10, None);
    drop(s);
    assert_eq!(pixels(&captured.snapshot, &id(10)).rgba, before.rgba);
}

#[test]
fn direct_prepared_import_is_one_atomic_resource_and_content_publication() {
    let mut s = session();
    let before = s.version();
    let (_, p) = crate::support::prepared(&s, |c| {
        c.create_empty_asset(id(10), "Imported".into())?;
        c.replace_image(&id(10), content())
    })
    .unwrap();
    let receipt = s.publish(p).unwrap();
    assert!(!receipt.writes.is_empty());
    assert_eq!(s.version().revision, before.revision + 1);
    let image = pixels(s.snapshot(), &id(10));
    assert_eq!(image.width, 2);
    assert!(
        crate::support::prepared(&s, |c| c.create_empty_asset(id(10), "Duplicate".into())).is_err()
    );
    s.undo(s.version()).unwrap();
    assert!(s.snapshot().document().asset(&id(10)).is_none());
    s.redo(s.version()).unwrap();
    assert_eq!(pixels(s.snapshot(), &id(10)).rgba, image.rgba);
}

#[test]
fn staged_image_load_checks_cancellation_and_version_again_at_commit() {
    let mut s = session();
    create(&mut s, 10);
    let mut draft = s.begin_draft();
    draft
        .batch(draft.source(), |c| c.replace_image(&id(10), content()))
        .unwrap();
    assert_eq!(
        pixels(&draft.read_snapshot().unwrap().snapshot, &id(10)).width,
        2
    );
    assert!(matches!(
        s.snapshot().document().asset(&id(10)).unwrap().content,
        zipzi_core::state::AssetContent::Empty
    ));
    let p = draft.prepare("image").unwrap();
    run(
        &mut s,
        R::PatchName {
            id: id(10),
            expected: String::new(),
            name: "Renamed".into(),
        },
    );
    assert_eq!(s.publish(p).unwrap_err().code, "STALE_SOURCE");
    replace_image(&mut s, 10, Some(content()));
    assert_eq!(
        s.snapshot().document().asset(&id(10)).unwrap().name,
        "Renamed"
    );
}

#[test]
fn committing_an_unchanged_staged_load_consumes_the_request_without_history() {
    let mut s = session();
    create(&mut s, 10);
    replace_image(&mut s, 10, Some(content()));
    let snapshot = s.snapshot().clone();
    let zipzi_core::state::AssetContent::Image { resource, .. } =
        snapshot.document().asset(&id(10)).unwrap().content
    else {
        panic!()
    };
    let image = snapshot.resources().image(resource).unwrap().clone();
    let version = s.version();
    let history = s.history_state();
    let (_, p) = crate::support::prepared(&s, |c| c.replace_image(&id(10), image)).unwrap();
    assert!(s.publish(p).unwrap().writes.is_empty());
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
}

#[test]
fn stale_resource_plan_aborts_all_prior_candidate_changes() {
    let mut s = session();
    let mut draft = s.begin_draft();
    let source = draft.source();
    draft
        .batch(source, |c| c.create_empty_asset(id(10), "First".into()))
        .unwrap();
    assert_eq!(
        draft
            .batch(source, |c| c.create_empty_asset(id(30), "Late".into()))
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert!(draft.prepare("failed").is_err());
    assert!(s.snapshot().document().asset_order().is_empty());
    assert!(s.take_events().is_empty());
}

fn content() -> zipzi_project::ImageResource {
    zipzi_project::ResourceStore::default()
        .import_image(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/asymmetric-2x2.png"),
        )
        .unwrap()
}
fn pixels(s: &zipzi_sdk::Snapshot, id: &str) -> zipzi_project::TextureData {
    s.resources()
        .read_texture(s.document().asset(id).unwrap())
        .unwrap()
}
fn replace_image(s: &mut AuthoringSession, n: u32, image: Option<zipzi_project::ImageResource>) {
    let (_, change) = crate::support::prepared(s, |c| match image {
        Some(image) => c.replace_image(&id(n), image),
        None => {
            let mut asset = c.read_document()?.asset(&id(n)).unwrap().clone();
            asset.content = zipzi_core::state::AssetContent::Empty;
            c.replace_asset(asset)
        }
    })
    .unwrap();
    s.publish(change).unwrap();
}

fn attach(s: &mut AuthoringSession, n: u32) {
    let (_, p) = crate::support::prepared(s, |c| {
        c.edit_attachment(AttachmentEdit::Create {
            id: id(n),
            path: format!("files/{n}.bin"),
        })
    })
    .unwrap();
    s.publish(p).unwrap();
}
fn lock(s: &mut AuthoringSession, n: u32) {
    let (_, p) = crate::support::prepared(s, |c| {
        let mut state = c.read_document()?.editor_state().clone();
        state.objects.entry(id(n)).or_default().locked = true;
        c.replace_editor_state(state)
    })
    .unwrap();
    s.publish(p).unwrap();
}

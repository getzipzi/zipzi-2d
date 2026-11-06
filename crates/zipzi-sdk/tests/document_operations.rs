use zipzi_core::DrawableFrameBounds;

use zipzi_contract::authoring::ObjectKind;
use zipzi_core::{Canvas, Vec2};
use zipzi_sdk::{rectangle_mesh, AuthoringSession, Snapshot};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn session() -> AuthoringSession {
    let mut sdk = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(1),
                (Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = crate::support::image_fixture(&id(2), "texture", &path).unwrap();
    let mesh = rectangle_mesh(
        &id(3),
        "mesh",
        &id(2),
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                crate::support::insert_image(edit, asset)?;
                crate::support::insert_mesh(edit, mesh)
            })
            .and_then(|value| {
                let prepared = draft.prepare("fixture")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    sdk
}

use zipzi_core::document::{DocumentAuthoringEdit, DocumentValue, ResourceEdit, SceneEdit};
fn patch(c: &mut zipzi_sdk::Commands<'_>, value: DocumentValue) -> Result<(), zipzi_sdk::Status> {
    let expected = c.read_document()?.document_value(value.field())?;
    c.edit_document(DocumentAuthoringEdit::Patch { expected, value })?;
    Ok(())
}
fn publish<T>(
    s: &mut AuthoringSession,
    f: impl FnOnce(&mut zipzi_sdk::Commands<'_>) -> Result<T, zipzi_sdk::Status>,
) -> Result<zipzi_sdk::Publication, zipzi_sdk::Status> {
    let (_, p) = crate::support::prepared(s, f)?;
    s.publish(p)
}
#[test]
fn boundless_canvas_supports_history_and_rejects_invalid_coordinates() {
    let mut s = AuthoringSession::new(
        Snapshot::new(
            zipzi_core::state::DocumentState::new(&id(1), Default::default()).unwrap(),
            Default::default(),
        )
        .unwrap(),
    );
    assert!(s.snapshot().document().setup().source_canvas.is_none());
    let settings = zipzi_core::ExportSettings {
        pixels_per_unit: 17.,
        ..Default::default()
    };
    let before = s.snapshot().content_id();
    publish(&mut s, |c| {
        patch(c, DocumentValue::ExportSettings(settings))
    })
    .unwrap();
    assert_ne!(s.snapshot().content_id(), before);
    assert_eq!(s.snapshot().document().setup().export, settings);
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().setup().export, Default::default());
    s.redo(s.version()).unwrap();
    for invalid in [
        Canvas::new(-1., 0., Vec2::default(), 1.),
        Canvas::new(0., f32::INFINITY, Vec2::default(), 1.),
        Canvas::new(0., 0., Vec2::new(f32::NAN, 0.), 1.),
        Canvas::new(0., 0., Vec2::default(), 0.),
    ] {
        assert!(zipzi_core::state::DocumentState::new(&id(3), invalid.into()).is_err());
    }
}
#[test]
fn canvas_and_draw_order_publish_together_and_reject_invalid_ranges() {
    let mut s = session();
    let mut settings = s.snapshot().document().setup().export;
    settings.pixels_per_unit = 20.;
    let version = s.version();
    let groups = s.snapshot().document().draw_order_groups().clone();
    publish(&mut s, |c| {
        patch(c, DocumentValue::ExportSettings(settings))?;
        c.edit_scene(
            SceneEdit::SetDrawingRange {
                owner: String::new(),
                expected: [0, 0],
                range: [-100, 100],
            },
            Default::default(),
            &Default::default(),
        )?;
        Ok(())
    })
    .unwrap();
    assert_eq!(s.version().revision, version.revision + 1);
    assert_eq!(s.snapshot().document().setup().export, settings);
    assert!(s.snapshot().document().draw_order_groups().is_some());
    assert_eq!(
        crate::support::frame(s.snapshot(), &Default::default())
            .unwrap()
            .drawables[0]
            .positions[0]
            .x,
        40.
    );
    let group = s
        .snapshot()
        .document()
        .draw_order_groups()
        .as_ref()
        .unwrap()[0]
        .clone();
    let version = s.version();
    assert!(publish(&mut s, |c| c.edit_scene(
        SceneEdit::SetDrawingRange {
            owner: String::new(),
            expected: [group.min_order, group.max_order],
            range: [10, -10]
        },
        Default::default(),
        &Default::default()
    ))
    .is_err());
    assert_eq!(s.version(), version);
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .setup()
            .source_canvas
            .unwrap()
            .pixels_per_unit,
        10.
    );
    assert_eq!(s.snapshot().document().draw_order_groups(), &groups);
}
#[test]
fn referenced_delete_rejects_and_recreated_id_does_not_revive_a_source() {
    let mut s = session();
    let mesh = s.snapshot().document().mesh(&id(3)).unwrap().clone();
    let stale = s.begin_draft();
    assert_eq!(s.snapshot().document().references_to(&id(2)), vec![id(3)]);
    let before = s.read_snapshot().source;
    assert!(publish(&mut s, |c| c.edit_resource(ResourceEdit::Delete {
        id: id(2),
        replacement: None
    }))
    .is_err());
    assert_eq!(s.read_snapshot().source, before);
    publish(&mut s, |c| {
        c.edit_scene(
            SceneEdit::Delete {
                ids: vec![id(3)],
                scope: Default::default(),
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    assert!(s.snapshot().document().mesh(&id(3)).is_none());
    publish(&mut s, |c| crate::support::insert_mesh(c, mesh)).unwrap();
    assert!(s
        .snapshot()
        .document()
        .contains_object(ObjectKind::Mesh, &id(3)));
    assert_eq!(
        s.publish(stale.prepare("old source").unwrap())
            .unwrap_err()
            .code
            .as_str(),
        "STALE_SOURCE"
    );
    s.undo(s.version()).unwrap();
    assert!(s.snapshot().document().mesh(&id(3)).is_none());
    s.undo(s.version()).unwrap();
    assert!(s.snapshot().document().mesh(&id(3)).is_some());
}
#[test]
fn same_batch_recreation_preserves_records_and_materializes_drawing_order() {
    let mut s = session();
    let mesh = s.snapshot().document().mesh(&id(3)).unwrap().clone();
    let before = s.read_snapshot().source;
    let history = s.history_state();
    let receipt = publish(&mut s, |c| {
        c.edit_scene(
            SceneEdit::Delete {
                ids: vec![id(3)],
                scope: Default::default(),
            },
            Default::default(),
            &Default::default(),
        )?;
        crate::support::insert_mesh(c, mesh)
    })
    .unwrap();
    assert!(receipt.writes.records().is_empty());
    assert!(receipt
        .writes
        .root_changed(zipzi_core::state::RootDomain::DrawOrderGroups));
    assert_ne!(s.read_snapshot().source, before);
    assert_eq!(s.history_state().undo_steps, history.undo_steps + 1);
    s.undo(s.version()).unwrap();
    assert!(s.snapshot().document().draw_order_groups().is_none());
}
#[test]
fn quality_warnings_are_separate_from_structural_validity() {
    let s = session();
    let reference = s.snapshot().quality_reference().unwrap();
    let mut deformed = reference.clone();
    deformed.drawables[0].positions.swap(1, 2);
    let before = s.read_snapshot().source;
    let quality = zipzi_sdk::mesh_quality(&deformed, &reference, &Default::default()).unwrap();
    assert!(quality[0].flipped_triangles > 0);
    assert!(s.snapshot().document().builder().freeze().is_ok());
    assert_eq!(s.read_snapshot().source, before);
    let mut options = zipzi_sdk::TopologyOverlay::default();
    options.heat_opacity = -1.;
    assert!(zipzi_sdk::TopologyOverlayValidation::validate(&options).is_err());
}
#[test]
fn quality_sampling_evaluates_disabled_meshes_and_parent_parts() {
    for parent in [false, true] {
        let mut s = session();
        let visible = s.snapshot().quality_reference().unwrap();
        let mut mesh = s.snapshot().document().mesh(&id(3)).unwrap().clone();
        publish(&mut s, |c| {
            if parent {
                c.create_part(zipzi_core::Part {
                    id: id(4),
                    runtime_id: id(4),
                    enabled: false,
                    ..Default::default()
                })?;
                mesh.part_id = id(4);
            } else {
                mesh.enabled = false;
            }
            c.replace_mesh(mesh)
        })
        .unwrap();
        let sampled = s.snapshot().quality_reference().unwrap();
        let q = zipzi_sdk::mesh_quality(&sampled, &visible, &Default::default()).unwrap();
        assert_eq!(
            sampled.drawables[0].positions,
            visible.drawables[0].positions
        );
        assert!(!q[0].visible);
        assert_eq!(q[0].degenerate_triangles, 0);
        assert_eq!(q[0].flipped_triangles, 0);
        assert_eq!(q[0].max_edge_stretch, Some(1.));
        assert!(crate::support::frame(s.snapshot(), &Default::default())
            .unwrap()
            .drawables[0]
            .positions
            .iter()
            .all(|p| *p == Vec2::default()));
    }
}
#[test]
fn replacement_failure_preserves_session_and_success_changes_generation() {
    let mut s = session();
    let old = s.read_snapshot();
    let mut version = s.version();
    version.revision += 1;
    let new = Snapshot::new(
        zipzi_core::state::DocumentState::new(&id(1), Default::default()).unwrap(),
        Default::default(),
    )
    .unwrap();
    assert!(s.replace(version, new.clone()).is_err());
    assert_eq!(s.read_snapshot().source, old.source);
    s.take_events();
    let before = s.version();
    let next = s.replace(before, new).unwrap();
    assert_eq!(next.version.session, before.session);
    assert_eq!(next.version.generation, before.generation + 1);
    assert!(s.snapshot().document().mesh_order().is_empty());
    assert_eq!(s.history_state().undo_steps, 0);
    assert_eq!(s.history_state().redo_steps, 0);
    assert_eq!(s.take_events().len(), 1);
    assert_eq!(
        crate::support::frame(&old.snapshot, &Default::default())
            .unwrap()
            .drawables
            .len(),
        1
    );
}
#[test]
fn source_framing_is_metadata_and_never_changes_model_bounds_or_evaluation() {
    let mut s = session();
    let source = s.read_snapshot();
    let before = crate::support::frame(s.snapshot(), &Default::default()).unwrap();
    let export = s.snapshot().document().setup().export;
    for canvas in [
        Some(Canvas::new(4000., 3000., Vec2::new(-800., 700.), 123.)),
        Some(Canvas::new(4000., 3000., Vec2::new(-800., 700.), 20.)),
        None,
    ] {
        assert_eq!(
            publish(&mut s, |c| patch(c, DocumentValue::SourceCanvas(canvas)))
                .unwrap_err()
                .code
                .as_str(),
            "SOURCE_METADATA_READ_ONLY"
        );
        assert_eq!(s.read_snapshot().source, source.source);
        let mut builder = source.snapshot.document().builder();
        let mut setup = builder.setup();
        setup.source_canvas = canvas;
        builder.set_setup(setup).unwrap();
        let snapshot = Snapshot::new(
            builder.freeze().unwrap().state,
            source.snapshot.resources().clone(),
        )
        .unwrap();
        assert_eq!(snapshot.document().setup().export, export);
        let frame = crate::support::frame(&snapshot, &Default::default()).unwrap();
        assert_eq!(frame.model_bounds(false), Some([40., 40., 60., 60.]));
        assert_eq!(frame.drawables[0].positions, before.drawables[0].positions);
    }
}

#[test]
fn model_space_and_source_reference_are_separate_project_fields() {
    let mut s = session();
    publish(&mut s, |c| {
        patch(
            c,
            DocumentValue::ExportSettings(zipzi_core::ExportSettings {
                pixels_per_unit: 20.,
                ..Default::default()
            }),
        )
    })
    .unwrap();
    let bytes = crate::support::archive::native_bytes_with_resources(
        s.snapshot().document(),
        s.snapshot().resources(),
    );
    let wire = crate::support::archive::project_json(&bytes);
    assert!(wire["document"].get("canvas").is_none());
    assert!(wire["document"].get("model_space").is_none());
    assert_eq!(wire["document"]["export_settings"]["pixels_per_unit"], 20.);
    assert_eq!(wire["document"]["source_canvas"]["pixels_per_unit"], 10.);
    assert_eq!(wire["document"]["source_canvas"]["origin"]["x"], 50.);
    let loaded = crate::support::roundtrip(s.snapshot());
    assert_eq!(loaded.document().setup(), s.snapshot().document().setup());
}
#[test]
fn scalar_preview_rejections_preserve_candidate_frame_and_transaction() {
    let mut s = session();
    publish(&mut s, |c| {
        c.create_parameter(zipzi_core::Parameter {
            id: id(8),
            runtime_id: "P".into(),
            minimum: -1.,
            maximum: 1.,
            ..Default::default()
        })
    })
    .unwrap();
    let before = s.version();
    let mut draft = s.begin_draft();
    let patch = |value| zipzi_core::state::PropertyPatch::Parameter {
        id: id(8),
        minimum: -1.,
        maximum: 1.,
        default_value: value,
        decimal_places: 6,
    };
    let (p, _) = draft
        .prepare_preview(draft.source(), |c| c.patch_property(patch(0.25)))
        .unwrap();
    draft.install_preview(p).unwrap();
    let first = draft.read_snapshot().unwrap();
    let first_frame = crate::support::frame(&first.snapshot, &Default::default()).unwrap();
    let (rejected, _) = draft
        .prepare_preview(draft.source(), |c| c.patch_property(patch(0.5)))
        .unwrap();
    assert!(
        crate::support::frame(&rejected.read_snapshot().unwrap(), &[(id(99), 0.)].into()).is_err()
    );
    assert_eq!(draft.read_snapshot().unwrap().source, first.source);
    assert!(draft
        .prepare_preview(draft.source(), |c| c.patch_property(patch(2.)))
        .is_err());
    let (p, _) = draft
        .prepare_preview(draft.source(), |c| c.patch_property(patch(0.75)))
        .unwrap();
    draft.install_preview(p).unwrap();
    assert_eq!(first_frame.parameters[0].value, 0.25);
    assert_eq!(
        crate::support::frame(
            &draft.read_snapshot().unwrap().snapshot,
            &Default::default()
        )
        .unwrap()
        .parameters[0]
            .value,
        0.75
    );
    s.publish(draft.prepare("drag").unwrap()).unwrap();
    assert_eq!(s.version().revision, before.revision + 1);
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .parameter(&id(8))
            .unwrap()
            .default_value,
        0.
    );
}

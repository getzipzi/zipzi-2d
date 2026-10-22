use std::sync::Arc;

use kasane_core::{draw_order::DrawOrderGroup, Canvas, Vec2};
use kasane_sdk::{
    prepare_png_asset, rectangle_mesh, AuthoringSession, GeometryBounds, GeometryChecks,
    GeometryDiagnosticKind, ObjectKind,
};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn session() -> AuthoringSession {
    let mut sdk = AuthoringSession::new(
        &id(1),
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = prepare_png_asset(&id(2), "texture", &path).unwrap();
    let mesh = rectangle_mesh(
        &id(3),
        "mesh",
        &id(2),
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    sdk.edit("fixture", None, |edit| {
        edit.create_asset(asset)?;
        edit.create_mesh(mesh)
    })
    .unwrap();
    sdk
}

#[test]
fn boundless_canvas_supports_history_and_rejects_invalid_coordinates() {
    let mut sdk = AuthoringSession::new(&id(1), kasane_core::ModelSetup::default()).unwrap();
    assert!(sdk.source_canvas().is_none());
    let partial = kasane_core::ExportSettings {
        pixels_per_unit: 17.,
        ..Default::default()
    };
    let revision = sdk.evaluation_revision();
    sdk.edit("output units", None, |edit| {
        edit.replace_export_settings(partial)
    })
    .unwrap();
    assert_eq!(sdk.export_settings(), partial);
    assert_eq!(sdk.evaluation_revision(), revision);
    sdk.undo().unwrap();
    assert_eq!(
        sdk.export_settings(),
        kasane_core::ExportSettings::default()
    );
    sdk.redo().unwrap();
    for invalid in [
        Canvas::new(-1.0, 0.0, Vec2::default(), 1.0),
        Canvas::new(0.0, f32::INFINITY, Vec2::default(), 1.0),
        Canvas::new(0.0, 0.0, Vec2::new(f32::NAN, 0.0), 1.0),
        Canvas::new(0.0, 0.0, Vec2::default(), 0.0),
    ] {
        assert!(AuthoringSession::new(&id(3), invalid).is_err());
    }
}

#[test]
fn canvas_and_draw_order_publish_together_and_reject_invalid_groups() {
    let mut sdk = session();
    let mut canvas = sdk.source_canvas().unwrap();
    canvas.pixels_per_unit = 20.0;
    let groups = vec![DrawOrderGroup {
        owner: String::new(),
        items: vec![id(3)],
        min_order: -100,
        max_order: 100,
    }];
    let before = sdk.version();
    sdk.edit("drawing", Some(before), |edit| {
        edit.replace_model_setup(canvas)?;
        edit.replace_draw_order_groups(groups.clone())
    })
    .unwrap();
    assert_eq!(sdk.version().revision, before.revision + 1);
    assert!(sdk.validate_structure().is_empty());
    assert_eq!(sdk.source_canvas().unwrap().pixels_per_unit, 20.0);
    assert_eq!(sdk.draw_order_groups(), Some(groups));
    assert_eq!(
        sdk.evaluate(&Default::default()).unwrap().drawables[0].positions[0].x,
        40.
    );

    let version = sdk.version();
    let mut edit = sdk.begin_edit("bad groups", None).unwrap();
    assert_eq!(
        edit.replace_draw_order_groups(vec![DrawOrderGroup {
            owner: String::new(),
            items: vec![],
            min_order: 0,
            max_order: 0,
        }])
        .unwrap_err()
        .code
        .as_ref(),
        "INVALID_DRAW_GROUP"
    );
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    assert_eq!(sdk.version(), version);
    sdk.undo().unwrap();
    assert_eq!(sdk.source_canvas().unwrap().pixels_per_unit, 10.0);
    assert!(sdk.draw_order_groups().is_none());
}

#[test]
fn referenced_delete_is_rejected_and_recreated_id_does_not_revive_handle() {
    let mut sdk = session();
    let mesh = sdk.mesh(&id(3)).unwrap();
    let old_handle = sdk.handle(ObjectKind::Mesh, &id(3)).unwrap();
    assert_eq!(sdk.references_to(&id(2)), vec![id(3)]);
    let version = sdk.version();
    let mut edit = sdk.begin_edit("bad erase", None).unwrap();
    let error = edit.erase_object(&id(2)).unwrap_err();
    assert_eq!(error.code.as_ref(), "OBJECT_REFERENCED");
    assert_eq!(&*error.referrers, &[id(3)]);
    assert_eq!(edit.commit().unwrap_err().code.as_ref(), "EDIT_ABORTED");
    assert_eq!(sdk.version(), version);

    sdk.edit("erase", None, |edit| edit.erase_object(&id(3)))
        .unwrap();
    assert!(sdk.mesh(&id(3)).is_none());
    assert_eq!(
        sdk.resolve_handle(&old_handle).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    sdk.edit("recreate", None, |edit| edit.create_mesh(mesh))
        .unwrap();
    assert_eq!(sdk.mesh_ids(), &[id(3)]);
    assert_eq!(
        sdk.resolve_handle(&old_handle).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    let current = sdk.handle(ObjectKind::Mesh, &id(3)).unwrap();
    assert_ne!(current, old_handle);
    sdk.undo().unwrap();
    assert!(sdk.mesh(&id(3)).is_none());
    sdk.undo().unwrap();
    assert!(sdk.mesh(&id(3)).is_some());
    assert_eq!(
        sdk.resolve_handle(&old_handle).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
}

#[test]
fn identical_same_batch_recreation_is_an_identity_change() {
    let mut sdk = session();
    let old = sdk.handle(ObjectKind::Mesh, &id(3)).unwrap();
    let mesh = sdk.mesh(&id(3)).unwrap();
    let before = sdk.version();
    let (_, receipt) = sdk
        .edit("recreate", Some(before), |edit| {
            edit.erase_object(&id(3))?;
            edit.create_mesh(mesh)
        })
        .unwrap();
    assert!(receipt.changed);
    assert_eq!(receipt.after.revision, before.revision + 1);
    assert_eq!(sdk.mesh_ids(), &[id(3)]);
    assert_eq!(
        sdk.resolve_handle(&old).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    let current = sdk.handle(ObjectKind::Mesh, &id(3)).unwrap();
    sdk.undo().unwrap();
    assert_eq!(
        sdk.resolve_handle(&current).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    assert_eq!(
        sdk.resolve_handle(&old).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    sdk.redo().unwrap();
    assert_eq!(
        sdk.resolve_handle(&current).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
}

#[test]
fn geometry_warnings_are_separate_from_structural_validity() {
    let mut sdk = session();
    let mut mesh = sdk.mesh(&id(3)).unwrap();
    mesh.triangles[1] = [0, 3, 2];
    sdk.edit("flip triangle", None, |edit| edit.replace_mesh(mesh))
        .unwrap();
    assert!(sdk.validate_structure().is_empty());
    let version = sdk.version();
    let warnings = sdk.diagnose_geometry(GeometryChecks::default()).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        warnings[0].kind,
        GeometryDiagnosticKind::InconsistentWinding
    );
    assert_eq!(warnings[0].triangle_index, Some(1));

    let warnings = sdk
        .diagnose_geometry(GeometryChecks {
            min_triangle_area: 201.0,
            model_bounds: Some(GeometryBounds {
                min: Vec2::new(0.0, 0.0),
                max: Vec2::new(50.0, 50.0),
            }),
        })
        .unwrap();
    assert_eq!(
        warnings
            .iter()
            .filter(|issue| issue.kind == GeometryDiagnosticKind::SmallTriangle)
            .count(),
        2
    );
    assert!(warnings
        .iter()
        .any(|issue| issue.kind == GeometryDiagnosticKind::OutsideModelBounds));
    assert_eq!(
        sdk.diagnose_geometry(GeometryChecks {
            min_triangle_area: -1.0,
            model_bounds: None,
        })
        .unwrap_err()
        .code
        .as_ref(),
        "INVALID_DIAGNOSTIC_OPTIONS"
    );
    assert_eq!(sdk.version(), version);
}

#[test]
fn new_project_failure_preserves_session_and_success_changes_generation() {
    let mut sdk = session();
    let old = sdk.handle(ObjectKind::Mesh, &id(3)).unwrap();
    let frame = sdk.preview_frame().unwrap();
    let current = sdk.version();
    let mut invalid_canvas = sdk.source_canvas().unwrap();
    invalid_canvas.pixels_per_unit = 0.0;
    assert_eq!(
        sdk.new_project(&id(1), invalid_canvas, Some(current))
            .unwrap_err()
            .code
            .as_ref(),
        "INVALID_MODEL_SETUP"
    );
    let mut stale = current;
    stale.revision += 1;
    assert_eq!(
        sdk.new_project(&id(1), sdk.source_canvas().unwrap(), Some(stale))
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_VERSION"
    );
    assert_eq!(sdk.version(), current);
    sdk.resolve_handle(&old).unwrap();
    assert!(Arc::ptr_eq(&frame, &sdk.preview_frame().unwrap()));
    assert_eq!(sdk.history_lengths(), (1, 0));

    let next = sdk
        .new_project(&id(1), sdk.source_canvas().unwrap(), Some(current))
        .unwrap();
    assert_eq!(next.session_id, current.session_id);
    assert_eq!(next.generation, current.generation + 1);
    assert!(sdk.asset_ids().is_empty());
    assert!(sdk.mesh_ids().is_empty());
    assert_eq!(sdk.history_lengths(), (0, 0));
    assert!(sdk.preview_values().is_empty());
    let events = sdk.drain_events();
    assert_eq!(events.len(), 1);
    assert!(events[0].changes.full_invalidation);
    assert_eq!(
        sdk.resolve_handle(&old).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
}

#[test]
fn source_framing_is_metadata_and_never_changes_model_bounds_or_evaluation() {
    let mut sdk = session();
    let before = sdk.preview_frame().unwrap().clone();
    let revision = sdk.evaluation_revision();
    let space = sdk.export_settings();
    assert_eq!(before.model_bounds(false), Some([40., 40., 60., 60.]));
    sdk.edit("Source framing", None, |edit| {
        edit.replace_source_canvas(Some(Canvas::new(
            4000.,
            3000.,
            Vec2::new(-800., 700.),
            123.,
        )))
    })
    .unwrap();
    assert_eq!(sdk.export_settings(), space);
    assert_eq!(sdk.evaluation_revision(), revision);
    assert!(Arc::ptr_eq(&sdk.preview_frame().unwrap(), &before));
    let source = sdk.source_canvas().unwrap();
    sdk.edit("Cubism units metadata", None, |edit| {
        edit.replace_source_canvas(Some(Canvas {
            pixels_per_unit: 20.,
            ..source
        }))
    })
    .unwrap();
    assert_eq!(sdk.evaluation_revision(), revision);
    assert!(Arc::ptr_eq(&sdk.preview_frame().unwrap(), &before));
    assert_eq!(
        sdk.preview_frame().unwrap().model_bounds(false),
        before.model_bounds(false)
    );
    sdk.undo().unwrap();
    assert_eq!(sdk.source_canvas(), Some(source));
    assert_eq!(sdk.evaluation_revision(), revision);
    sdk.edit("Remove source reference", None, |edit| {
        edit.replace_source_canvas(None)
    })
    .unwrap();
    assert!(sdk.source_canvas().is_none());
    assert_eq!(
        sdk.preview_frame().unwrap().model_bounds(false),
        before.model_bounds(false)
    );
    sdk.undo().unwrap();
    assert_eq!(sdk.source_canvas(), Some(source));
}

#[test]
fn model_space_and_source_reference_are_separate_project_fields() {
    let mut sdk =
        AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::new(50., 50.), 10.)).unwrap();
    sdk.edit("Export", None, |edit| {
        edit.replace_export_settings(kasane_core::ExportSettings {
            pixels_per_unit: 20.,
            ..Default::default()
        })
    })
    .unwrap();
    let before = sdk.read_snapshot();
    let json = kasane_project::encode_project(before.document()).unwrap();
    let wire: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(wire["format_version"], 17);
    assert!(wire["document"].get("canvas").is_none());
    assert!(wire["document"].get("model_space").is_none());
    assert_eq!(wire["document"]["export_settings"]["pixels_per_unit"], 20.);
    assert_eq!(wire["document"]["source_canvas"]["pixels_per_unit"], 10.);
    assert_eq!(wire["document"]["source_canvas"]["origin"]["x"], 50.);
    let reopened = kasane_project::decode_project(&json).unwrap();
    assert!(reopened.same_content(before.document()));
}

#[test]
fn scalar_preview_rejections_preserve_candidate_frame_and_transaction() {
    use kasane_core::{document::PropertyPatch, preview::PreviewState, Parameter};
    let mut sdk = AuthoringSession::new(&id(1), kasane_core::ModelSetup::default()).unwrap();
    sdk.edit("parameter", None, |edit| {
        edit.create_parameter(Parameter {
            id: id(2),
            runtime_id: "P".into(),
            minimum: -1.,
            maximum: 1.,
            ..Default::default()
        })
    })
    .unwrap();
    let before = sdk.version();
    let mut edit = sdk.begin_owned_edit("drag", None).unwrap();
    let patch = |value| PropertyPatch::Parameter {
        id: id(2),
        minimum: -1.,
        maximum: 1.,
        default_value: value,
        decimal_places: 6,
    };
    let mut preview = PreviewState::default();
    edit.preview_property(&patch(0.25), &mut preview, Default::default())
        .unwrap();
    let frame = preview.frame(edit.candidate_document(), 0).unwrap();
    let revision = edit.candidate_document().revision();
    // Mutation succeeds, evaluation fails: undo just this update, including its revision.
    assert!(edit
        .preview_property(&patch(0.5), &mut preview, [(id(99), 0.)].into())
        .is_err());
    assert_eq!(edit.candidate_document().revision(), revision);
    assert_eq!(
        edit.candidate_document()
            .get_parameter(&id(2))
            .unwrap()
            .default_value,
        0.25
    );
    assert!(Arc::ptr_eq(
        &frame,
        &preview.frame(edit.candidate_document(), 0).unwrap()
    ));
    // A validation failure must also keep this same transaction usable.
    assert!(edit
        .preview_property(&patch(2.), &mut preview, Default::default())
        .is_err());
    edit.preview_property(&patch(0.75), &mut preview, Default::default())
        .unwrap();
    assert_eq!(frame.parameters[0].value, 0.25);
    assert_eq!(
        preview
            .frame(edit.candidate_document(), 0)
            .unwrap()
            .parameters[0]
            .value,
        0.75
    );
    edit.commit_to(&mut sdk).unwrap();
    assert_eq!(sdk.version().revision, before.revision + 1);
    sdk.undo().unwrap();
    assert_eq!(
        sdk.document().get_parameter(&id(2)).unwrap().default_value,
        0.
    );
}

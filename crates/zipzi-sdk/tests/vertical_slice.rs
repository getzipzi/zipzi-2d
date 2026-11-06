use zipzi_core::document::{DocumentAuthoringEdit, DocumentValue};
use zipzi_core::{Canvas, PreviewValues, Vec2};
use zipzi_sdk::{rectangle_mesh, AuthoringSession};

const DOCUMENT_ID: &str = "00000000-0000-4000-8000-000000000001";
const ASSET_ID: &str = "00000000-0000-4000-8000-000000000002";
const MESH_ID: &str = "00000000-0000-4000-8000-000000000003";
fn fixture_png() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png")
}

fn session() -> AuthoringSession {
    AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                DOCUMENT_ID,
                (Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    )
}

#[test]
fn draft_isolated_until_publication_and_checks_source_version() {
    let mut sdk = session();
    let original = sdk.snapshot().document().setup().export;
    let resized = zipzi_core::ExportSettings {
        pixels_per_unit: 20.,
        ..original
    };
    let prepare = |s: &AuthoringSession, expected, value| {
        crate::support::prepared(s, |c| {
            c.edit_document(DocumentAuthoringEdit::Patch {
                expected: DocumentValue::ExportSettings(expected),
                value: DocumentValue::ExportSettings(value),
            })
        })
        .unwrap()
        .1
    };
    let draft = prepare(&sdk, original, resized);
    assert_eq!(draft.snapshot().document().setup().export, resized);
    assert_eq!(sdk.snapshot().document().setup().export, original);
    sdk.publish(draft).unwrap();
    assert_eq!(sdk.history_state().undo_steps, 1);
    let stale = prepare(&sdk, resized, original);
    sdk.undo(sdk.version()).unwrap();
    assert_eq!(sdk.publish(stale).unwrap_err().code, "STALE_SOURCE");
    assert_eq!(sdk.snapshot().document().setup().export, original);
}

#[test]
fn shared_authoring_contract_matches_expected_results() {
    let spec: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/authoring-contract.json")).unwrap();
    let text = |field: &str| spec[field].as_str().unwrap();
    let point = |field: &str| {
        Vec2::new(
            spec[field][0].as_f64().unwrap() as f32,
            spec[field][1].as_f64().unwrap() as f32,
        )
    };
    let mut sdk = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                text("document_id"),
                (Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    let invalid = rectangle_mesh(
        text("mesh_id"),
        text("initial_name"),
        text("missing_asset_id"),
        point("minimum"),
        point("maximum"),
    )
    .unwrap();
    let before = sdk.version();
    let failure = {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| edit.create_mesh(invalid))
            .and_then(|value| {
                let prepared = draft.prepare("invalid")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap_err();
    assert_eq!(failure.code.as_str(), text("invalid_create_code"));
    assert_eq!(sdk.version(), before);
    assert_eq!(
        {
            let history = sdk.history_state();
            (history.undo_steps, history.redo_steps)
        },
        (0, 0)
    );

    let asset = crate::support::image_fixture(text("asset_id"), "texture", &fixture_png()).unwrap();
    let mesh = rectangle_mesh(
        text("mesh_id"),
        text("initial_name"),
        text("asset_id"),
        point("minimum"),
        point("maximum"),
    )
    .unwrap();
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                crate::support::insert_image(edit, asset)?;
                edit.create_mesh(mesh)
            })
            .and_then(|value| {
                let prepared = draft.prepare("create")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let frame = crate::support::frame(sdk.snapshot(), &PreviewValues::new()).unwrap();
    assert_eq!(
        frame.drawables[0].positions[0],
        point("evaluated_first_position")
    );
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.rename(
                    zipzi_sdk::NameTarget::Mesh(text("mesh_id")),
                    text("updated_name").into(),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("rename")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(
        sdk.snapshot()
            .document()
            .mesh(text("mesh_id"))
            .cloned()
            .unwrap()
            .name,
        text("updated_name")
    );
    sdk.undo(sdk.version()).unwrap();
    assert_eq!(
        sdk.snapshot()
            .document()
            .mesh(text("mesh_id"))
            .cloned()
            .unwrap()
            .name,
        text("initial_name")
    );
    sdk.redo(sdk.version()).unwrap();
    assert_eq!(
        sdk.snapshot()
            .document()
            .mesh(text("mesh_id"))
            .cloned()
            .unwrap()
            .name,
        text("updated_name")
    );
}

#[test]
fn creates_png_rectangle_evaluates_and_undoes_all_content() {
    let path = fixture_png();
    let asset = crate::support::image_fixture(ASSET_ID, "asymmetric", &path).unwrap();
    assert_eq!((asset.image.width(), asset.image.height()), (2, 2));
    let key = asset.image.key();
    let mesh = rectangle_mesh(
        MESH_ID,
        "face",
        ASSET_ID,
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    let mut sdk = session();
    let before = sdk.version();
    let (_, prepared) = crate::support::prepared(&sdk, |c| {
        crate::support::insert_image(c, asset)?;
        c.create_mesh(mesh)
    })
    .unwrap();
    let receipt = sdk.publish(prepared).unwrap();
    assert_eq!(receipt.version.revision, before.revision + 1);
    assert_eq!(sdk.snapshot().document().asset_order(), &[ASSET_ID]);
    assert!(
        matches!(sdk.snapshot().document().asset(ASSET_ID).unwrap().content, zipzi_core::state::AssetContent::Image { resource, .. } if resource == key)
    );
    assert_eq!(sdk.snapshot().document().mesh_order(), &[MESH_ID]);
    assert_eq!(
        sdk.snapshot()
            .document()
            .mesh(MESH_ID)
            .cloned()
            .unwrap()
            .triangles,
        vec![[0, 1, 2], [0, 2, 3]]
    );
    assert_eq!(
        sdk.snapshot().document().mesh(MESH_ID).unwrap().vertex_ids,
        [0, 1, 2, 3]
    );
    let frame = crate::support::frame(sdk.snapshot(), &PreviewValues::new()).unwrap();
    assert_eq!(frame.drawables.len(), 1);
    assert_eq!(frame.drawables[0].positions[0], Vec2::new(40.0, 40.0));
    assert_eq!(frame.drawables[0].positions[2], Vec2::new(60.0, 60.0));

    let geometry = sdk.snapshot().document().mesh(MESH_ID).cloned().unwrap();
    let moved: Vec<_> = geometry
        .base_positions
        .iter()
        .map(|p| Vec2::new(p.x + 10.0, p.y))
        .collect();
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.set_vertex_positions(
                    MESH_ID,
                    &geometry
                        .vertex_ids
                        .iter()
                        .copied()
                        .zip(moved.iter().copied())
                        .collect::<Vec<_>>(),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("move")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(
        crate::support::frame(sdk.snapshot(), &PreviewValues::new())
            .unwrap()
            .drawables[0]
            .positions[0],
        Vec2::new(50.0, 40.0)
    );
    assert_eq!(
        {
            let history = sdk.history_state();
            (history.undo_steps, history.redo_steps)
        },
        (2, 0)
    );
    sdk.undo(sdk.version()).unwrap();
    assert_eq!(
        sdk.snapshot()
            .document()
            .mesh(MESH_ID)
            .cloned()
            .unwrap()
            .base_positions,
        geometry.base_positions
    );
    sdk.undo(sdk.version()).unwrap();
    assert!(sdk.snapshot().document().mesh_order().is_empty());
    assert!(sdk.snapshot().document().asset_order().is_empty());
    sdk.redo(sdk.version()).unwrap();
    sdk.redo(sdk.version()).unwrap();
    assert_eq!(
        sdk.snapshot()
            .document()
            .mesh(MESH_ID)
            .cloned()
            .unwrap()
            .base_positions,
        moved
    );
    assert_eq!(sdk.take_events().len(), 6);
}

#[test]
fn failed_or_abandoned_edit_never_publishes_or_clears_redo() {
    let mut sdk = session();
    let before = sdk.version();
    let invalid = rectangle_mesh(
        MESH_ID,
        "orphan",
        ASSET_ID,
        Vec2::new(0.0, 0.0),
        Vec2::new(1.0, 1.0),
    )
    .unwrap();
    let mut draft = sdk.begin_draft();
    assert_eq!(
        draft
            .batch(draft.source(), |c| c.create_mesh(invalid))
            .unwrap_err()
            .code,
        "MISSING_ASSET"
    );
    assert!(!draft.is_active());
    assert_eq!(sdk.version(), before);
    assert_eq!(
        {
            let history = sdk.history_state();
            (history.undo_steps, history.redo_steps)
        },
        (0, 0)
    );
    assert!(sdk.take_events().is_empty());

    let path = fixture_png();
    let asset = crate::support::image_fixture(ASSET_ID, "fixture", &path).unwrap();
    let mut draft = sdk.begin_draft();
    draft
        .batch(draft.source(), |c| {
            crate::support::insert_image(
                c,
                crate::support::image_fixture(ASSET_ID, "abandoned", &path)?,
            )
        })
        .unwrap();
    draft.cancel();
    assert!(sdk.snapshot().document().asset_order().is_empty());
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                crate::support::insert_image(edit, asset)
            })
            .and_then(|value| {
                let prepared = draft.prepare("asset")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    sdk.undo(sdk.version()).unwrap();
    let redo_version = sdk.version();
    let (_, noop) = {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |_edit| Ok(()))
            .and_then(|value| {
                let prepared = draft.prepare("noop")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(noop.writes.is_empty());
    assert_eq!(sdk.version(), redo_version);
    assert_eq!(
        {
            let history = sdk.history_state();
            (history.undo_steps, history.redo_steps)
        },
        (0, 1)
    );
    sdk.redo(sdk.version()).unwrap();
    assert_eq!(sdk.snapshot().document().asset_order(), &[ASSET_ID]);
}

#[test]
fn stale_version_and_invalid_position_are_rejected_without_partial_write() {
    let path = fixture_png();
    let asset = crate::support::image_fixture(ASSET_ID, "fixture", &path).unwrap();
    let mesh = rectangle_mesh(
        MESH_ID,
        "face",
        ASSET_ID,
        Vec2::new(0.0, 0.0),
        Vec2::new(10.0, 10.0),
    )
    .unwrap();
    let mut sdk = session();
    let stale = sdk.begin_draft().prepare("stale").unwrap();
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                crate::support::insert_image(edit, asset)?;
                edit.create_mesh(mesh)
            })
            .and_then(|value| {
                let prepared = draft.prepare("create")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(sdk.publish(stale).unwrap_err().code, "STALE_SOURCE");
    let version = sdk.version();
    let original = sdk
        .snapshot()
        .document()
        .mesh(MESH_ID)
        .unwrap()
        .base_positions
        .clone();
    let mut draft = sdk.begin_draft();
    assert_eq!(
        draft
            .batch(draft.source(), |c| c
                .set_vertex_positions(MESH_ID, &[(0, Vec2::new(f32::NAN, 0.))]))
            .unwrap_err()
            .code,
        "INVALID_POSITION"
    );
    assert!(!draft.is_active());
    assert_eq!(sdk.version(), version);
    assert_eq!(
        sdk.snapshot()
            .document()
            .mesh(MESH_ID)
            .cloned()
            .unwrap()
            .base_positions,
        original
    );
}

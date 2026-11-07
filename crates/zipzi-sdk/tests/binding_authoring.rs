use zipzi_core::{
    Appearance, BindingAxis, Canvas, MeshBinding, MeshKeyform, Parameter, PreviewValues, Vec2,
};
use zipzi_sdk::{rectangle_mesh, AuthoringSession};

const DOCUMENT_ID: &str = "00000000-0000-4000-8000-000000000401";
const ASSET_ID: &str = "00000000-0000-4000-8000-000000000402";
const MESH_ID: &str = "00000000-0000-4000-8000-000000000403";
const PARAMETER_ID: &str = "00000000-0000-4000-8000-000000000404";
const BINDING_ID: &str = "00000000-0000-4000-8000-000000000405";

fn fixture() -> AuthoringSession {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/asymmetric-2x2.png");
    let asset = crate::support::image_fixture(ASSET_ID, "texture", &path).unwrap();
    let mesh = rectangle_mesh(
        MESH_ID,
        "eye",
        ASSET_ID,
        Vec2::new(40.0, 40.0),
        Vec2::new(60.0, 60.0),
    )
    .unwrap();
    let mut sdk = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                DOCUMENT_ID,
                (Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                crate::support::insert_image(edit, asset)?;
                edit.create_mesh(mesh)
            })
            .and_then(|value| {
                let prepared = draft.prepare("fixture")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    sdk.take_events();
    sdk
}

#[test]
fn metadata_publication_reuses_bindings_motions_and_identity_keys() {
    let mut sdk = fixture();
    let binding = binding(&sdk);
    let mut peer = sdk.snapshot().document().mesh(MESH_ID).cloned().unwrap();
    peer.id = "00000000-0000-4000-8000-000000000421".into();
    peer.runtime_id = "Peer".into();
    let mut peer_binding = binding.clone();
    peer_binding.id = "00000000-0000-4000-8000-000000000422".into();
    peer_binding.mesh_id = peer.id.clone();
    let clip: zipzi_core::document::MotionClip = serde_json::from_value(serde_json::json!({
        "id":"00000000-0000-4000-8000-000000000420", "name":"Motion", "duration":1., "fps":30.,
        "looping":false, "restricted_beziers":true, "fade_in":null, "fade_out":null,
        "track_order":[], "tracks":[], "events":[], "extensions":{}, "meta_extensions":{},
        "opaque_source_ids":null
    }))
    .unwrap();
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.create_mesh(peer)?;
                edit.create_parameter(parameter(false))?;
                edit.create_binding(binding)?;
                edit.create_binding(peer_binding)?;
                edit.create_motion(clip)
            })
            .and_then(|value| {
                let prepared = draft.prepare("dependencies")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let snapshot = sdk.read_snapshot();
    let handle = {
        let kind = zipzi_contract::authoring::ObjectKind::Mesh;
        let object_id = (MESH_ID).to_owned();
        sdk.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, sdk.read_snapshot().source))
    }
    .unwrap();
    for opacity in [0.3, 0.6, 0.9] {
        let previous = sdk
            .snapshot()
            .document()
            .mesh(MESH_ID)
            .cloned()
            .unwrap()
            .appearance
            .opacity;
        #[cfg(feature = "bench-instrumentation")]
        let capture = zipzi_core::benchmark::Capture::start();
        let mut mesh = sdk.snapshot().document().mesh(MESH_ID).cloned().unwrap();
        mesh.name = format!("Mesh {opacity}");
        mesh.appearance.opacity = opacity;
        let (_, receipt) = {
            let mut draft = sdk.begin_draft();
            draft
                .batch(draft.source(), |edit| edit.replace_mesh(mesh))
                .and_then(|value| {
                    let prepared = draft.prepare("presentation")?;
                    sdk.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        #[cfg(feature = "bench-instrumentation")]
        {
            let stages = capture.finish();
            for stage in [
                "sdk.keys_in",
                "core.validate_motion",
                "core.canonicalize_binding",
                "core.canonicalize_scene_binding",
            ] {
                assert!(!stages.contains_key(stage), "repeated {stage}: {stages:?}");
            }
        }
        assert!(!receipt.writes.is_empty());
        assert!(receipt
            .writes
            .records()
            .iter()
            .all(|w| w.change != zipzi_core::state::RecordChange::Created));
        assert!(receipt
            .writes
            .records()
            .iter()
            .all(|w| w.change != zipzi_core::state::RecordChange::Deleted));
        assert_eq!(
            sdk.snapshot()
                .document()
                .mesh(MESH_ID)
                .cloned()
                .unwrap()
                .appearance
                .opacity,
            opacity
        );
        sdk.snapshot()
            .document()
            .require_object(handle.0, &handle.1)
            .unwrap();
        assert_eq!(
            snapshot
                .snapshot
                .document()
                .mesh(MESH_ID)
                .unwrap()
                .appearance
                .opacity,
            1.
        );
        assert!(sdk.snapshot().document().builder().freeze().is_ok());
        sdk.undo(sdk.version()).unwrap();
        assert_eq!(
            sdk.snapshot()
                .document()
                .mesh(MESH_ID)
                .cloned()
                .unwrap()
                .appearance
                .opacity,
            previous
        );
        sdk.redo(sdk.version()).unwrap();
        assert_eq!(
            sdk.snapshot()
                .document()
                .mesh(MESH_ID)
                .cloned()
                .unwrap()
                .appearance
                .opacity,
            opacity
        );
        sdk.snapshot()
            .document()
            .require_object(handle.0, &handle.1)
            .unwrap();
    }
    // Keyform payload changes preserve owned axis/sample IDs too. Validate the
    // changed binding during replacement/publication, without revisiting peers.
    let mut binding = sdk
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .unwrap()
        .clone();
    binding.keyforms[0].appearance.opacity = 0.25;
    #[cfg(feature = "bench-instrumentation")]
    let capture = zipzi_core::benchmark::Capture::start();
    let (_, receipt) = {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_binding(binding))
            .and_then(|value| {
                let prepared = draft.prepare("keyform")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    #[cfg(feature = "bench-instrumentation")]
    {
        let stages = capture.finish();
        for stage in [
            "sdk.keys_in",
            "core.validate_motion",
            "core.canonicalize_scene_binding",
        ] {
            assert!(!stages.contains_key(stage), "repeated {stage}: {stages:?}");
        }
        assert_eq!(stages["core.canonicalize_binding"].calls, 2);
    }
    assert!(!receipt.writes.is_empty());
    assert!(receipt
        .writes
        .records()
        .iter()
        .all(|w| w.change != zipzi_core::state::RecordChange::Created));
    assert!(receipt
        .writes
        .records()
        .iter()
        .all(|w| w.change != zipzi_core::state::RecordChange::Deleted));
    assert!(!receipt.writes.records().is_empty());
    assert!(sdk.snapshot().document().builder().freeze().is_ok());
    sdk.undo(sdk.version()).unwrap();
    sdk.redo(sdk.version()).unwrap();
    assert_eq!(
        sdk.snapshot()
            .document()
            .binding(BINDING_ID)
            .unwrap()
            .keyforms[0]
            .appearance
            .opacity,
        0.25
    );
    sdk.snapshot()
        .document()
        .require_object(handle.0, &handle.1)
        .unwrap();
}

fn parameter(repeat: bool) -> Parameter {
    Parameter {
        id: PARAMETER_ID.into(),
        runtime_id: PARAMETER_ID.into(),
        name: "open".into(),
        minimum: 0.0,
        maximum: 1.0,
        default_value: 0.0,
        repeat,
        ..Parameter::default()
    }
}

fn binding(sdk: &AuthoringSession) -> MeshBinding {
    let base = sdk
        .snapshot()
        .document()
        .mesh(MESH_ID)
        .unwrap()
        .base_positions
        .clone();
    let shifted = base.iter().map(|p| Vec2::new(p.x + 10.0, p.y)).collect();
    MeshBinding {
        id: BINDING_ID.into(),
        mesh_id: MESH_ID.into(),
        axes: vec![BindingAxis {
            id: Default::default(),
            samples: Default::default(),
            parameter_id: PARAMETER_ID.into(),
            keys: vec![0.0, 1.0],
        }],
        keyforms: vec![
            MeshKeyform {
                samples: Default::default(),
                keys: vec![0.0],
                positions: base,
                appearance: Appearance::default(),
                draw_order: None,
            },
            MeshKeyform {
                samples: Default::default(),
                keys: vec![1.0],
                positions: shifted,
                appearance: Appearance::default(),
                draw_order: None,
            },
        ],
    }
}

fn sample(sdk: &AuthoringSession, requested: f32) -> (f32, f32, bool) {
    let values = PreviewValues::from([(PARAMETER_ID.into(), requested)]);
    let frame = crate::support::frame(sdk.snapshot(), &values).unwrap();
    (
        frame.parameters[0].value,
        frame.drawables[0].positions[0].x,
        frame.parameters[0].clamped,
    )
}

#[test]
fn complete_binding_interpolates_and_undoes_with_parameter() {
    let mut sdk = fixture();
    let b = binding(&sdk);
    let before = sdk.version();
    let (_, p) = crate::support::prepared(&sdk, |c| {
        c.create_parameter(parameter(false))?;
        c.create_binding(b)
    })
    .unwrap();
    let receipt = sdk.publish(p).unwrap();
    assert_eq!(receipt.version.revision, before.revision + 1);
    assert_eq!(sdk.snapshot().document().parameter_order(), &[PARAMETER_ID]);
    assert_eq!(sdk.snapshot().document().binding_order(), &[BINDING_ID]);
    assert_eq!(
        sdk.snapshot()
            .document()
            .binding_for_mesh(MESH_ID)
            .unwrap()
            .id,
        BINDING_ID
    );
    assert_eq!(sample(&sdk, 0.0), (0.0, 40.0, false));
    assert_eq!(sample(&sdk, 0.5), (0.5, 45.0, false));
    assert_eq!(sample(&sdk, 1.0), (1.0, 50.0, false));
    assert_eq!(sample(&sdk, 2.0), (1.0, 50.0, true));
    sdk.undo(sdk.version()).unwrap();
    assert!(sdk.snapshot().document().parameter_order().is_empty());
    assert!(sdk.snapshot().document().binding_order().is_empty());
    sdk.redo(sdk.version()).unwrap();
    assert_eq!(sample(&sdk, 0.5), (0.5, 45.0, false));
}

#[test]
fn incomplete_binding_aborts_parameter_creation_and_failed_range_replacement() {
    let mut sdk = fixture();
    let mut incomplete = binding(&sdk);
    incomplete.keyforms.pop();
    let version = sdk.version();
    let mut edit = sdk.begin_draft();
    edit.batch(edit.source(), |c| c.create_parameter(parameter(false)))
        .unwrap();
    assert_eq!(
        edit.batch(edit.source(), |c| c.create_binding(incomplete))
            .unwrap_err()
            .code
            .as_str(),
        "INCOMPLETE_KEYFORMS"
    );
    assert!(edit.prepare("invalid").is_err());
    assert_eq!(sdk.version(), version);
    assert!(sdk.snapshot().document().parameter_order().is_empty());

    let complete = binding(&sdk);
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.create_parameter(parameter(false))?;
                edit.create_binding(complete)
            })
            .and_then(|value| {
                let prepared = draft.prepare("bind")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let version = sdk.version();
    let mut narrowed = sdk
        .snapshot()
        .document()
        .parameter(PARAMETER_ID)
        .cloned()
        .unwrap();
    narrowed.maximum = 0.5;
    let mut edit = sdk.begin_draft();
    assert_eq!(
        edit.batch(edit.source(), |c| c.replace_parameter(narrowed))
            .unwrap_err()
            .code
            .as_str(),
        "INVALID_KEYS"
    );
    assert!(edit.prepare("invalid").is_err());
    assert_eq!(sdk.version(), version);
    assert_eq!(
        sdk.snapshot()
            .document()
            .parameter(PARAMETER_ID)
            .cloned()
            .unwrap()
            .maximum,
        1.0
    );
}

#[test]
fn keyform_replacement_and_repeat_mapping_use_core_rules() {
    let mut sdk = fixture();
    let b = binding(&sdk);
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.create_parameter(parameter(true))?;
                edit.create_binding(b)
            })
            .and_then(|value| {
                let prepared = draft.prepare("bind")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(sample(&sdk, 1.25), (0.25, 42.5, false));
    let mut form = sdk
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap()
        .keyforms[1]
        .clone();
    form.positions.iter_mut().for_each(|p| p.x += 10.0);
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.set_mesh_keyform(BINDING_ID, form)
            })
            .and_then(|value| {
                let prepared = draft.prepare("change form")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(sample(&sdk, 1.25), (0.25, 45.0, false));
    let mut binding = sdk
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap();
    binding.keyforms[1]
        .positions
        .iter_mut()
        .for_each(|p| p.x += 10.0);
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |edit| edit.replace_binding(binding))
            .and_then(|value| {
                let prepared = draft.prepare("replace table")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(sample(&sdk, 1.25), (0.25, 47.5, false));
}

#[path = "binding_authoring/conversions/mod.rs"]
mod conversions;

#[path = "binding_authoring/stable_domains/mod.rs"]
mod stable_domains;

#[test]
fn ordinary_binding_snapshot_delete_is_atomic_cancellable_and_preserves_base() {
    use zipzi_contract::authoring::BindingAuthoringEdit as E;
    use zipzi_sdk::{BindingGridEdit as G, ConversionControl};
    let mut sdk = fixture();
    let binding = binding(&sdk);
    {
        let mut draft = sdk.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter(parameter(false))?;
                e.create_binding(binding)
            })
            .and_then(|value| {
                let prepared = draft.prepare("driven")?;
                sdk.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let axis = sdk
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap()
        .axes[0]
        .id
        .clone();
    let mesh = sdk.snapshot().document().mesh(MESH_ID).cloned().unwrap();
    let snapshot = sdk.read_snapshot();
    let control = ConversionControl::default();
    let plan = crate::support::binding(
        &sdk,
        E::Grid(G::Delete {
            owner: BINDING_ID.into(),
        }),
    )
    .unwrap();
    assert!(plan.0.report.deleted.iter().any(
        |a| matches!(&a.element,zipzi_core::document::ConversionElement::Axis(id) if id==&axis)
    ));
    control.cancel();
    assert!(sdk
        .publish_with_control(plan.1, || control.check().is_ok())
        .is_err());
    assert!(sdk.snapshot().document().binding(BINDING_ID).is_some());
    let stale = sdk.begin_draft();
    let plan = crate::support::binding(
        &sdk,
        E::Grid(G::Delete {
            owner: BINDING_ID.into(),
        }),
    )
    .unwrap();
    let receipt = sdk.publish(plan.1).unwrap();
    assert!(receipt
        .writes
        .records()
        .iter()
        .any(|w| w.id == BINDING_ID && w.change == zipzi_core::state::RecordChange::Deleted));
    assert!(sdk
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .is_none());
    assert_eq!(
        sdk.snapshot().document().mesh(MESH_ID).cloned().unwrap(),
        mesh
    );
    assert!(sdk.snapshot().document().parameter(PARAMETER_ID).is_some());
    assert!(snapshot.snapshot.document().binding(BINDING_ID).is_some());
    sdk.undo(sdk.version()).unwrap();
    assert!(sdk
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .is_some());
    assert_eq!(
        sdk.publish(stale.prepare("old grid").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    sdk.redo(sdk.version()).unwrap();
    assert!(sdk
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .is_none());
}

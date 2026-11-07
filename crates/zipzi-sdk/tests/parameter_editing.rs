use zipzi_core::{state::DocumentState, ModelSetup, ParameterKind};
use zipzi_sdk::{AuthoringSession, ParameterCreateSpec, ParameterPatch, ResourceView, Snapshot};

fn sdk() -> AuthoringSession {
    AuthoringSession::new(
        Snapshot::new(
            DocumentState::new(
                "79600000-0000-4000-8000-000000000001",
                ModelSetup::default(),
            )
            .unwrap(),
            ResourceView::default(),
        )
        .unwrap(),
    )
}
fn spec() -> ParameterCreateSpec {
    ParameterCreateSpec {
        name: "Angle".into(),
        minimum: -30.,
        maximum: 30.,
        default_value: 0.,
        id: None,
        runtime_id: None,
        kind: None,
        decimal_places: None,
        repeat: None,
    }
}

#[test]
fn selected_parameter_specs_allocate_identity_and_keep_defaults_in_sdk() {
    let mut sdk = sdk();
    let mut draft = sdk.begin_draft();
    let id = draft
        .batch(draft.source(), |c| c.create_parameter_spec(spec()))
        .unwrap();
    assert_eq!(uuid::Uuid::parse_str(&id).unwrap().get_version_num(), 4);
    let value = draft.document().unwrap().parameter(&id).unwrap();
    assert_eq!(value.runtime_id, id);
    assert_eq!(value.kind, ParameterKind::Normal);
    assert_eq!(value.decimal_places, 6);
    assert!(!value.repeat);
    draft
        .batch(draft.source(), |c| {
            c.patch_parameter(
                &id,
                ParameterPatch {
                    name: Some("Final".into()),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    sdk.publish(draft.prepare("defaults").unwrap()).unwrap();
    assert_eq!(
        sdk.snapshot().document().parameter(&id).unwrap().name,
        "Final"
    );
    sdk.undo(sdk.version()).unwrap();
    assert!(sdk.snapshot().document().parameter(&id).is_none());
    sdk.redo(sdk.version()).unwrap();
    assert_eq!(
        sdk.snapshot().document().parameter(&id).unwrap().name,
        "Final"
    );
}

#[test]
fn partial_parameter_patch_keeps_unspecified_fields_and_caught_failure_aborts() {
    let mut sdk = sdk();
    let mut draft = sdk.begin_draft();
    let id = draft
        .batch(draft.source(), |c| {
            let id = c.create_parameter_spec(ParameterCreateSpec {
                runtime_id: Some("ParamBlend".into()),
                kind: Some(ParameterKind::BlendShape),
                decimal_places: Some(2),
                repeat: Some(true),
                ..spec()
            })?;
            c.patch_parameter(
                &id,
                ParameterPatch {
                    name: Some("Preserved".into()),
                    ..Default::default()
                },
            )?;
            Ok(id)
        })
        .unwrap();
    let value = draft.document().unwrap().parameter(&id).unwrap();
    assert_eq!(value.runtime_id, "ParamBlend");
    assert_eq!(value.kind, ParameterKind::BlendShape);
    assert_eq!(value.decimal_places, 2);
    assert!(value.repeat);
    draft
        .batch(draft.source(), |c| {
            c.patch_parameter(
                &id,
                ParameterPatch {
                    repeat: Some(false),
                    decimal_places: Some(0),
                    ..Default::default()
                },
            )
        })
        .unwrap();
    sdk.publish(draft.prepare("explicit").unwrap()).unwrap();
    assert!(!sdk.snapshot().document().parameter(&id).unwrap().repeat);
    assert_eq!(
        sdk.snapshot()
            .document()
            .parameter(&id)
            .unwrap()
            .decimal_places,
        0
    );
    let before = sdk.version();
    let history = sdk.history_state();
    let mut draft = sdk.begin_draft();
    let failure = draft
        .batch(draft.source(), |c| {
            c.patch_parameter(
                &id,
                ParameterPatch {
                    name: Some("Unpublished".into()),
                    ..Default::default()
                },
            )?;
            assert_eq!(
                c.patch_parameter(
                    &id,
                    ParameterPatch {
                        minimum: Some(50.),
                        ..Default::default()
                    }
                )
                .unwrap_err()
                .code,
                "INVALID_PARAMETER"
            );
            Ok(()) // Ignoring the failure must not permit publication.
        })
        .unwrap_err();
    assert_eq!(failure.code, "INVALID_PARAMETER");
    assert!(draft.prepare("caught failure").is_err());
    assert_eq!(sdk.version(), before);
    assert_eq!(
        sdk.snapshot().document().parameter(&id).unwrap().name,
        "Preserved"
    );
    assert_eq!(sdk.history_state(), history);
    let mut draft = sdk.begin_draft();
    assert_eq!(
        draft
            .batch(draft.source(), |c| c
                .patch_parameter("missing", Default::default()))
            .unwrap_err()
            .code,
        "MISSING_PARAMETER"
    );
    assert!(draft.prepare("missing").is_err());
}

#[test]
fn duplicate_runtime_id_rejection_does_not_publish_and_replacement_updates_index() {
    let mut sdk = sdk();
    let mut draft = sdk.begin_draft();
    let id = draft
        .batch(draft.source(), |c| {
            c.create_parameter_spec(ParameterCreateSpec {
                runtime_id: Some("ParamAngle".into()),
                ..spec()
            })
        })
        .unwrap();
    sdk.publish(draft.prepare("create").unwrap()).unwrap();
    let before = sdk.version();
    let mut failed = sdk.begin_draft();
    assert_eq!(
        failed
            .batch(failed.source(), |c| c.create_parameter_spec(
                ParameterCreateSpec {
                    runtime_id: Some("ParamAngle".into()),
                    ..spec()
                }
            ))
            .unwrap_err()
            .code,
        "DUPLICATE_RUNTIME_ID"
    );
    assert_eq!(sdk.version(), before);
    let mut draft = sdk.begin_draft();
    let second = draft
        .batch(draft.source(), |c| {
            c.patch_parameter(
                &id,
                ParameterPatch {
                    runtime_id: Some("Renamed".into()),
                    ..Default::default()
                },
            )?;
            c.create_parameter_spec(ParameterCreateSpec {
                runtime_id: Some("ParamAngle".into()),
                ..spec()
            })
        })
        .unwrap();
    sdk.publish(draft.prepare("reuse runtime ID").unwrap())
        .unwrap();
    assert_eq!(
        sdk.snapshot().document().parameter(&id).unwrap().runtime_id,
        "Renamed"
    );
    assert_eq!(
        sdk.snapshot()
            .document()
            .parameter(&second)
            .unwrap()
            .runtime_id,
        "ParamAngle"
    );
    sdk.undo(sdk.version()).unwrap();
    assert_eq!(
        sdk.snapshot().document().parameter(&id).unwrap().runtime_id,
        "ParamAngle"
    );
    assert!(sdk.snapshot().document().parameter(&second).is_none());
}

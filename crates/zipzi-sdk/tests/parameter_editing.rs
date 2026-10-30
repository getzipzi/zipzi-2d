use zipzi_core::{ModelSetup, ParameterKind};
use zipzi_sdk::{
    AuthoringSession, ParameterCreateSpec, ParameterOperation, ParameterOperationApply,
    ParameterPatch,
};

fn sdk() -> AuthoringSession {
    AuthoringSession::new(
        "79600000-0000-4000-8000-000000000001",
        ModelSetup::default(),
    )
    .unwrap()
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
    let mut edit = sdk.begin_owned_edit("defaults", None).unwrap();
    let id = ParameterOperation::Create { spec: spec() }
        .apply(&mut edit)
        .unwrap();
    assert_eq!(uuid::Uuid::parse_str(&id).unwrap().get_version_num(), 4);
    let value = edit.candidate_document().get_parameter(&id).unwrap();
    assert_eq!(value.runtime_id, id);
    assert_eq!(value.kind, ParameterKind::Normal);
    assert_eq!(value.decimal_places, 6);
    assert!(!value.repeat);
    ParameterOperation::Patch {
        id: id.clone(),
        patch: ParameterPatch {
            name: Some("Final".into()),
            ..Default::default()
        },
    }
    .apply(&mut edit)
    .unwrap();
    edit.commit_to(&mut sdk).unwrap();
    assert_eq!(sdk.parameter(&id).unwrap().name, "Final");
    assert_eq!(sdk.history_lengths(), (1, 0));
}

#[test]
fn partial_parameter_patch_keeps_unspecified_fields_and_caught_failure_aborts() {
    let mut sdk = sdk();
    let mut edit = sdk.begin_owned_edit("explicit", None).unwrap();
    let id = edit
        .create_parameter_spec(ParameterCreateSpec {
            runtime_id: Some("ParamBlend".into()),
            kind: Some(ParameterKind::BlendShape),
            decimal_places: Some(2),
            repeat: Some(true),
            ..spec()
        })
        .unwrap();
    edit.patch_parameter(
        &id,
        ParameterPatch {
            name: Some("Preserved".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let value = edit.candidate_document().get_parameter(&id).unwrap();
    assert_eq!(value.runtime_id, "ParamBlend");
    assert_eq!(value.kind, ParameterKind::BlendShape);
    assert_eq!(value.decimal_places, 2);
    assert!(value.repeat);
    edit.patch_parameter(
        &id,
        ParameterPatch {
            repeat: Some(false),
            decimal_places: Some(0),
            ..Default::default()
        },
    )
    .unwrap();
    edit.commit_to(&mut sdk).unwrap();
    assert!(!sdk.parameter(&id).unwrap().repeat);
    assert_eq!(sdk.parameter(&id).unwrap().decimal_places, 0);
    let before = sdk.version();
    let mut edit = sdk.begin_owned_edit("caught failure", None).unwrap();
    edit.patch_parameter(
        &id,
        ParameterPatch {
            name: Some("Unpublished".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let error = edit
        .patch_parameter(
            &id,
            ParameterPatch {
                minimum: Some(50.),
                ..Default::default()
            },
        )
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "INVALID_PARAMETER");
    assert!(edit.commit_to(&mut sdk).is_err());
    assert_eq!(sdk.version(), before);
    assert_eq!(sdk.parameter(&id).unwrap().name, "Preserved");
    assert_eq!(sdk.history_lengths(), (1, 0));
    let mut edit = sdk.begin_owned_edit("missing", None).unwrap();
    let error = edit
        .patch_parameter("missing", Default::default())
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "MISSING_PARAMETER");
    assert_eq!(error.object_ids, ["missing"]);
    assert!(edit.commit_to(&mut sdk).is_err());
}

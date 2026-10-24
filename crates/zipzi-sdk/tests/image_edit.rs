use zipzi_core::ModelSetup;
use zipzi_sdk::{AuthoringSession, ImageCreateSpec};

#[test]
fn selected_image_spec_allocates_sdk_identity_and_reads_only_current_candidate_content() {
    let mut sdk = AuthoringSession::with_history_limits(
        "79500000-0000-4000-8000-000000000001",
        ModelSetup::default(),
        Default::default(),
    )
    .unwrap();
    let a = sdk
        .prepare_rgba_image(1, 1, std::sync::Arc::from([12, 34, 56, 255]))
        .unwrap();
    let b = sdk
        .prepare_rgba_image(1, 1, std::sync::Arc::from([11, 22, 33, 255]))
        .unwrap();
    let mut edit = sdk.begin_edit("image spec", None).unwrap();
    let id = edit
        .create_image_spec(
            ImageCreateSpec {
                id: None,
                name: "Texture".into(),
            },
            &a,
        )
        .unwrap();
    assert_eq!(uuid::Uuid::parse_str(&id).unwrap().get_version_num(), 4);
    let old = edit.memory_image(&id).unwrap();
    edit.replace_prepared_image(&id, &b).unwrap();
    assert_eq!(
        edit.memory_image(&id).unwrap().texture().rgba.as_ref(),
        &[11, 22, 33, 255]
    );
    assert_eq!(old.texture().rgba.as_ref(), &[12, 34, 56, 255]);
    edit.commit().unwrap();
    assert_eq!(sdk.document().get_asset(&id).unwrap().name, "Texture");
    assert_eq!(
        sdk.read_texture_asset(sdk.document().get_asset(&id).unwrap())
            .unwrap()
            .rgba
            .as_ref(),
        &[11, 22, 33, 255]
    );
    assert_eq!(sdk.history_lengths(), (1, 0));
}

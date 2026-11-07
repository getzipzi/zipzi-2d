use super::*;

const DOCUMENT: &str = "00000000-0000-0000-0000-000000000001";
const A: &str = "00000000-0000-0000-0000-000000000002";
const B: &str = "00000000-0000-0000-0000-000000000003";

fn empty() -> DocumentState {
    DocumentState::new(DOCUMENT, ModelSetup::default()).unwrap()
}
fn parameter(id: &str) -> Parameter {
    Parameter {
        id: id.into(),
        ..Default::default()
    }
}
fn populated() -> DocumentState {
    let mut builder = empty().builder();
    builder.create_parameter(parameter(A)).unwrap();
    builder.create_parameter(parameter(B)).unwrap();
    builder.freeze().unwrap().state
}

#[test]
fn snapshots_and_parallel_builders_are_isolated() {
    let base = populated();
    let mut first = base.builder();
    let mut second = base.builder();
    let mut changed = first.parameter(A).unwrap().clone();
    changed.name = "first".into();
    first.replace_parameter(changed).unwrap();
    second.remove_parameter(B).unwrap();
    let first = first.freeze().unwrap();
    let second = second.freeze().unwrap();
    assert_eq!(base.parameter(A).unwrap().name, "");
    assert_eq!(first.state.parameter(A).unwrap().name, "first");
    assert!(base.parameter(B).is_some());
    assert!(first.state.parameter(B).is_some());
    assert!(second.state.parameter(B).is_none());
    assert!(std::ptr::eq(
        base.parameter(B).unwrap(),
        first.state.parameter(B).unwrap()
    ));
    assert!(std::ptr::eq(
        base.parameter(A).unwrap(),
        second.state.parameter(A).unwrap()
    ));
    assert!(std::ptr::eq(
        base.parameter_order(),
        first.state.parameter_order()
    ));
    assert!(Arc::ptr_eq(
        &base.content.parameter_runtime_ids,
        &first.state.content.parameter_runtime_ids
    ));
    assert_ne!(first.state.content_id(), second.state.content_id());
    assert_eq!(first.writes.records()[0].change, RecordChange::Replaced);
    assert_eq!(second.writes.records()[0].change, RecordChange::Deleted);
}

#[test]
fn no_net_changes_reuse_content_identity_and_roots() {
    let base = populated();
    let mut builder = base.builder();
    let original = builder.parameter(A).unwrap().clone();
    let mut replacement = original.clone();
    replacement.name = "temporary".into();
    builder.replace_parameter(replacement).unwrap();
    builder.replace_parameter(original).unwrap();
    let mut setup = base.setup();
    setup.export.padding_pixels += 1.;
    builder.set_setup(setup).unwrap();
    builder.set_setup(base.setup()).unwrap();
    let frozen = builder.freeze().unwrap();
    assert!(frozen.writes.is_empty());
    assert_eq!(base.content_id(), frozen.state.content_id());
    assert!(Arc::ptr_eq(&base.content, &frozen.state.content));

    let mut builder = base.builder();
    let id = "00000000-0000-0000-0000-000000000004";
    builder.create_parameter(parameter(id)).unwrap();
    builder.remove_parameter(id).unwrap();
    let frozen = builder.freeze().unwrap();
    assert!(frozen.writes.is_empty());
    assert_eq!(base.content_id(), frozen.state.content_id());
}

#[test]
fn net_zero_record_recovers_sharing_in_a_changed_batch() {
    let base = populated();
    let mut builder = base.builder();
    let original = builder.parameter(A).unwrap().clone();
    let mut changed = original.clone();
    changed.name = "temporary".into();
    builder.replace_parameter(changed).unwrap();
    builder.replace_parameter(original).unwrap();
    builder.remove_parameter(B).unwrap();
    let frozen = builder.freeze().unwrap();
    assert!(std::ptr::eq(
        base.parameter(A).unwrap(),
        frozen.state.parameter(A).unwrap()
    ));
    assert_eq!(frozen.writes.records().len(), 1);
    assert_eq!(frozen.writes.records()[0].id, B);
}

#[test]
fn separate_freezes_do_not_reuse_old_content_identity() {
    let base = populated();
    let original = base.parameter(A).unwrap().clone();
    let mut edited = original.clone();
    edited.name = "temporary".into();
    let mut builder = base.builder();
    builder.replace_parameter(edited).unwrap();
    let changed = builder.freeze().unwrap().state;
    let mut builder = changed.builder();
    builder.replace_parameter(original).unwrap();
    let restored_values = builder.freeze().unwrap();
    assert_eq!(base.parameter(A), restored_values.state.parameter(A));
    assert_ne!(base.content_id(), restored_values.state.content_id());
    assert_ne!(changed.content_id(), restored_values.state.content_id());
    assert!(!restored_values.writes.is_empty());
}

#[test]
fn order_only_change_is_not_a_noop() {
    let base = populated();
    let mut builder = base.builder();
    let original = builder.parameter(A).unwrap().clone();
    builder.remove_parameter(A).unwrap();
    builder.create_parameter(original).unwrap();
    let frozen = builder.freeze().unwrap();
    assert!(frozen.writes.records().is_empty());
    assert!(frozen.writes.order_changed(RecordDomain::Parameter));
    assert_eq!(frozen.state.parameter_order(), &[B, A]);
    assert_eq!(base.parameter_order(), &[A, B]);
    assert_ne!(base.content_id(), frozen.state.content_id());
}

#[test]
fn failed_command_discards_the_entire_candidate() {
    let base = empty();
    let mut builder = base.builder();
    builder.create_parameter(parameter(A)).unwrap();
    let error = builder.create_parameter(parameter(A)).unwrap_err();
    assert_eq!(error.code, "DUPLICATE_ID");
    assert_eq!(
        builder.create_parameter(parameter(B)).unwrap_err().code,
        error.code
    );
    assert_eq!(builder.freeze().unwrap_err().code, error.code);
    assert!(base.parameter_order().is_empty());
}

#[test]
fn runtime_index_is_frozen_with_its_content() {
    let base = populated();
    let mut builder = base.builder();
    let mut changed = builder.parameter(A).unwrap().clone();
    changed.runtime_id = "renamed".into();
    builder.replace_parameter(changed).unwrap();
    let changed = builder.freeze().unwrap().state;
    assert!(changed.parameter_by_runtime_id(A).is_none());
    assert_eq!(changed.parameter_by_runtime_id("renamed").unwrap().id, A);
    assert_eq!(base.parameter_by_runtime_id(A).unwrap().id, A);
    assert!(base.parameter_by_runtime_id("renamed").is_none());
    let mut builder = changed.builder();
    let mut collision = builder.parameter(B).unwrap().clone();
    collision.runtime_id = "renamed".into();
    assert_eq!(
        builder.replace_parameter(collision).unwrap_err().code,
        "DUPLICATE_RUNTIME_ID"
    );
    assert!(builder.freeze().is_err());
}

#[test]
fn parameter_rules_reject_nonfinite_ranges_and_invalid_precision() {
    for (min, max, default, precision) in [
        (f32::NAN, 1., 0., 6),
        (-1., f32::INFINITY, 0., 6),
        (1., 1., 1., 6),
        (-f32::MAX, f32::MAX, 0., 6),
        (-1., 1., 2., 6),
        (-1., 1., 0., 10),
    ] {
        let mut builder = empty().builder();
        let value = Parameter {
            minimum: min,
            maximum: max,
            default_value: default,
            decimal_places: precision,
            ..parameter(A)
        };
        assert_eq!(
            builder.create_parameter(value).unwrap_err().code,
            "INVALID_PARAMETER"
        );
        assert!(builder.freeze().is_err());
    }
}

#[test]
fn assets_track_versions_and_preserve_unresolved_descriptions() {
    let base = empty();
    let mut builder = base.builder();
    let key = ResourceKey::new();
    let image = AssetRecord {
        id: A.into(),
        name: "image".into(),
        content: AssetContent::Image {
            resource: key,
            width: 2,
            height: 3,
        },
    };
    builder.create_asset(image.clone()).unwrap();
    builder
        .create_asset(AssetRecord {
            id: B.into(),
            name: "missing".into(),
            content: AssetContent::Unresolved {
                source: "missing.png".into(),
            },
        })
        .unwrap();
    let first = builder.freeze().unwrap();
    assert_eq!(first.writes.records().len(), 2);
    assert!(first
        .writes
        .records()
        .iter()
        .all(|write| write.change == RecordChange::Created));
    assert_eq!(
        first.state.image_resources().collect::<Vec<_>>(),
        vec![(key, 2, 3)]
    );
    let mut builder = first.state.builder();
    builder
        .replace_asset(AssetRecord {
            content: AssetContent::Empty,
            ..image
        })
        .unwrap();
    builder.remove_asset(B).unwrap();
    let second = builder.freeze().unwrap();
    assert_eq!(second.state.image_resources().count(), 0);
    assert_eq!(first.state.image_resources().count(), 1);
    assert_eq!(second.state.asset_order(), &[A]);
    assert_eq!(second.writes.records()[0].change, RecordChange::Replaced);
    assert_eq!(second.writes.records()[1].change, RecordChange::Deleted);
}

#[test]
fn invalid_assets_and_cross_domain_duplicate_ids_cannot_freeze() {
    let base = populated();
    let mut builder = base.builder();
    assert_eq!(
        builder
            .create_asset(AssetRecord {
                id: A.into(),
                name: String::new(),
                content: AssetContent::Empty
            })
            .unwrap_err()
            .code,
        "DUPLICATE_ID"
    );
    assert!(builder.freeze().is_err());
    let mut builder = empty().builder();
    assert_eq!(
        builder
            .create_asset(AssetRecord {
                id: A.into(),
                name: String::new(),
                content: AssetContent::Image {
                    resource: ResourceKey::new(),
                    width: 0,
                    height: 1
                }
            })
            .unwrap_err()
            .code,
        "INVALID_ASSET"
    );
    assert!(builder.freeze().is_err());
}

#[test]
fn frozen_state_is_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<DocumentState>();
    check::<FrozenDocument>();
}

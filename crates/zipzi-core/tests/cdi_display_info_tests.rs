mod support;

use std::{collections::BTreeMap, sync::Arc};
use support::prepare;
use zipzi_core::state::{DocumentState, RootDomain};

use serde_json::json;
use zipzi_core::document::{
    CdiCombinedSet, CdiParameterEntry, CdiParameterGroup, CdiParameterRef, DisplayInfo,
    ReferenceEntry, ReferenceEntryAllocation,
};
use zipzi_core::preview::PreviewState;
use zipzi_core::{Canvas, Parameter, Part, Vec2};

const DOC: &str = "00000000-0000-4000-8000-000000000801";
const PARAM: &str = "00000000-0000-4000-8000-000000000802";
const PART: &str = "00000000-0000-4000-8000-000000000803";
const GROUP: &str = "00000000-0000-4000-8000-000000000804";
const CHILD: &str = "00000000-0000-4000-8000-000000000805";
const SET: &str = "00000000-0000-4000-8000-000000000806";

fn fixture_document() -> DocumentState {
    let base =
        DocumentState::new(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0).into()).unwrap();
    let mut builder = base.builder();
    builder
        .create_parameter(Parameter {
            id: PARAM.into(),
            runtime_id: "ParamA".into(),
            name: "old".into(),
            ..Default::default()
        })
        .unwrap();
    builder
        .create_part(Part {
            id: PART.into(),
            runtime_id: "PartA".into(),
            name: "old part".into(),
            ..Default::default()
        })
        .unwrap();
    builder.freeze().unwrap().state
}

fn info() -> DisplayInfo {
    DisplayInfo {
        parameters: Some(vec![CdiParameterEntry::Resolved {
            parameter_id: PARAM.into(),
            group_id: Some(GROUP.into()),
            extensions: BTreeMap::new(),
        }]),
        parameter_groups: Some(vec![
            CdiParameterGroup {
                id: GROUP.into(),
                runtime_id: "Face".into(),
                name: "顔".into(),
                parent_id: None,
                extensions: BTreeMap::new(),
            },
            CdiParameterGroup {
                id: CHILD.into(),
                runtime_id: "Child".into(),
                name: "子".into(),
                parent_id: Some(GROUP.into()),
                extensions: BTreeMap::new(),
            },
        ]),
        combined_parameters: Some(vec![CdiCombinedSet {
            id: SET.into(),
            members: vec![ReferenceEntry::new(CdiParameterRef::Resolved {
                parameter_id: PARAM.into(),
            })],
        }]),
        ..Default::default()
    }
}

#[test]
fn cdi_identity_references_and_recovery_are_checked() {
    let base = fixture_document();
    let mut invalid = info();
    invalid.parameter_groups.as_mut().unwrap()[0].id = PARAM.into();
    assert_eq!(
        prepare(&base, |b| b.replace_display_info(invalid))
            .unwrap_err()
            .code,
        "DUPLICATE_ID"
    );
    let mut invalid = info();
    invalid.parameter_groups.as_mut().unwrap()[0].parent_id = Some(CHILD.into());
    assert_eq!(
        prepare(&base, |b| b.replace_display_info(invalid))
            .unwrap_err()
            .code,
        "CDI_GROUP_CYCLE"
    );

    let document = prepare(&base, |b| b.replace_display_info(info()))
        .unwrap()
        .state;
    assert!(document.contains_id(GROUP));
    assert!(document.contains_id(SET));
    let mut without_group = document.display_info().clone();
    without_group.parameter_groups.as_mut().unwrap().remove(0);
    assert!(prepare(&document, |b| b.replace_display_info(without_group)).is_err());
    assert!(prepare(&document, |b| b.remove_parameter(PARAM)).is_err());
    let mut without_combined = document.display_info().clone();
    without_combined
        .combined_parameters
        .as_mut()
        .unwrap()
        .clear();
    let removed = prepare(&document, |b| b.replace_display_info(without_combined)).unwrap();
    assert!(removed.writes.root_changed(RootDomain::DisplayInfo));
    assert!(!removed.state.contains_id(SET));
    let removed = prepare(&removed.state, |b| {
        b.replace_display_info(DisplayInfo::default())?;
        b.remove_parameter(PARAM)
    })
    .unwrap();
    assert!(removed.state.parameter(PARAM).is_none());

    let mut unresolved = DisplayInfo {
        parameters: Some(vec![CdiParameterEntry::Unresolved {
            runtime_id: "NewParam".into(),
            name: "unknown".into(),
            group_id: None,
            extensions: BTreeMap::new(),
        }]),
        ..DisplayInfo::default()
    };
    let document = prepare(&base, |b| b.replace_display_info(unresolved.clone()))
        .unwrap()
        .state;
    let new_id = "00000000-0000-4000-8000-000000000807";
    let parameter = Parameter {
        id: new_id.into(),
        runtime_id: "NewParam".into(),
        ..Default::default()
    };
    assert_eq!(
        prepare(&document, |b| b.create_parameter(parameter.clone()))
            .unwrap_err()
            .code,
        "DUPLICATE_RUNTIME_ID"
    );
    unresolved.parameters = Some(vec![CdiParameterEntry::Resolved {
        parameter_id: new_id.into(),
        group_id: None,
        extensions: BTreeMap::new(),
    }]);
    let repaired = prepare(&document, |b| {
        b.replace_display_info(DisplayInfo::default())?;
        b.create_parameter(parameter)?;
        b.replace_display_info(unresolved)
    })
    .unwrap();
    assert!(repaired.state.contains_id(new_id));
    assert!(!document.contains_id(new_id));
    assert!(matches!(
        document.display_info().parameters.as_deref(),
        Some([CdiParameterEntry::Unresolved { .. }])
    ));
}

#[test]
fn display_metadata_keeps_geometry_results_equal_across_frozen_snapshots() {
    let old = fixture_document();
    let mut preview = PreviewState::default();
    let first = preview.frame(&old, 1).unwrap();
    let count = preview.evaluation_count();
    let mut parameter = old.parameter(PARAM).unwrap().clone();
    parameter.name = "新名称".into();
    let mut part = old.part(PART).unwrap().clone();
    part.name = "部件".into();
    let edited = prepare(&old, |b| {
        b.replace_parameter(parameter)?;
        b.replace_part(part)?;
        b.replace_display_info(info())
    })
    .unwrap();
    assert!(edited.writes.root_changed(RootDomain::DisplayInfo));
    assert_ne!(edited.state.content_id(), old.content_id());
    assert_eq!(edited.state.parameter(PARAM).unwrap().name, "新名称");
    assert_eq!(edited.state.part(PART).unwrap().name, "部件");
    let next = preview.frame(&edited.state, 1).unwrap();
    assert_eq!(preview.evaluation_count(), count + 1);
    assert_eq!(first, next);
    assert!(Arc::ptr_eq(
        &next,
        &preview.frame(&edited.state, 1).unwrap()
    ));
    assert_eq!(first, preview.frame(&old, 1).unwrap());
    assert_eq!(old.parameter(PARAM).unwrap().name, "old");
}

#[test]
fn nested_extension_maps_increase_the_write_set_estimate() {
    let base = fixture_document();
    let mut info = DisplayInfo::default();
    info.extensions
        .insert("one".into(), json!({"nested":{"tiny":true}}));
    let one = prepare(&base, |b| b.replace_display_info(info.clone())).unwrap();
    assert!(one.writes.root_changed(RootDomain::DisplayInfo));
    assert!(one.writes.estimated_bytes() > 0);

    info.extensions
        .insert("many".into(), json!({"a":1,"b":2,"c":3,"d":4}));
    let many = prepare(&base, |b| b.replace_display_info(info)).unwrap();
    assert!(many.writes.estimated_bytes() > one.writes.estimated_bytes());
}

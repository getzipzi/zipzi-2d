use std::collections::HashMap;
use zipzi_runtime::{authoring::EditOperation, modeling::ModelingCommand};
use zipzi_sdk::{GeometrySnapshot, GeometrySpace, TopologyReplacement, Version};
#[test]
fn integer_vertex_maps_roundtrip_inside_buffered_tagged_authoring_commands() {
    let mesh = zipzi_core::Mesh {
        id: uuid::Uuid::new_v4().to_string(),
        ..Default::default()
    };
    let operation = EditOperation::Modeling {
        operation: Box::new(ModelingCommand::ReplaceTopology {
            source: GeometrySnapshot {
                version: Version {
                    session_id: 1,
                    generation: 1,
                    revision: 0,
                },
                mesh_id: mesh.id.clone(),
                vertex_ids: vec![42, 7],
                positions: vec![],
                uvs: vec![],
                triangles: vec![],
                space: GeometrySpace::Model,
            },
            replacement: TopologyReplacement {
                mesh,
                binding: None,
                blend_bindings: vec![],
                glues: vec![],
                vertex_mapping: HashMap::from([(42, Some(99)), (7, None)]),
            },
        }),
    };
    let value = serde_json::to_value(operation).unwrap();
    let decoded: EditOperation = serde_json::from_value(value.clone()).unwrap();
    let EditOperation::Modeling { operation } = decoded else {
        panic!("wrong operation")
    };
    let ModelingCommand::ReplaceTopology { replacement, .. } = *operation else {
        panic!("wrong modeling operation")
    };
    assert_eq!(
        replacement.vertex_mapping,
        HashMap::from([(42, Some(99)), (7, None)])
    );
    for key in ["-1", "4294967296", "not-a-vertex"] {
        let mut invalid = value.clone();
        invalid["operation"]["replacement"]["vertex_mapping"] = serde_json::json!({key:null});
        assert!(serde_json::from_value::<EditOperation>(invalid).is_err());
    }
}
#[test]
fn integer_offscreen_maps_deserialize_through_tagged_content_intents() {
    let value = serde_json::json!({"domain":"content","intent":{"operation":"delete_offscreen_keyforms","offscreen_id":"test","slots":[1],"policy":{"kind":"remap","replacements":{"3":7,"9":null}}}});
    let intent: zipzi_sdk::ModelingIntent = serde_json::from_value(value).unwrap();
    let zipzi_sdk::ModelingIntent::Content(zipzi_sdk::SceneContentEdit::DeleteOffscreenKeyforms {
        policy: zipzi_sdk::OffscreenDeletePolicy::Remap { replacements },
        ..
    }) = intent
    else {
        panic!("wrong policy")
    };
    assert_eq!(
        replacements.into_iter().collect::<Vec<_>>(),
        vec![(3, Some(7)), (9, None)]
    );
}

#[test]
fn ordinary_batch_slots_do_not_inline_complete_modeling_records() {
    assert!(std::mem::size_of::<EditOperation>() <= 256);
    assert!(std::mem::size_of::<ModelingCommand>() > 512);
}

use zipzi_contract::{
    document::{OffscreenDeletePolicy, SceneContentEdit},
    kernel_commands::Command,
};

#[test]
fn offscreen_slot_maps_roundtrip_inside_typed_command_batches() {
    let command = Command::SceneContent {
        edit: SceneContentEdit::DeleteOffscreenKeyforms {
            offscreen_id: uuid::Uuid::new_v4().to_string(),
            slots: vec![3, 9],
            policy: OffscreenDeletePolicy::Remap {
                replacements: [(3, Some(7)), (9, None)].into(),
            },
        },
    };
    let value = serde_json::to_value(vec![command]).unwrap();
    let commands: Vec<Command> = serde_json::from_value(value.clone()).unwrap();
    let [Command::SceneContent {
        edit:
            SceneContentEdit::DeleteOffscreenKeyforms {
                policy: OffscreenDeletePolicy::Remap { replacements },
                ..
            },
    }] = commands.as_slice()
    else {
        panic!("wrong command or policy");
    };
    assert_eq!(replacements, &[(3, Some(7)), (9, None)].into());
    for key in ["-1", "18446744073709551616", "not-a-slot"] {
        let mut invalid = value.clone();
        invalid[0]["edit"]["policy"]["replacements"] = serde_json::json!({key: null});
        assert!(serde_json::from_value::<Vec<Command>>(invalid).is_err());
    }
}

//! Build a self-contained animation package from the repository's small model.
use std::path::PathBuf;

use zipzi_core::document::{MotionGroup, MotionRegistration, PackageAttachment};
use zipzi_project::{
    import_expression3, import_motion3, import_physics3, import_pose3, ProjectImporter,
    TransferFormat, TransferSnapshot,
};

const EXPRESSION: &str = "00000000-0000-4000-8000-000000000e21";
const MOTION: &str = "00000000-0000-4000-8000-000000000e22";
const PHYSICS: &str = "00000000-0000-4000-8000-000000000e23";
const POSE: &str = "00000000-0000-4000-8000-000000000e24";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("package destination required")?,
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("modules directory")?
        .parent()
        .ok_or("repo root")?
        .to_path_buf();
    let (result, imported, _) = ProjectImporter::default()
        .import_model3(&root.join("fixtures/external_v50/model.model3.json"));
    if !result.status.is_ok() || !result.resources_complete() {
        return Err(format!("{result:?}").into());
    }
    let imported = imported.ok_or("missing imported model")?;
    let mut builder = imported.document.builder();
    builder
        .set_missing_attachments(Vec::new())
        .map_err(|s| s.message)?;
    let mut candidate = builder.freeze().map_err(|s| s.message)?.state;
    candidate = import_expression3(
        &candidate,
        EXPRESSION,
        "Smile",
        &std::fs::read_to_string(root.join("fixtures/animation_cpu/minimal.exp3.json"))?,
    )?
    .candidate;
    candidate = import_motion3(
        &candidate,
        MOTION,
        "Idle",
        &std::fs::read_to_string(root.join("fixtures/animation_cpu/loop.motion3.json"))?,
    )?
    .candidate;
    candidate = import_physics3(
        &candidate,
        PHYSICS,
        &std::fs::read_to_string(root.join("fixtures/animation_cpu/thirty.physics3.json"))?,
    )?
    .candidate;
    candidate = import_pose3(
        &candidate,
        POSE,
        r#"{"Type":"Live2D Pose","Groups":[[{"Id":"Part0"}]]}"#,
    )?
    .candidate;
    let sound_id = uuid::Uuid::new_v4().to_string();
    let data_id = uuid::Uuid::new_v4().to_string();
    let groups = vec![MotionGroup {
        id: uuid::Uuid::new_v4().to_string(),
        name: "Idle".into(),
        entries: vec![MotionRegistration {
            id: uuid::Uuid::new_v4().to_string(),
            clip_id: MOTION.into(),
            fade_in: None,
            fade_out: None,
            sound: Some(zipzi_core::document::AttachmentRef::Resolved {
                attachment_id: sound_id.clone(),
            }),
            extensions: Default::default(),
        }],
    }];
    let param = candidate
        .parameter(&candidate.parameter_order()[0])
        .ok_or("parameter")?
        .runtime_id
        .clone();
    let hit = candidate
        .mesh(&candidate.mesh_order()[0])
        .ok_or("drawable")?
        .runtime_id
        .clone();
    let mut settings = zipzi_project::model3::import_settings(
        &candidate,
        &serde_json::json!({
            "Groups": [{"Target":"Parameter","Name":"EyeBlink","Ids":[param]}],
            "Layout": {"CenterX": 0.0, "Width": 2.0},
            "HitAreas": [{"Id": hit, "Name": "Head"}],
            "FileReferences": {"UserData": "data.userdata3.json"}
        }),
    )
    .map_err(|status| status.message)?;
    settings.user_data = Some(zipzi_core::document::AttachmentRef::Resolved {
        attachment_id: data_id.clone(),
    });
    let mut builder = candidate.builder();
    builder.set_motion_groups(groups).map_err(|s| s.message)?;
    builder
        .set_model3_settings(settings)
        .map_err(|s| s.message)?;
    builder
        .set_package_attachments(vec![
            PackageAttachment {
                id: sound_id,
                path: "sounds/probe.wav".into(),
                bytes: Some(std::sync::Arc::new(b"RIFF\0\0\0\0WAVE".to_vec())),
            },
            PackageAttachment {
                id: data_id,
                path: "data.userdata3.json".into(),
                bytes: Some(std::sync::Arc::new(
                    br#"{"Version":3,"UserData":[]}"#.to_vec(),
                )),
            },
        ])
        .map_err(|s| s.message)?;
    let candidate = builder.freeze().map_err(|s| s.message)?.state;
    TransferSnapshot::new(candidate, imported.resources, imported.protected_paths)
        .map_err(|s| s.message)?
        .publish(TransferFormat::Runtime, &destination)
        .map_err(|s| s.message)?;
    println!("{}", destination.display());
    Ok(())
}

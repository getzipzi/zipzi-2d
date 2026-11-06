//! Optional integration with the locally installed Nijiiro Mao sample.
//! Source assets stay under models/local; generated artifacts stay under target.
use std::{collections::BTreeMap, fs, path::PathBuf};
use zipzi_cmo3::inspect_cubism;
use zipzi_project::{
    encode_project_cubism, publish_cubism, Cmo3Error, CubismOptions, LossPolicy, ProjectImporter,
    ProjectIo, ResourceStore,
};

#[test]
fn local_mao_exports_linked_editable_cubism_projects() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = root.join("models/local/mao/runtime/mao_pro.model3.json");
    if !source.is_file() {
        eprintln!(
            "Skipping optional Mao Cubism export: {} not installed",
            source.display()
        );
        return;
    }
    let output = root.join("target/mao-cubism-integration");
    fs::create_dir_all(&output).unwrap();
    let importer = ProjectImporter::default();
    let (imported, prepared, _) = importer.import_model3(&source);
    assert!(imported.status.is_ok(), "{imported:?}");
    let prepared = prepared.unwrap();
    let document = &prepared.document;
    assert!(document.builder().freeze().is_ok());
    assert!(document.missing_attachments().is_empty());
    assert_eq!(document.mesh_order().len(), 260);
    let images: std::collections::HashSet<_> = document
        .mesh_order()
        .iter()
        .map(|id| document.mesh(id).unwrap().texture_asset_id.clone())
        .collect();
    assert_eq!(
        images.len(),
        260,
        "Every imported Mesh needs independent artwork"
    );
    assert_eq!(document.glue_order().len(), 7);
    assert_eq!(document.motion_order().len(), 7);
    assert_eq!(document.expression_order().len(), 8);
    let before = document.clone();
    let mut poses = vec![std::collections::HashMap::new()];
    for runtime in ["ParamAngleX", "ParamAngleY", "ParamAngleZ", "ParamEyeLOpen"] {
        let parameter = document
            .parameter_order()
            .iter()
            .find_map(|id| document.parameter(id).filter(|p| p.runtime_id == runtime))
            .unwrap();
        for value in [parameter.minimum, parameter.maximum] {
            poses.push(std::collections::HashMap::from([(
                parameter.id.clone(),
                value,
            )]));
        }
    }
    // Exercise representative additive morph drivers as well as ordinary grids.
    for kind in [
        zipzi_core::BlendShapeTargetKind::Mesh,
        zipzi_core::BlendShapeTargetKind::Warp,
        zipzi_core::BlendShapeTargetKind::Rotation,
    ] {
        let Some(binding) = document
            .blend_binding_order()
            .iter()
            .map(|id| document.blend_binding(id).unwrap())
            .find(|b| b.target_kind == kind)
        else {
            continue;
        };
        let table = document.blend_key_table(&binding.key_table_id).unwrap();
        let parameter = document.parameter(&table.parameter_id).unwrap();
        for value in [parameter.minimum, parameter.maximum] {
            let mut pose = std::collections::HashMap::from([(parameter.id.clone(), value)]);
            for id in &binding.constraint_ids {
                let constraint = document.blend_constraint(id).unwrap();
                let index = constraint
                    .weights
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .unwrap()
                    .0;
                pose.insert(constraint.parameter_id.clone(), constraint.keys[index]);
            }
            poses.push(pose);
        }
    }
    let mut geometry = Vec::new();
    for pose in poses {
        let mut frame = zipzi_core::DrawableFrame::default();
        assert!(zipzi_core::evaluation::evaluate_frame_including_hidden(
            document, &pose, &mut frame
        )
        .is_ok());
        assert_eq!(frame.drawables.len(), 260);
        let canvas = document.setup().source_canvas.unwrap();
        let positions: BTreeMap<_, Vec<[f32; 2]>> = frame
            .drawables
            .iter()
            .map(|mesh| {
                (
                    mesh.runtime_id.clone(),
                    mesh.positions
                        .iter()
                        .map(|point| [point.x + canvas.origin.x, canvas.origin.y - point.y])
                        .collect(),
                )
            })
            .collect();
        let parameters: BTreeMap<_, _> = pose
            .iter()
            .map(|(id, value)| (document.parameter(id).unwrap().runtime_id.clone(), *value))
            .collect();
        let mut drawables: Vec<_> = frame.drawables.iter().collect();
        drawables.sort_by_key(|mesh| mesh.render_order);
        let render_order: Vec<_> = drawables.iter().map(|mesh| &mesh.runtime_id).collect();
        geometry.push(serde_json::json!({"parameters":parameters,"positions":positions,"render_order":render_order}));
    }
    fs::write(
        output.join("expected-geometry.json"),
        serde_json::to_vec(&geometry).unwrap(),
    )
    .unwrap();
    let preflight = inspect_cubism(document).unwrap();
    // The full runtime sample contains expressions, pose and model3 metadata.
    // Strict conversion must report these instead of silently dropping them.
    assert!(preflight.has_loss());
    match zipzi_project::encode_document_cubism(
        document,
        &prepared.resources,
        &CubismOptions::default(),
    ) {
        Err(Cmo3Error::Unsupported(report)) => assert_eq!(report, preflight),
        other => panic!("Mao strict export should report known losses: {other:?}"),
    }
    let options = CubismOptions {
        base_name: "mao".into(),
        bake_animation: false,
        auto_frame_rate: false,
        model: zipzi_project::Cmo3Options {
            model_name: "Nijiiro Mao".into(),
            loss_policy: LossPolicy::Allow,
            ..Default::default()
        },
    };
    let artifact =
        zipzi_project::encode_document_cubism(document, &prepared.resources, &options).unwrap();
    assert_eq!(&artifact.cmo3.bytes[..4], b"CAFF");
    assert_eq!(&artifact.can3.bytes[..4], b"CAFF");
    for loss in preflight.diagnostics {
        assert!(
            artifact.report.diagnostics.contains(&loss),
            "missing {loss:?}"
        );
    }
    assert_eq!(document.content_id(), before.content_id());

    // Exercise the actual saved Zipzi project -> paired Cubism boundary too.
    let temporary = std::env::temp_dir().join(format!("zipzi-mao-cubism-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&temporary).unwrap();
    let project = temporary.join("project.zipzi");
    ProjectIo::new(&temporary, ResourceStore::default())
        .save(document, &prepared.resources, &project, None, || true)
        .unwrap();
    let reopened = encode_project_cubism(&project, &options).unwrap();
    assert_eq!(reopened.cmo3.bytes, artifact.cmo3.bytes);
    assert_eq!(reopened.can3.bytes, artifact.can3.bytes);
    assert_eq!(reopened.report, artifact.report);
    publish_cubism(&artifact, &output.join("editable")).unwrap();
    fs::remove_dir_all(temporary).unwrap();

    let mut diagnostics = BTreeMap::<String, usize>::new();
    for diagnostic in &artifact.report.diagnostics {
        *diagnostics.entry(diagnostic.code.clone()).or_default() += 1;
    }
    let summary = serde_json::json!({
        "source": "models/local/mao/runtime/mao_pro.model3.json",
        "meshes": document.mesh_order().len(),
        "transforms": document.transform_order().len(),
        "parameters": document.parameter_order().len(),
        "parts": document.part_order().len(),
        "glues": document.glue_order().len(),
        "blend_bindings": document.blend_binding_order().len(),
        "motions": document.motion_order().len(),
        "expressions": document.expression_order().len(),
        "scenes": document.motion_order().iter().map(|id| document.motion(id).unwrap().name.clone()).collect::<Vec<_>>(),
        "cmo3_bytes": artifact.cmo3.bytes.len(),
        "can3_bytes": artifact.can3.bytes.len(),
        "strict_export": "rejected_with_loss_report",
        "allow_export": "passed",
        "saved_project_equivalence": "passed",
        "source_document_unchanged": "passed",
        "geometry_reference_poses": geometry.len(),
        "unsupported_motion_targets": document.motion_order().iter().flat_map(|id| {
            let clip=document.motion(id).unwrap();
            clip.tracks.iter().filter(|track| artifact.report.diagnostics.iter().any(|d|d.code=="CAN3_UNSUPPORTED_TARGET" && d.object_id==track.id))
                .map(|track|serde_json::json!({"clip":clip.name,"target":track.target}))
        }).collect::<Vec<_>>(),
        "diagnostics_by_code": diagnostics,
    });
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary).unwrap(),
    )
    .unwrap();
    eprintln!("Mao editable Cubism export: {summary}");
}

//! Local editable-project import and Mao geometry reference for the optional Cubism probe.
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let mut imported = zipzi_cmo3::import_model(&std::fs::read(path).unwrap()).unwrap();
    if let Some(animation) = std::env::args().nth(2) {
        zipzi_cmo3::import_animation(&std::fs::read(animation).unwrap(), &mut imported.document)
            .unwrap();
    }
    println!(
        "blend {} motions {} expressions {} physics {}",
        imported.document.blend_binding_order().len(),
        imported.document.motion_order().len(),
        imported.document.expression_order().len(),
        imported
            .document
            .physics()
            .map_or(0, |p| p.data.settings.len())
    );
    println!(
        "parameters {} parts {} transforms {} meshes {} glue {}",
        imported.document.parameter_order().len(),
        imported.document.part_order().len(),
        imported.document.transform_order().len(),
        imported.document.mesh_order().len(),
        imported.document.glue_order().len()
    );

    if let Some(output) = std::env::args().nth(3) {
        let doc = &imported.document;
        let canvas = doc.source_canvas().unwrap();
        let mut cases = Vec::new();
        for (name, value) in [
            ("", 0.),
            ("ParamAngleX", -30.),
            ("ParamAngleX", 30.),
            ("ParamMouthA", 1.),
        ] {
            let mut preview = std::collections::HashMap::new();
            let mut params = std::collections::BTreeMap::new();
            if !name.is_empty() {
                if let Some(p) = doc
                    .parameter_order()
                    .iter()
                    .map(|id| doc.get_parameter(id).unwrap())
                    .find(|p| p.runtime_id == name)
                {
                    preview.insert(p.id.clone(), value);
                    params.insert(name, value);
                }
            }
            let mut frame = zipzi_core::DrawableFrame::default();
            let status =
                zipzi_core::evaluation::evaluate_frame_including_hidden(doc, &preview, &mut frame);
            assert!(status.is_ok(), "{:?}", status);
            let positions: std::collections::BTreeMap<_, _> = frame
                .drawables
                .iter()
                .map(|d| {
                    let m = doc.get_mesh(&d.id).unwrap();
                    (
                        m.runtime_id.clone(),
                        d.positions
                            .iter()
                            .map(|p| [p.x + canvas.origin.x, canvas.origin.y - p.y])
                            .collect::<Vec<_>>(),
                    )
                })
                .collect();
            cases.push(serde_json::json!({"parameters":params,"positions":positions}));
        }
        std::fs::write(output, serde_json::to_vec(&cases).unwrap()).unwrap();
    }
}

use zipzi_editor_core::{
    CanvasInput, Document, EditorCommand, ObjectKind, ObjectRef, ViewportMetrics, ViewportPoint,
};

#[test]
fn mao_artmesh266_is_editable_at_its_initial_parameter_values() -> anyhow::Result<()> {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/local/mao/mao_pro_t06.cmo3");
    if !source.exists() {
        eprintln!("SKIP: local Mao Cubism project unavailable");
        return Ok(());
    }
    // Exercise initial authoring independently of the host's separate archive
    // workspace limits and the adjacent animation project's absolute links.
    let imported = zipzi_cmo3::import_model(&std::fs::read(&source)?)?;
    let store = zipzi_sdk::ResourceStore::default();
    let mut builder = imported.document.builder();
    let mut images = Vec::new();
    for id in imported.document.asset_order() {
        let mut asset = imported.document.asset(id).unwrap().clone();
        let zipzi_core::state::AssetContent::Unresolved { source } = &asset.content else {
            continue;
        };
        let image = store.adopt_image(zipzi_project::prepare_png_content(
            imported.images[source.as_str()].clone().into(),
            store.budget(),
        )?)?;
        asset.content = zipzi_core::state::AssetContent::Image {
            resource: image.key(),
            width: image.width(),
            height: image.height(),
        };
        builder.replace_asset(asset)?;
        images.push(image);
    }
    let fixture = tempfile::tempdir()?;
    let path = fixture.path().join("mao.zipzi");
    let resources = zipzi_sdk::ResourceView::default().with_images(images);
    let io = zipzi_sdk::ProjectIo::new(fixture.path(), store);
    io.save(&builder.freeze()?.state, &resources, &path, None, || true)?;
    let mut doc = Document::open(&path)?;
    let initial = doc.snapshot()?;
    let mesh = initial
        .viewport_frame
        .drawables
        .iter()
        .find(|m| m.runtime_id == "ArtMesh266")
        .unwrap();
    let object = ObjectRef::new(ObjectKind::Mesh, &mesh.id);
    let (original_mesh, original_blends, parameter, binding) = {
        let host = doc.runtime_host();
        let snapshot = super::support::published_snapshot(&host, doc.runtime_session_id())?;
        let model = snapshot.document();
        let parameter = model
            .parameter_order()
            .iter()
            .filter_map(|id| model.parameter(id))
            .find(|p| p.runtime_id == "ParamMouthUp")
            .unwrap();
        assert_eq!(parameter.default_value, 1.);
        let original_mesh = model.mesh(&object.id).unwrap().clone();
        let original_blends = model
            .blend_bindings_for_target(&object.id)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let binding = original_blends
            .iter()
            .find(|b| model.blend_key_table(&b.key_table_id).unwrap().parameter_id == parameter.id)
            .unwrap()
            .id
            .clone();
        (
            original_mesh,
            original_blends,
            parameter.id.clone(),
            binding,
        )
    };
    doc.select_object(object.clone())?;
    assert!(doc.context().parameter_focus.is_none());
    assert!(doc.context().active_binding.is_none());
    let mut editor = super::support::editor_from_document(doc);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 1200.,
        height: 900.,
        scale_factor: 1.,
    });
    super::support::present_canvas(&mut editor)?;
    let overlay = editor.overlay();
    assert!(overlay.unavailable.is_none(), "{:?}", overlay.unavailable);
    let label = overlay.target_label.clone();
    assert!(
        label
            .as_deref()
            .is_some_and(|s| s.contains("Mouth Corner Upward = 1")),
        "{label:?}"
    );
    let start = overlay.vertices[0].position;
    super::support::pointer_input(
        &mut editor,
        CanvasInput::Down {
            primary: false,
            point: start,
            shift: false,
            alt: false,
        },
    )?;
    let selected = editor.overlay().active_component.unwrap();
    assert_eq!(selected.object, object);
    let slot = original_mesh
        .vertex_ids
        .iter()
        .position(|id| *id == selected.id)
        .unwrap();
    let end = ViewportPoint {
        x: start.x + 10.,
        y: start.y + 5.,
    };
    super::support::pointer_input(&mut editor, CanvasInput::Up { point: end })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    let moved = super::support::present_canvas(&mut editor)?;
    let actual = moved
        .transform
        .to_viewport(moved.geometry.mesh(&object.id).unwrap().vertices[slot].1);
    assert!(
        (actual.x - end.x).hypot(actual.y - end.y) < 0.05,
        "{actual:?} != {end:?}"
    );
    {
        let doc = editor.document().unwrap();
        let host = doc.runtime_host();
        let snapshot = super::support::published_snapshot(&host, doc.runtime_session_id())?;
        let model = snapshot.document();
        assert_eq!(model.mesh(&object.id), Some(&original_mesh));
        for original in &original_blends {
            let changed = model.blend_binding(&original.id).unwrap();
            if original.id == binding {
                assert_ne!(changed, original);
                let (
                    zipzi_core::DeltaKeyforms::Mesh(before),
                    zipzi_core::DeltaKeyforms::Mesh(after),
                ) = (&original.keyforms, &changed.keyforms)
                else {
                    panic!()
                };
                assert_eq!(after[0], before[0]);
            } else {
                assert_eq!(changed, original);
            }
        }
    }
    editor.execute(EditorCommand::Undo)?;
    let restored = super::support::present_canvas(&mut editor)?;
    assert_eq!(
        restored.geometry.mesh(&object.id).unwrap().vertices,
        initial
            .viewport_frame
            .drawables
            .iter()
            .find(|m| m.id == object.id)
            .unwrap()
            .positions
            .iter()
            .enumerate()
            .map(|(i, p)| (
                original_mesh.vertex_ids[i],
                zipzi_editor_core::ModelPoint { x: p.x, y: p.y }
            ))
            .collect::<Vec<_>>()
    );
    // Focusing the slider without changing its value chooses the same target.
    editor.execute(EditorCommand::FocusParameter(parameter.clone()))?;
    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id: parameter,
        value: 1.,
    })?;
    super::support::present_canvas(&mut editor)?;
    assert_eq!(editor.overlay().target_label, label);
    Ok(())
}

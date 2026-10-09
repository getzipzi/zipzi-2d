use editor_core::Document;

/// The artwork is intentionally local; use --ignored to verify the reported file.
#[test]
#[ignore = "requires models/local/heijao/heijiao.psd"]
fn heijiao_imports_with_clipping_and_survives_save_reopen() -> anyhow::Result<()> {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/local/heijao/heijiao.psd");
    let (mut document, notices) = Document::import_psd(&source)?;
    assert!(notices.is_empty(), "{notices:?}");
    assert!(document.path().is_none());
    assert!(document.modified());
    assert_eq!(document.import_name(), Some("heijiao"));
    assert_eq!(
        (document.canvas().width, document.canvas().height),
        (2003.0, 3835.0)
    );
    let imported = document.snapshot()?;
    assert!(imported.textures.len() > 100);
    let layers = document.layers();
    let mut ordered: Vec<_> = imported.viewport_frame.drawables.iter().collect();
    ordered.sort_by_key(|d| d.render_order);
    assert_eq!(
        ordered.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        layers
            .iter()
            .map(|layer| layer.id.as_str())
            .collect::<Vec<_>>()
    );
    let neck_id = &layers
        .iter()
        .find(|layer| layer.name == "脖子阴影")
        .unwrap()
        .id;
    let neck = imported
        .viewport_frame
        .drawables
        .iter()
        .find(|d| &d.id == neck_id)
        .unwrap();
    assert_eq!(neck.raw_blend_mode, Some(256));
    assert!(neck.masks.is_empty());
    assert!(
        imported
            .viewport_frame
            .offscreens
            .iter()
            .any(|group| group.owner_part_id == neck.part_id)
    );
    let base = imported
        .viewport_frame
        .drawables
        .iter()
        .find(|d| d.part_id == neck.part_id && d.raw_blend_mode.is_none())
        .unwrap();
    assert_eq!(
        layers
            .iter()
            .find(|layer| layer.id == base.id)
            .unwrap()
            .name,
        "脖子底色"
    );
    let clipped = imported
        .viewport_frame
        .drawables
        .iter()
        .filter(|d| d.raw_blend_mode.is_some_and(|mode| mode >> 8 == 1))
        .count();
    assert!(clipped > 10);
    let output = tempfile::tempdir()?;
    let receipt = document.save(&output.path().join("heijiao.kasane"))?;
    let mut reopened = Document::open(&receipt.manifest)?;
    let restored = reopened.snapshot()?;
    assert_eq!(
        restored.viewport_frame.drawables.len(),
        imported.viewport_frame.drawables.len()
    );
    assert_eq!(restored.textures.len(), imported.textures.len());
    for (before, after) in imported
        .viewport_frame
        .drawables
        .iter()
        .zip(&restored.viewport_frame.drawables)
    {
        assert_eq!(before.masks, after.masks);
        assert_eq!(before.raw_blend_mode, after.raw_blend_mode);
    }
    assert_eq!(
        imported.viewport_frame.offscreens,
        restored.viewport_frame.offscreens
    );
    assert_eq!(
        imported.viewport_frame.render_plan,
        restored.viewport_frame.render_plan
    );
    for (before, after) in imported.textures.iter().zip(restored.textures.iter()) {
        assert_eq!(before.rgba, after.rgba);
    }
    println!(
        "Imported {} layers and {clipped} clipped layers",
        imported.textures.len()
    );
    Ok(())
}

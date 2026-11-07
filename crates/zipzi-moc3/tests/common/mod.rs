pub mod import_cases;
pub mod purism;

#[allow(dead_code)]
pub fn prepare(
    state: &zipzi_core::state::DocumentState,
    write: impl FnOnce(&mut zipzi_core::state::DocumentBuilder) -> Result<(), zipzi_core::Status>,
) -> zipzi_core::state::DocumentState {
    let mut builder = state.builder();
    write(&mut builder).unwrap();
    builder.freeze().unwrap().state
}

/// Attach actual imported pixels, or generated pixels for model-only fixtures,
/// before exercising native project persistence and strict MOC3 export.
#[allow(dead_code)]
pub fn capture_images(
    state: &zipzi_core::state::DocumentState,
) -> (
    zipzi_core::state::DocumentState,
    zipzi_project::ResourceView,
) {
    use zipzi_core::state::AssetContent;
    let store = zipzi_project::ResourceStore::default();
    let mut builder = state.builder();
    let mut images = Vec::new();
    for id in state.asset_order() {
        let mut asset = state.asset(id).unwrap().clone();
        let image = match &asset.content {
            AssetContent::Unresolved { source } => {
                match store.import_image(std::path::Path::new(source)) {
                    Ok(image) => image,
                    Err(_) => continue,
                }
            }
            AssetContent::Image { width, height, .. } => store
                .prepare_rgba(
                    *width,
                    *height,
                    vec![255; *width as usize * *height as usize * 4].into(),
                )
                .unwrap(),
            AssetContent::Empty => continue,
        };
        asset.content = AssetContent::Image {
            resource: image.key(),
            width: image.width(),
            height: image.height(),
        };
        builder.replace_asset(asset).unwrap();
        images.push(image);
    }
    (
        builder.freeze().unwrap().state,
        zipzi_project::ResourceView::default().with_images(images),
    )
}
#[allow(dead_code)]
pub fn native_roundtrip(
    state: &zipzi_core::state::DocumentState,
) -> zipzi_core::state::DocumentState {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "zipzi-moc3-roundtrip-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&root).unwrap();
    let (state, resources) = capture_images(state);
    let io = zipzi_project::ProjectIo::new(&root, zipzi_project::ResourceStore::default());
    let path = root.join("project.zipzi");
    io.save(&state, &resources, &path, None, || true).unwrap();
    let loaded = io.load(&path, || true).unwrap();
    std::fs::remove_dir_all(root).unwrap();
    loaded.document
}

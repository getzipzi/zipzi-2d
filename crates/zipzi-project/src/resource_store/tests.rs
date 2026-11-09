use super::*;
use zipzi_contract::kernel::{AssetContent, AssetRecord};

fn model(image: &ImageResource) -> DocumentState {
    let state =
        DocumentState::new("00000000-0000-0000-0000-000000000001", Default::default()).unwrap();
    let mut builder = state.builder();
    builder
        .create_asset(AssetRecord {
            id: "00000000-0000-0000-0000-000000000002".into(),
            name: "image".into(),
            content: AssetContent::Image {
                resource: image.key(),
                width: image.width(),
                height: image.height(),
            },
        })
        .unwrap();
    builder.freeze().unwrap().state
}

#[test]
fn views_retain_versions_after_store_and_source_handles_drop() {
    let budget = ImageMemoryBudget::new(1024 * 1024);
    let store = ResourceStore::new(budget.clone());
    let first = store.prepare_rgba(1, 1, Arc::from([1, 2, 3, 255])).unwrap();
    let key = first.key();
    let weak = Arc::downgrade(&first.0);
    let state = model(&first);
    let old = ResourceView::default().with_images([first]);
    let second = store.prepare_rgba(1, 1, Arc::from([4, 5, 6, 255])).unwrap();
    assert_ne!(key, second.key());
    let new_key = second.key();
    let candidate = old.with_images([second]);
    let mut builder = state.builder();
    let mut asset = state.asset(&state.asset_order()[0]).unwrap().clone();
    asset.content = AssetContent::Image {
        resource: new_key,
        width: 1,
        height: 1,
    };
    builder.replace_asset(asset).unwrap();
    let new_state = builder.freeze().unwrap().state;
    let new = candidate.for_state(&new_state).unwrap();
    drop(candidate);
    drop(store);
    assert_eq!(old.len(), 1);
    assert_eq!(new.len(), 1);
    assert_eq!(&*old.image(key).unwrap().texture().rgba, &[1, 2, 3, 255]);
    assert_eq!(
        &*new.image(new_key).unwrap().texture().rgba,
        &[4, 5, 6, 255]
    );
    assert_eq!(budget.usage().rgba_bytes, 8);
    drop(old);
    assert!(weak.upgrade().is_none());
    assert_eq!(budget.usage().rgba_bytes, 4);
    drop(new);
    assert_eq!(budget.usage().total_bytes(), 0);
}

#[test]
fn compressible_generated_image_saves_with_limited_encoding_headroom() {
    let root = tempfile::tempdir().unwrap();
    let pixels: Arc<[u8]> = vec![123; 1024 * 1024 * 4].into();
    let budget = ImageMemoryBudget::new(pixels.len() + (1 << 20));
    let store = ResourceStore::new(budget.clone());
    let image = store.prepare_rgba(1024, 1024, pixels.clone()).unwrap();
    let state = model(&image);
    let resources = ResourceView::default().with_images([image]);
    let path = root.path().join("generated.zipzi");
    let saved = crate::archive::ProjectIo::new(root.path(), store)
        .save(&state, &resources, &path, None, || true)
        .unwrap();
    assert_eq!(budget.usage().preparing_bytes, 0);
    assert_eq!(budget.usage().png_bytes as u64, saved.asset_bytes);

    let loaded = crate::archive::ProjectIo::new(root.path(), ResourceStore::default())
        .load(&path, || true)
        .unwrap();
    assert!(loaded.diagnostics.is_empty());
    let asset = loaded.document.asset(&state.asset_order()[0]).unwrap();
    assert_eq!(loaded.resources.read_texture(asset).unwrap().rgba, pixels);
}

#[test]
fn external_file_is_captured_before_becoming_a_resource() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.png");
    let store = ResourceStore::default();
    let original = store
        .prepare_rgba(1, 1, Arc::from([11, 22, 33, 255]))
        .unwrap();
    let encoded = original.read_asset().unwrap();
    std::fs::write(&path, &encoded.bytes).unwrap();
    let imported = store.import_image(&path).unwrap();
    let key = imported.key();
    std::fs::write(&path, b"corrupt replacement").unwrap();
    assert!(store.import_image(&path).is_err());
    std::fs::remove_file(path).unwrap();
    drop(store);
    assert_eq!(imported.read_asset().unwrap(), encoded);
    assert_eq!(imported.key(), key);
}

#[test]
fn pairing_rejects_missing_or_mismatched_resources() {
    let store = ResourceStore::default();
    let image = store.prepare_rgba(1, 1, Arc::from([1, 2, 3, 255])).unwrap();
    let state = model(&image);
    assert_eq!(
        ResourceView::default().for_state(&state).unwrap_err().code,
        "MISSING_RESOURCE"
    );
    let view = ResourceView::default().with_images([image]);
    assert_eq!(view.for_state(&state).unwrap().len(), 1);
    let mut builder = state.builder();
    let mut asset = state.asset(&state.asset_order()[0]).unwrap().clone();
    if let AssetContent::Image { width, .. } = &mut asset.content {
        *width = 2;
    }
    builder.replace_asset(asset).unwrap();
    let mismatch = builder.freeze().unwrap().state;
    assert_eq!(
        view.for_state(&mismatch).unwrap_err().code,
        "RESOURCE_DIMENSIONS"
    );
    assert_eq!(view.for_state(&state).unwrap().len(), 1);
}

#[test]
fn unresolved_assets_need_no_placeholder_resource() {
    let state =
        DocumentState::new("00000000-0000-0000-0000-000000000001", Default::default()).unwrap();
    let mut builder = state.builder();
    builder
        .create_asset(AssetRecord {
            id: "00000000-0000-0000-0000-000000000002".into(),
            name: "missing".into(),
            content: AssetContent::Unresolved {
                source: "missing.png".into(),
            },
        })
        .unwrap();
    let state = builder.freeze().unwrap().state;
    assert!(ResourceView::default()
        .for_state(&state)
        .unwrap()
        .is_empty());
}

#[test]
fn adoption_shares_pixels_and_enforces_the_store_budget() {
    let pixels: Arc<[u8]> = Arc::from([1, 2, 3, 255]);
    let prepared =
        PreparedImageContent::from_rgba(1, 1, pixels.clone(), &ImageMemoryBudget::default())
            .unwrap();
    let small = ResourceStore::new(ImageMemoryBudget::new(3));
    assert!(small.adopt_image(prepared.clone()).is_err());
    assert_eq!(small.budget().usage().total_bytes(), 0);
    let store = ResourceStore::new(ImageMemoryBudget::new(4));
    let image = store.adopt_image(prepared).unwrap();
    assert!(Arc::ptr_eq(&pixels, &image.texture().rgba));
    assert_eq!(store.budget().usage().rgba_bytes, 4);
}

#[test]
fn multiple_asset_ids_can_share_one_resource_version() {
    let image = ResourceStore::default()
        .prepare_rgba(1, 1, Arc::from([1, 2, 3, 255]))
        .unwrap();
    let state = model(&image);
    let mut builder = state.builder();
    let mut duplicate = state.asset(&state.asset_order()[0]).unwrap().clone();
    duplicate.id = "00000000-0000-0000-0000-000000000003".into();
    builder.create_asset(duplicate).unwrap();
    let state = builder.freeze().unwrap().state;
    let view = ResourceView::default().with_images([image]);
    assert_eq!(view.for_state(&state).unwrap().len(), 1);
}

#[test]
fn resources_are_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<ResourceStore>();
    check::<ResourceView>();
    check::<ImageResource>();
}

#[test]
fn escaped_texture_keeps_its_resource_budget_responsibility() {
    let budget = ImageMemoryBudget::new(4);
    let store = ResourceStore::new(budget.clone());
    let image = store.prepare_rgba(1, 1, Arc::from([1, 2, 3, 255])).unwrap();
    let texture = image.texture();
    let view = ResourceView::default().with_images([image]);
    drop(store);
    drop(view);
    assert_eq!(budget.usage().rgba_bytes, 4);
    assert_eq!(&*texture.rgba, &[1, 2, 3, 255]);
    drop(texture);
    assert_eq!(budget.usage().total_bytes(), 0);
}

#[test]
fn texture_reads_bind_resource_versions_and_validate_record_dimensions() {
    let store = ResourceStore::default();
    let red = store
        .prepare_rgba(1, 1, Arc::from([255, 0, 0, 255]))
        .unwrap();
    let state = model(&red);
    let asset = state.asset(&state.asset_order()[0]).unwrap();
    let old = ResourceView::default().with_images([red.clone()]);
    let pixels = old.read_texture(asset).unwrap();
    assert!(Arc::ptr_eq(&pixels.rgba, &red.texture().rgba));
    let blue = store
        .prepare_rgba(1, 1, Arc::from([0, 0, 255, 255]))
        .unwrap();
    let mut changed = asset.clone();
    changed.content = AssetContent::Image {
        resource: blue.key(),
        width: 1,
        height: 1,
    };
    let new = ResourceView::default().with_images([blue]);
    assert_eq!(
        new.read_texture(asset).unwrap_err().code,
        "MISSING_RESOURCE"
    );
    assert_eq!(
        &*new.read_texture(&changed).unwrap().rgba,
        &[0, 0, 255, 255]
    );
    if let AssetContent::Image { width, .. } = &mut changed.content {
        *width = 2;
    }
    assert_eq!(
        new.read_texture(&changed).unwrap_err().code,
        "RESOURCE_DIMENSIONS"
    );
    changed.content = AssetContent::Empty;
    assert_eq!(
        old.read_texture(&changed).unwrap_err().code,
        "MISSING_RESOURCE"
    );
    drop(old);
    drop(red);
    drop(new);
    drop(store);
    assert_eq!(&*pixels.rgba, &[255, 0, 0, 255]);
}

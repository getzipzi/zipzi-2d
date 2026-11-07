use crate::support::prepared;
use std::sync::Arc;
use zipzi_core::{state::DocumentState, ModelSetup};
use zipzi_project::{ImageMemoryBudget, ImageResource, PreparedImageContent, ResourceStore};
use zipzi_sdk::{AuthoringSession, HistoryLimits, Snapshot};
const DOC: &str = "61000000-0000-4000-8000-000000000001";
const IMAGE: &str = "61000000-0000-4000-8000-000000000002";
fn session() -> AuthoringSession {
    let mut s = AuthoringSession::new(
        Snapshot::new(
            DocumentState::new(DOC, ModelSetup::default().into()).unwrap(),
            Default::default(),
        )
        .unwrap(),
    );
    s.set_history_limits(HistoryLimits {
        max_steps: 2,
        target_bytes: 1 << 20,
    });
    s
}
fn image(value: u8, store: &ResourceStore) -> ImageResource {
    store
        .prepare_rgba(1, 1, Arc::from([value, 0, 0, 255]))
        .unwrap()
}
fn insert(
    c: &mut zipzi_sdk::Commands<'_>,
    id: &str,
    image: ImageResource,
) -> Result<(), zipzi_core::Status> {
    c.create_empty_asset(id.into(), "image".into())?;
    c.replace_image(id, image)
}
fn pixels(s: &Snapshot) -> zipzi_project::TextureData {
    s.resources()
        .read_texture(s.document().asset(IMAGE).unwrap())
        .unwrap()
}
#[test]
fn hundred_distinct_cancelled_images_return_to_baseline_without_closing() {
    let budget = ImageMemoryBudget::new(1 << 20);
    let store = ResourceStore::new(budget.clone());
    let s = session();
    let version = s.version();
    for n in 0..100 {
        let mut d = s.begin_draft();
        d.batch(d.source(), |c| insert(c, IMAGE, image(n, &store)))
            .unwrap();
        let snapshot = d.read_snapshot().unwrap();
        assert_eq!(pixels(&snapshot.snapshot).rgba.as_ref(), &[n, 0, 0, 255]);
        assert_eq!(budget.usage().rgba_bytes, 4);
        drop(snapshot);
        drop(d);
        assert_eq!(budget.usage().total_bytes(), 0);
    }
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state().undo_steps, 0);
}
#[test]
fn candidate_and_snapshot_pin_resources_before_publication() {
    let budget = ImageMemoryBudget::new(1 << 20);
    let store = ResourceStore::new(budget.clone());
    let s = session();
    let mut d = s.begin_draft();
    d.batch(d.source(), |c| insert(c, IMAGE, image(17, &store)))
        .unwrap();
    let snapshot = d.read_snapshot().unwrap().snapshot;
    drop(d);
    drop(store);
    drop(s);
    assert_eq!(
        snapshot
            .resources()
            .read_asset(snapshot.document().asset(IMAGE).unwrap())
            .unwrap()
            .rgba,
        [17, 0, 0, 255]
    );
    let pixels = pixels(&snapshot).rgba;
    drop(snapshot);
    assert_eq!(budget.usage().png_bytes, 0);
    assert_eq!(budget.usage().rgba_bytes, 4);
    drop(pixels);
    assert_eq!(budget.usage().total_bytes(), 0);
}
#[test]
fn history_eviction_retains_undo_candidates_and_snapshots_then_reclaims() {
    let budget = ImageMemoryBudget::new(1 << 20);
    let store = ResourceStore::new(budget.clone());
    let mut s = session();
    let (_, p) = prepared(&s, |c| insert(c, IMAGE, image(0, &store))).unwrap();
    s.publish(p).unwrap();
    let snapshot = s.read_snapshot();
    let candidate = s.begin_draft();
    for n in 1..=100 {
        let (_, p) = prepared(&s, |c| c.replace_image(IMAGE, image(n, &store))).unwrap();
        s.publish(p).unwrap();
        assert!(budget.usage().rgba_bytes <= 16);
        assert_eq!(pixels(&snapshot.snapshot).rgba[0], 0);
    }
    assert_eq!(s.history_state().undo_steps, 2);
    s.undo(s.version()).unwrap();
    assert_eq!(pixels(s.snapshot()).rgba[0], 99);
    s.redo(s.version()).unwrap();
    assert_eq!(pixels(s.snapshot()).rgba[0], 100);
    assert_eq!(
        pixels(&candidate.read_snapshot().unwrap().snapshot).rgba[0],
        0
    );
    drop(candidate);
    drop(snapshot);
    assert_eq!(budget.usage().rgba_bytes, 12);
    drop(s);
    assert_eq!(budget.usage().total_bytes(), 0);
}
#[test]
fn budget_rejection_and_png_retry_leave_no_preparation_or_history_changes() {
    let budget = ImageMemoryBudget::new(4);
    let store = ResourceStore::new(budget.clone());
    let mut s = session();
    let img = image(7, &store);
    assert_eq!(
        store
            .prepare_rgba(1, 1, Arc::from([99, 0, 0, 255]))
            .unwrap_err()
            .code,
        "RESOURCE_BUDGET_EXCEEDED"
    );
    for _ in 0..2 {
        assert_eq!(
            img.read_asset().unwrap_err().code,
            "RESOURCE_BUDGET_EXCEEDED"
        );
        assert_eq!(budget.usage().preparing_bytes, 0);
    }
    let (_, p) = prepared(&s, |c| insert(c, IMAGE, img)).unwrap();
    s.publish(p).unwrap();
    let version = s.version();
    assert_eq!(
        s.snapshot()
            .resources()
            .read_asset(s.snapshot().document().asset(IMAGE).unwrap())
            .unwrap_err()
            .code,
        "RESOURCE_BUDGET_EXCEEDED"
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state().undo_steps, 1);
    drop(s);
    assert_eq!(budget.usage().total_bytes(), 0);
}
#[test]
fn store_admission_cannot_bypass_its_budget_with_unlimited_preparation() {
    let img = PreparedImageContent::from_rgba(1, 1, Arc::from([8, 0, 0, 255]), &Default::default())
        .unwrap();
    let budget = ImageMemoryBudget::new(3);
    let store = ResourceStore::new(budget.clone());
    assert_eq!(
        store.adopt_image(img).unwrap_err().code,
        "RESOURCE_BUDGET_EXCEEDED"
    );
    assert_eq!(budget.usage().total_bytes(), 0);
    assert_eq!(budget.usage().allocations, 0);
}
#[test]
fn cached_png_and_rgba_admission_is_atomic_on_budget_rejection() {
    let img = PreparedImageContent::from_rgba(1, 1, Arc::from([8, 0, 0, 255]), &Default::default())
        .unwrap();
    img.read_asset().unwrap();
    let budget = ImageMemoryBudget::new(4);
    assert!(ResourceStore::new(budget.clone()).adopt_image(img).is_err());
    assert_eq!(budget.usage().total_bytes(), 0);
    assert_eq!(budget.usage().allocations, 0);
}
#[test]
fn prepared_images_survive_other_commits_and_unused_handles_release_memory() {
    let budget = ImageMemoryBudget::new(1 << 20);
    let store = ResourceStore::new(budget.clone());
    let mut s = session();
    let second = "61000000-0000-4000-8000-000000000003";
    let a = image(1, &store);
    let b = image(2, &store);
    let unused = image(3, &store);
    let (_, p) = prepared(&s, |c| insert(c, IMAGE, a)).unwrap();
    s.publish(p).unwrap();
    assert_eq!(budget.usage().rgba_bytes, 12);
    drop(unused);
    assert_eq!(budget.usage().rgba_bytes, 8);
    let (_, p) = prepared(&s, |c| insert(c, second, b)).unwrap();
    s.publish(p).unwrap();
    assert_eq!(pixels(s.snapshot()).rgba.as_ref(), &[1, 0, 0, 255]);
    s.undo(s.version()).unwrap();
    s.redo(s.version()).unwrap();
    assert_eq!(
        s.snapshot()
            .resources()
            .read_texture(s.snapshot().document().asset(second).unwrap())
            .unwrap()
            .rgba
            .as_ref(),
        &[2, 0, 0, 255]
    );
}
#[test]
fn captured_disk_image_pins_encoded_bytes_and_pixels_after_source_is_replaced() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("image.png");
    let budget = ImageMemoryBudget::new(1 << 20);
    let store = ResourceStore::new(budget.clone());
    std::fs::write(
        &path,
        image(17, &ResourceStore::default())
            .read_asset()
            .unwrap()
            .bytes,
    )
    .unwrap();
    let captured = store.import_image(&path).unwrap();
    let png = captured.read_asset().unwrap().bytes;
    std::fs::write(&path, b"changed after capture").unwrap();
    assert_eq!(captured.texture().rgba.as_ref(), &[17, 0, 0, 255]);
    assert_eq!(captured.read_asset().unwrap().bytes, png);
    assert!(store.import_image(&path).is_err());
    drop(captured);
    assert_eq!(budget.usage().total_bytes(), 0);
}

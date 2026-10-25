use std::sync::Arc;
use zipzi_core::ModelSetup;
use zipzi_project::{ImageMemoryBudget, PreparedImageContent};
use zipzi_sdk::{AuthoringSession, HistoryLimits};
const DOC: &str = "61000000-0000-4000-8000-000000000001";
const IMAGE: &str = "61000000-0000-4000-8000-000000000002";
fn session(budget: &ImageMemoryBudget) -> AuthoringSession {
    let mut sdk = AuthoringSession::with_history_limits(
        DOC,
        ModelSetup::default(),
        HistoryLimits {
            max_steps: 2,
            max_bytes: 1 << 20,
        },
    )
    .unwrap();
    sdk.set_image_memory_budget(budget.clone()).unwrap();
    sdk
}
fn image(value: u8, budget: &ImageMemoryBudget) -> PreparedImageContent {
    PreparedImageContent::from_rgba(1, 1, Arc::from([value, 0, 0, 255]), budget).unwrap()
}
#[test]
fn a17_hundred_distinct_cancelled_images_return_to_baseline_without_closing() {
    let budget = ImageMemoryBudget::new(1 << 20);
    let sdk = session(&budget);
    let version = sdk.version();
    for n in 0..100 {
        let prepared = sdk
            .prepare_rgba_image(1, 1, Arc::from([n, 0, 0, 255]))
            .unwrap();
        let mut edit = sdk.begin_owned_edit("draft", None).unwrap();
        edit.create_prepared_image(IMAGE.into(), "draft".into(), &prepared)
            .unwrap();
        drop(prepared);
        sdk.collect_image_resources();
        let snapshot = sdk.candidate_snapshot(&edit).unwrap();
        assert_eq!(
            snapshot
                .memory_image(IMAGE)
                .unwrap()
                .texture()
                .rgba
                .as_ref(),
            &[n, 0, 0, 255]
        );
        assert_eq!(budget.usage().rgba_bytes, 4);
        drop(snapshot);
        drop(edit);
        sdk.collect_image_resources();
        assert_eq!(sdk.memory_image_entries(), 0);
        assert_eq!(budget.usage().total_bytes(), 0);
    }
    assert_eq!(sdk.version(), version);
    assert_eq!(sdk.history_lengths(), (0, 0));
}
#[test]
fn a14_candidate_prepared_image_and_snapshot_pin_before_registration() {
    let budget = ImageMemoryBudget::new(1 << 20);
    let sdk = session(&budget);
    let image = image(17, &budget);
    let mut edit = sdk.begin_owned_edit("prepared", None).unwrap();
    edit.create_prepared_image(IMAGE.into(), "image".into(), &image)
        .unwrap();
    let snapshot = sdk.candidate_snapshot(&edit).unwrap();
    drop(image);
    drop(edit);
    sdk.collect_image_resources();
    assert_eq!(sdk.memory_image_entries(), 0);
    assert_eq!(
        snapshot
            .memory_image(IMAGE)
            .unwrap()
            .read_asset()
            .unwrap()
            .rgba,
        [17, 0, 0, 255]
    );
    let pixels = snapshot.memory_image(IMAGE).unwrap().texture().rgba;
    drop(snapshot);
    assert_eq!(budget.usage().png_bytes, 0);
    assert_eq!(budget.usage().rgba_bytes, 4);
    drop(pixels);
    assert_eq!(budget.usage().total_bytes(), 0);
}
#[test]
fn a17_history_eviction_retains_undo_candidates_and_snapshots_then_reclaims() {
    let budget = ImageMemoryBudget::new(1 << 20);
    let mut sdk = session(&budget);
    sdk.edit("create", None, |e| {
        e.create_prepared_image(IMAGE.into(), "image".into(), &image(0, &budget))
    })
    .unwrap();
    let snapshot = sdk.read_snapshot();
    let candidate = sdk.begin_owned_edit("old candidate", None).unwrap();
    for n in 1..=100 {
        sdk.edit("replace", None, |e| {
            e.replace_prepared_image(IMAGE, &image(n, &budget))
        })
        .unwrap();
        sdk.collect_image_resources();
        assert!(sdk.memory_image_entries() <= 4);
        assert!(budget.usage().rgba_bytes <= 16);
        assert_eq!(snapshot.memory_image(IMAGE).unwrap().texture().rgba[0], 0);
    }
    assert_eq!(sdk.history_lengths(), (2, 0));
    sdk.undo().unwrap();
    assert_eq!(sdk.read_texture(IMAGE).unwrap().rgba[0], 99);
    sdk.redo().unwrap();
    assert_eq!(sdk.read_texture(IMAGE).unwrap().rgba[0], 100);
    assert_eq!(
        sdk.candidate_snapshot(&candidate)
            .unwrap()
            .memory_image(IMAGE)
            .unwrap()
            .texture()
            .rgba[0],
        0
    );
    drop(candidate);
    drop(snapshot);
    sdk.collect_image_resources();
    assert_eq!(sdk.memory_image_entries(), 3);
    assert_eq!(budget.usage().rgba_bytes, 12);
}
#[test]
fn a17_budget_rejection_and_png_retry_leave_no_preparation_or_history_changes() {
    let budget = ImageMemoryBudget::new(4);
    let mut sdk = session(&budget);
    let prepared = image(7, &budget);
    assert_eq!(image_error(&budget), "RESOURCE_BUDGET_EXCEEDED");
    assert_eq!(
        prepared.read_asset().unwrap_err().code,
        "RESOURCE_BUDGET_EXCEEDED"
    );
    assert_eq!(budget.usage().preparing_bytes, 0);
    sdk.edit("create", None, |e| {
        e.create_prepared_image(IMAGE.into(), "image".into(), &prepared)
    })
    .unwrap();
    let version = sdk.version();
    assert_eq!(
        sdk.read_asset(IMAGE).unwrap_err().code.as_ref(),
        "RESOURCE_BUDGET_EXCEEDED"
    );
    assert_eq!(sdk.version(), version);
    assert_eq!(sdk.history_lengths(), (1, 0));
    let larger = ImageMemoryBudget::new(1 << 20);
    sdk.set_image_memory_budget(larger.clone()).unwrap();
    assert_eq!(sdk.read_asset(IMAGE).unwrap().rgba, [7, 0, 0, 255]);
    assert!(larger.usage().png_bytes > 0);
    assert_eq!(larger.usage().preparing_bytes, 0);
}
fn image_error(budget: &ImageMemoryBudget) -> String {
    PreparedImageContent::from_rgba(1, 1, Arc::from([99, 0, 0, 255]), budget)
        .unwrap_err()
        .code
}

#[test]
fn owned_candidate_cannot_publish_images_using_its_previous_unlimited_budget() {
    let mut sdk = AuthoringSession::new(DOC, ModelSetup::default()).unwrap();
    let mut candidate = sdk.begin_owned_edit("before hosting", None).unwrap();
    let prepared = image(8, &Default::default());
    candidate
        .create_prepared_image(IMAGE.into(), "image".into(), &prepared)
        .unwrap();
    let limited = ImageMemoryBudget::new(3);
    sdk.set_image_memory_budget(limited.clone()).unwrap();
    assert_eq!(
        candidate.commit_to(&mut sdk).unwrap_err().code.as_ref(),
        "RESOURCE_BUDGET_EXCEEDED"
    );
    assert!(sdk.asset(IMAGE).is_none());
    assert_eq!(sdk.history_lengths(), (0, 0));
    assert_eq!(limited.usage().total_bytes(), 0);
}

#[test]
fn cached_png_and_rgba_admission_is_atomic_on_budget_rejection() {
    let prepared = image(8, &Default::default());
    prepared.read_asset().unwrap();
    let budget = ImageMemoryBudget::new(4);
    assert!(prepared.with_budget(&budget).is_err());
    assert_eq!(budget.usage().total_bytes(), 0);
    assert_eq!(budget.usage().allocations, 0);
}

#[test]
fn prepared_images_survive_other_commits_and_unused_leases_release_memory() {
    let budget = ImageMemoryBudget::new(1 << 20);
    let mut sdk = session(&budget);
    let second = "61000000-0000-4000-8000-000000000003";
    let a = sdk
        .prepare_rgba_image(1, 1, Arc::from([1, 0, 0, 255]))
        .unwrap();
    let b = sdk
        .prepare_rgba_image(1, 1, Arc::from([2, 0, 0, 255]))
        .unwrap();
    let unused = sdk
        .prepare_rgba_image(1, 1, Arc::from([3, 0, 0, 255]))
        .unwrap();
    assert_eq!(sdk.memory_image_entries(), 0);
    sdk.edit("A", None, |e| {
        e.create_prepared_image(IMAGE.into(), "A".into(), &a)
    })
    .unwrap();
    drop(a);
    sdk.collect_image_resources();
    assert_eq!(budget.usage().rgba_bytes, 12);
    drop(unused);
    assert_eq!(budget.usage().rgba_bytes, 8);
    sdk.edit("B", None, |e| {
        e.create_prepared_image(second.into(), "B".into(), &b)
    })
    .unwrap();
    drop(b);
    sdk.collect_image_resources();
    assert_eq!(
        sdk.read_texture(IMAGE).unwrap().rgba.as_ref(),
        &[1, 0, 0, 255]
    );
    assert_eq!(
        sdk.read_texture(second).unwrap().rgba.as_ref(),
        &[2, 0, 0, 255]
    );
    sdk.undo().unwrap();
    sdk.collect_image_resources();
    sdk.redo().unwrap();
    assert_eq!(
        sdk.read_texture(second).unwrap().rgba.as_ref(),
        &[2, 0, 0, 255]
    );
}

#[test]
fn captured_disk_image_pins_encoded_bytes_and_pixels_after_source_is_replaced() {
    let folder = std::env::temp_dir().join(format!("zipzi-disk-lease-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&folder).unwrap();
    let budget = ImageMemoryBudget::new(1 << 20);
    let mut sdk = session(&budget);
    let prepared = image(17, &budget);
    sdk.edit("image", None, |e| {
        e.create_prepared_image(IMAGE.into(), "image".into(), &prepared)
    })
    .unwrap();
    drop(prepared);
    sdk.save_project(&folder.join("project"), None).unwrap();
    let asset = sdk.asset(IMAGE).unwrap();
    let root = sdk.project_path().unwrap().parent().unwrap();
    let path = root.join(&asset.content.as_ref().unwrap().source);
    let captured = sdk.capture_image_content(&asset, &budget).unwrap();
    let png = captured.read_asset().unwrap().bytes;
    std::fs::write(&path, b"changed after capture").unwrap();
    assert_eq!(captured.texture().rgba.as_ref(), [17, 0, 0, 255]);
    assert_eq!(captured.read_asset().unwrap().bytes, png);
    assert!(sdk.capture_image_content(&asset, &budget).is_err());
    drop(captured);
    drop(sdk);
    assert_eq!(budget.usage().total_bytes(), 0);
    std::fs::remove_dir_all(folder).unwrap();
}

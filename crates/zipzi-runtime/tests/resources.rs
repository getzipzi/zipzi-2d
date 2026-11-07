use zipzi_contract::kernel_commands::{Command, ImageInput};
use zipzi_runtime::*;
use zipzi_sdk::SourcedSnapshot;

fn id(n: u32) -> String {
    format!("79400000-0000-4000-8000-{n:012x}")
}

fn setup() -> (Runtime, ClientId, Version) {
    let mut rt = Runtime::default();
    let client = rt.register_client().unwrap();
    let session = rt
        .create_session(client, &id(1), Default::default())
        .unwrap();
    let version = rt.subscribe(client, session).unwrap().cursor;
    (rt, client, version)
}

fn create() -> Command {
    Command::CreateAsset {
        id: id(2),
        name: "Texture".into(),
    }
}

fn replace(rgba: [u8; 4]) -> Command {
    Command::ReplaceImage {
        id: id(2),
        input: ImageInput::Rgba {
            width: 1,
            height: 1,
            bytes: rgba.to_vec(),
        },
    }
}

fn pixels(source: &SourcedSnapshot) -> std::sync::Arc<[u8]> {
    let asset = source.snapshot.document().asset(&id(2)).unwrap();
    source
        .snapshot
        .resources()
        .read_texture(asset)
        .unwrap()
        .rgba
}

#[test]
fn draft_snapshots_and_history_keep_distinct_pixels_until_the_last_owner() {
    let (mut rt, client, version) = setup();
    let budget = rt.resources().budget().clone();
    let a = [12, 34, 56, 255];
    let b = [250, 128, 64, 255];
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt
        .commands(client, draft, vec![create(), replace(a)])
        .unwrap();
    let first = rt.read_draft(client, draft).unwrap();
    let committed = rt.publish(client, draft, "First image").unwrap();
    let draft = rt.begin_draft(client, committed.version).unwrap();
    let (draft, _) = rt.commands(client, draft, vec![replace(b)]).unwrap();
    let second = rt.read_draft(client, draft).unwrap();
    assert_eq!(pixels(&first).as_ref(), a);
    assert_eq!(pixels(&second).as_ref(), b);
    assert_eq!(
        pixels(&rt.read_snapshot(client, version.session).unwrap()).as_ref(),
        a
    );
    let committed = rt.publish(client, draft, "Second image").unwrap();
    assert_eq!(committed.history.undo_steps, 2);
    assert_eq!(budget.usage().rgba_bytes, 8);
    let undone = rt.undo(client, committed.version).unwrap();
    assert_eq!(
        pixels(&rt.read_snapshot(client, version.session).unwrap()).as_ref(),
        a
    );
    rt.redo(client, undone.version).unwrap();
    assert_eq!(
        pixels(&rt.read_snapshot(client, version.session).unwrap()).as_ref(),
        b
    );
    rt.close_session(client, version.session, true).unwrap();
    assert_eq!(pixels(&first).as_ref(), a);
    assert_eq!(pixels(&second).as_ref(), b);
    drop(first);
    assert_eq!(budget.usage().rgba_bytes, 4);
    drop(second);
    assert_eq!(budget.usage().total_bytes(), 0);
}

#[test]
fn close_and_cancel_sweep_pixel_allocations_without_a_followup_budget_query() {
    let (mut rt, client, version) = setup();
    let budget = rt.transfer_budget();
    for commit in [false, true] {
        let draft = rt.begin_draft(client, version).unwrap();
        let (draft, _) = rt
            .commands(client, draft, vec![create(), replace([1, 2, 3, 255])])
            .unwrap();
        if commit {
            rt.publish(client, draft, "Image").unwrap();
            rt.close_session(client, version.session, true).unwrap();
        } else {
            rt.cancel_draft(client, draft.draft).unwrap();
        }
        // A stale Weak<[u8]> would pin its inline RGBA allocation until this
        // explicit sweep. Lifecycle cleanup must already have removed it.
        assert_eq!(budget.collect_unused(), 0);
    }
}

#[test]
fn foreign_blob_admission_keeps_the_candidate_and_owners_bytes_intact() {
    let (mut rt, client, version) = setup();
    let other = rt.register_client().unwrap();
    let blob = rt.create_blob(other, 4).unwrap();
    rt.append_blob(other, blob, 0, &[1, 2, 3, 255]).unwrap();
    rt.seal_blob(other, blob).unwrap();
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt.commands(client, draft, vec![create()]).unwrap();
    assert_eq!(
        rt.replace_image_blob(client, draft, id(2), blob, 1, 1)
            .unwrap_err()
            .code,
        "NOT_OWNER"
    );
    assert_eq!(
        rt.read_draft(client, draft).unwrap().source,
        QuerySource::Draft(draft)
    );
    assert_eq!(rt.blob(other, blob).unwrap().bytes(), &[1, 2, 3, 255]);
    rt.release_client(other);
    assert_eq!(rt.task_bytes(), 0);
    rt.cancel_draft(client, draft.draft).unwrap();
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
}

#[test]
fn a_hundred_cancelled_drafts_release_pixels_after_readers_drop() {
    let (mut rt, client, version) = setup();
    let budget = rt.resources().budget().clone();
    for index in 0..100 {
        let draft = rt.begin_draft(client, version).unwrap();
        let (draft, _) = rt
            .commands(client, draft, vec![create(), replace([index, 2, 3, 255])])
            .unwrap();
        let frozen = rt.read_draft(client, draft).unwrap();
        rt.cancel_draft(client, draft.draft).unwrap();
        assert_eq!(pixels(&frozen)[0], index);
        assert_eq!(budget.usage().rgba_bytes, 4);
        drop(frozen);
        assert_eq!(budget.usage().total_bytes(), 0);
    }
    assert_eq!(
        rt.subscribe(client, version.session)
            .unwrap()
            .history
            .undo_steps,
        0
    );
}

#[test]
fn stale_resource_command_cannot_apply_after_another_batch() {
    let (mut rt, client, version) = setup();
    let draft = rt.begin_draft(client, version).unwrap();
    let (draft, _) = rt
        .commands(client, draft, vec![create(), replace([1, 2, 3, 255])])
        .unwrap();
    let (current, _) = rt
        .commands(client, draft, vec![replace([4, 5, 6, 255])])
        .unwrap();
    let before = rt.read_draft(client, current).unwrap();
    assert_eq!(
        rt.commands(client, draft, vec![replace([7, 8, 9, 255])])
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert_eq!(
        rt.publish(client, current, "Rejected draft")
            .unwrap_err()
            .code,
        "DRAFT_ENDED"
    );
    assert_eq!(pixels(&before).as_ref(), &[4, 5, 6, 255]);
    assert_eq!(
        rt.subscribe(client, version.session).unwrap().cursor,
        version
    );
}

#[test]
fn client_expiry_reclaims_only_its_bulk_buffers_and_their_actual_lifetimes() {
    let mut rt = Runtime::new(RuntimeLimits {
        task_bytes: 8,
        ..Default::default()
    });
    let a = rt.register_client().unwrap();
    let b = rt.register_client().unwrap();
    let first = rt.create_blob(a, 4).unwrap();
    let second = rt.create_blob(b, 4).unwrap();
    for (owner, blob) in [(a, first), (b, second)] {
        rt.append_blob(owner, blob, 0, b"rgba").unwrap();
        rt.seal_blob(owner, blob).unwrap();
    }
    let held = rt.blob(a, first).unwrap();
    assert_eq!(rt.create_blob(a, 1).unwrap_err().code, "BUDGET_EXCEEDED");
    rt.release_client(a);
    assert_eq!(rt.task_bytes(), 8);
    assert_eq!(rt.blob(b, first).err().unwrap().code, "BLOB_NOT_FOUND");
    assert_eq!(rt.blob(b, second).unwrap().bytes(), b"rgba");
    drop(held);
    assert_eq!(rt.task_bytes(), 4);
    rt.release_client(b);
    assert_eq!(rt.task_bytes(), 0);
}

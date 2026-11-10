use super::*;
use std::sync::Arc;
use zipzi_core::{
    state::{AssetContent, AssetRecord, DocumentState, OwnedDocument},
    Mesh, ModelSetup, Parameter, Part, Vec2,
};
use zipzi_project::{ResourceStore, ResourceView};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn snapshot() -> Snapshot {
    let state = DocumentState::from_owned(OwnedDocument {
        id: id(1),
        parameters: vec![Parameter {
            id: id(2),
            name: "parameter".into(),
            ..Default::default()
        }],
        parts: vec![Part {
            id: id(3),
            runtime_id: "part".into(),
            name: "part".into(),
            ..Default::default()
        }],
        meshes: vec![mesh()],
        assets: vec![AssetRecord {
            id: id(5),
            name: "image".into(),
            content: AssetContent::Empty,
        }],
        ..Default::default()
    })
    .unwrap();
    Snapshot::new(state, ResourceView::default()).unwrap()
}
fn mesh() -> Mesh {
    Mesh {
        id: id(4),
        runtime_id: "mesh".into(),
        part_id: id(3),
        name: "mesh".into(),
        vertex_ids: vec![1, 2, 3],
        base_positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
        uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
        triangles: vec![[1, 2, 3]],
        ..Default::default()
    }
}
fn rename(draft: &mut Draft, name: &str) {
    draft
        .batch(draft.source(), |commands| {
            commands.rename(NameTarget::Mesh(&id(4)), name.into())
        })
        .unwrap();
}
fn commit(session: &mut AuthoringSession, name: &str) -> Publication {
    let mut draft = session.begin_draft();
    rename(&mut draft, name);
    session.publish(draft.prepare(name).unwrap()).unwrap()
}

#[test]
fn two_drafts_are_isolated_and_publication_rejects_stale_or_foreign_sources() {
    let initial = snapshot();
    let mut session = AuthoringSession::new(initial.clone());
    let mut first = session.begin_draft();
    let mut second = session.begin_draft();
    assert_ne!(first.source().draft, second.source().draft);
    rename(&mut first, "first");
    rename(&mut second, "second");
    assert_eq!(initial.document().mesh(&id(4)).unwrap().name, "mesh");
    assert_eq!(session.snapshot().content_id(), initial.content_id());
    let prepared = first.prepare("first").unwrap();
    let mut foreign = AuthoringSession::new(initial.clone());
    assert_eq!(
        foreign
            .publish(second.prepare("second").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    let stale = session.begin_draft().prepare("stale no-op").unwrap();
    let receipt = session.publish(prepared).unwrap();
    assert_eq!(receipt.version.revision, 1);
    assert_eq!(session.publish(stale).unwrap_err().code, "STALE_SOURCE");
    assert_eq!(
        session.snapshot().document().mesh(&id(4)).unwrap().name,
        "first"
    );
    assert_eq!(session.take_events().len(), 1);
    assert!(session.take_events().is_empty());
}

#[test]
fn batch_creates_forward_references_and_publishes_one_step() {
    let empty = Snapshot::new(
        DocumentState::new(id(1), ModelSetup::default()).unwrap(),
        ResourceView::default(),
    )
    .unwrap();
    let mut session = AuthoringSession::new(empty);
    let mut draft = session.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.create_mesh(mesh())?;
            c.create_part(Part {
                id: id(3),
                runtime_id: "part".into(),
                ..Default::default()
            })?;
            c.create_parameter(Parameter {
                id: id(2),
                ..Default::default()
            })
        })
        .unwrap();
    assert_eq!(draft.source().sequence, 1);
    let prepared = draft.prepare("create batch").unwrap();
    assert_eq!(prepared.writes().records().len(), 3);
    session.publish(prepared).unwrap();
    assert_eq!(session.history_state().undo_steps, 1);
    session.undo(session.version()).unwrap();
    assert!(session.snapshot().document().mesh_order().is_empty());
    session.redo(session.version()).unwrap();
    assert_eq!(session.snapshot().document().mesh_order(), &[id(4)]);
}

#[test]
fn command_failure_even_when_ignored_ends_the_entire_draft_and_preserves_redo() {
    let mut session = AuthoringSession::new(snapshot());
    commit(&mut session, "committed");
    session.undo(session.version()).unwrap();
    let before = session.read_snapshot();
    let history = session.history_state();
    let mut draft = session.begin_draft();
    rename(&mut draft, "discard me");
    let failure = draft
        .batch(draft.source(), |c| {
            let _ = c.rename(NameTarget::Mesh("missing"), "x".into());
            Ok(())
        })
        .unwrap_err();
    assert_eq!(failure.code, "MISSING_OBJECT");
    assert!(!draft.is_active());
    assert_eq!(draft.prepare("failed").unwrap_err().code, "DRAFT_ENDED");
    assert_eq!(session.read_snapshot().source, before.source);
    assert_eq!(session.history_state(), history);
    let mut invalid = session.begin_draft();
    rename(&mut invalid, "valid first batch");
    assert!(invalid
        .batch(invalid.source(), |c| c.create_mesh(Mesh {
            id: id(99),
            runtime_id: "bad".into(),
            part_id: id(999),
            ..Default::default()
        }))
        .is_err());
    assert!(!invalid.is_active());
    session.redo(session.version()).unwrap();
    assert_eq!(
        session.snapshot().document().mesh(&id(4)).unwrap().name,
        "committed"
    );
}

#[test]
fn stale_candidate_query_and_unwind_end_only_their_draft() {
    let session = AuthoringSession::new(snapshot());
    let mut draft = session.begin_draft();
    let source = draft.source();
    let old = draft.read_snapshot().unwrap();
    rename(&mut draft, "changed");
    assert_eq!(old.snapshot.document().mesh(&id(4)).unwrap().name, "mesh");
    assert_eq!(
        draft.batch(source, |_| Ok(())).unwrap_err().code,
        "STALE_SOURCE"
    );
    assert!(!draft.is_active());
    let mut draft = session.begin_draft();
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = draft.batch(draft.source(), |c| -> Result<(), Status> {
            c.rename(NameTarget::Mesh(&id(4)), "discard".into())?;
            panic!("operation panic");
        });
    }));
    assert!(unwind.is_err());
    assert!(!draft.is_active());
    assert_eq!(session.version().revision, 0);
}

#[test]
fn noops_and_cancelled_publication_preserve_version_history_and_events() {
    let mut session = AuthoringSession::new(snapshot());
    commit(&mut session, "one");
    session.undo(session.version()).unwrap();
    session.take_events();
    let before = session.read_snapshot();
    let history = session.history_state();
    let mut draft = session.begin_draft();
    rename(&mut draft, "temporary");
    rename(&mut draft, "mesh");
    let receipt = session.publish(draft.prepare("no-op").unwrap()).unwrap();
    assert_eq!(receipt.kind, PublicationKind::Unchanged);
    assert_eq!(receipt.content, before.snapshot.content_id());
    let mut draft = session.begin_draft();
    rename(&mut draft, "cancelled");
    let mut calls = 0;
    let err = session
        .publish_with_control(draft.prepare("cancel").unwrap(), || {
            calls += 1;
            calls == 1
        })
        .unwrap_err();
    assert_eq!(err.code, "CANCELLED");
    assert_eq!(session.read_snapshot().source, before.source);
    assert_eq!(session.history_state(), history);
    assert!(session.take_events().is_empty());
}

#[test]
fn undo_restores_content_but_never_revives_old_sources() {
    let mut session = AuthoringSession::new(snapshot());
    let old = session.read_snapshot();
    let stale = session.begin_draft().prepare("old").unwrap();
    commit(&mut session, "one");
    let receipt = session.undo(session.version()).unwrap();
    assert_eq!(receipt.content, old.snapshot.content_id());
    assert_eq!(receipt.version.revision, 2);
    assert_ne!(QuerySource::Published(receipt.version), old.source);
    assert_eq!(session.publish(stale).unwrap_err().code, "STALE_SOURCE");
    commit(&mut session, "new branch");
    assert_eq!(session.history_state().redo_steps, 0);
    assert_eq!(session.redo(session.version()).unwrap_err().code, "NO_REDO");
}

#[test]
fn oversized_step_is_allowed_and_undo_redo_do_not_trim() {
    let mut session = AuthoringSession::new(snapshot());
    session.set_history_limits(HistoryLimits {
        max_steps: 50,
        target_bytes: 0,
    });
    commit(&mut session, "one");
    assert!(session.history_state().over_target);
    assert_eq!(session.history_state().undo_steps, 1);
    commit(&mut session, "two");
    assert_eq!(session.history_state().undo_steps, 1);
    let cost = session.history_state().estimated_bytes;
    session.undo(session.version()).unwrap();
    assert_eq!(
        session.snapshot().document().mesh(&id(4)).unwrap().name,
        "one"
    );
    assert_eq!(session.history_state().estimated_bytes, cost);
    session.redo(session.version()).unwrap();
    assert_eq!(session.history_state().estimated_bytes, cost);
    assert_eq!(
        session.snapshot().document().mesh(&id(4)).unwrap().name,
        "two"
    );
}

#[test]
fn publication_uses_latest_history_policy_and_cancel_keeps_other_drafts_valid() {
    let mut session = AuthoringSession::new(snapshot());
    let mut cancelled = session.begin_draft();
    let mut retained = session.begin_draft();
    rename(&mut cancelled, "cancelled");
    rename(&mut retained, "retained");
    cancelled.cancel();
    let prepared = retained.prepare("retained").unwrap();
    session.set_history_limits(HistoryLimits {
        max_steps: 1,
        target_bytes: 0,
    });
    session.publish(prepared).unwrap();
    assert!(session.history_state().over_target);
    assert_eq!(session.history_state().undo_steps, 1);
}

#[test]
fn explicit_history_maintenance_keeps_a_contiguous_interval() {
    let mut session = AuthoringSession::new(snapshot());
    for name in ["one", "two", "three", "four"] {
        commit(&mut session, name);
    }
    session.undo(session.version()).unwrap();
    session.undo(session.version()).unwrap();
    let current = session.snapshot().content_id();
    session.set_history_limits(HistoryLimits {
        max_steps: 1,
        target_bytes: usize::MAX,
    });
    assert_eq!(session.snapshot().content_id(), current);
    assert_eq!(
        (
            session.history_state().undo_steps,
            session.history_state().redo_steps
        ),
        (0, 1)
    );
    session.redo(session.version()).unwrap();
    assert_eq!(
        session.snapshot().document().mesh(&id(4)).unwrap().name,
        "three"
    );
    session.set_history_limits(HistoryLimits {
        max_steps: 0,
        target_bytes: 0,
    });
    assert_eq!(session.history_state().estimated_bytes, 0);
    commit(&mut session, "history disabled");
    assert_eq!(session.history_state().undo_steps, 0);
}

#[test]
fn vertex_gesture_uses_absolute_positions_and_honors_inherited_locks() {
    let mut session = AuthoringSession::new(snapshot());
    let mut draft = session.begin_draft();
    for x in [2., 3.] {
        draft
            .batch(draft.source(), |c| {
                c.set_vertex_positions(&id(4), &[(2, Vec2::new(x, 0.))])
            })
            .unwrap();
    }
    assert_eq!(
        draft
            .baseline()
            .document()
            .mesh(&id(4))
            .unwrap()
            .base_positions[1]
            .x,
        1.
    );
    session.publish(draft.prepare("drag").unwrap()).unwrap();
    assert_eq!(
        session
            .snapshot()
            .document()
            .mesh(&id(4))
            .unwrap()
            .base_positions[1]
            .x,
        3.
    );
    session.undo(session.version()).unwrap();
    assert_eq!(
        session
            .snapshot()
            .document()
            .mesh(&id(4))
            .unwrap()
            .base_positions[1]
            .x,
        1.
    );
    let mut builder = session.snapshot().document().builder();
    let mut editor = builder.editor_state().clone();
    editor.objects.insert(
        id(3),
        zipzi_core::document::ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    builder.set_editor_state(editor).unwrap();
    let locked = Snapshot::new(builder.freeze().unwrap().state, ResourceView::default()).unwrap();
    session.replace(session.version(), locked).unwrap();
    let mut draft = session.begin_draft();
    assert_eq!(
        draft
            .batch(draft.source(), |c| c
                .set_vertex_positions(&id(4), &[(1, Vec2::new(2., 2.))]))
            .unwrap_err()
            .code,
        "OBJECT_LOCKED"
    );
}

#[test]
fn image_versions_follow_snapshots_history_and_last_holder_lifetime() {
    let store = ResourceStore::default();
    let old_pixels: Arc<[u8]> = Arc::from([255, 0, 0, 255]);
    let weak = Arc::downgrade(&old_pixels);
    let old_image = store.prepare_rgba(1, 1, old_pixels).unwrap();
    let old_key = old_image.key();
    let mut session = AuthoringSession::new(snapshot());
    let mut draft = session.begin_draft();
    draft
        .batch(draft.source(), |c| c.replace_image(&id(5), old_image))
        .unwrap();
    session
        .publish(draft.prepare("old image").unwrap())
        .unwrap();
    let retained = session.read_snapshot();
    let new_image = store
        .prepare_rgba(1, 1, Arc::from([0, 0, 255, 255]))
        .unwrap();
    let new_key = new_image.key();
    let mut draft = session.begin_draft();
    draft
        .batch(draft.source(), |c| c.replace_image(&id(5), new_image))
        .unwrap();
    session
        .publish(draft.prepare("new image").unwrap())
        .unwrap();
    assert!(session.snapshot().resources().image(old_key).is_none());
    assert!(session.snapshot().resources().image(new_key).is_some());
    session.undo(session.version()).unwrap();
    assert!(session.snapshot().resources().image(old_key).is_some());
    session.redo(session.version()).unwrap();
    assert!(Snapshot::new(
        retained.snapshot.document().clone(),
        ResourceView::default()
    )
    .is_err());
    drop(session);
    drop(store);
    assert!(weak.upgrade().is_some());
    drop(retained);
    assert!(weak.upgrade().is_none());
}

#[test]
fn replacement_clears_history_advances_generation_and_invalidates_drafts() {
    let mut session = AuthoringSession::new(snapshot());
    commit(&mut session, "edit");
    let draft = session.begin_draft();
    let old = session.version();
    let receipt = session.replace(old, snapshot()).unwrap();
    assert_eq!(receipt.version.generation, old.generation + 1);
    assert_eq!(receipt.version.revision, old.revision + 1);
    assert_eq!(session.history_state().undo_steps, 0);
    assert_eq!(
        session
            .publish(draft.prepare("stale").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert_eq!(
        session.replace(old, snapshot()).unwrap_err().code,
        "STALE_SOURCE"
    );
}

#[test]
fn worker_values_are_send_and_sync() {
    fn check<T: Send + Sync>() {}
    check::<Snapshot>();
    check::<SourcedSnapshot>();
    check::<Draft>();
    check::<PreparedChange>();
    let mut session = AuthoringSession::new(snapshot());
    let mut draft = session.begin_draft();
    let worker = std::thread::spawn(move || {
        rename(&mut draft, "worker");
        draft.prepare("worker").unwrap()
    });
    let prepared = worker.join().unwrap();
    assert_eq!(
        session.snapshot().document().mesh(&id(4)).unwrap().name,
        "mesh"
    );
    session.publish(prepared).unwrap();
    assert_eq!(
        session.snapshot().document().mesh(&id(4)).unwrap().name,
        "worker"
    );
}

mod domains;

mod conversions;

mod scene;

mod deletion;
mod glue;
mod mesh;

mod metadata;

#[test]
fn prepared_preview_is_bound_to_one_candidate_and_retains_prior_net_writes() {
    let mut session = AuthoringSession::new(snapshot());
    let mut draft = session.begin_draft();
    let mut other = session.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.create_parameter(Parameter {
                id: id(99),
                name: "new".into(),
                ..Default::default()
            })
        })
        .unwrap();
    let (prepared, ()) = draft
        .prepare_preview(draft.source(), |c| {
            c.rename(NameTarget::Parameter(&id(99)), "preview".into())
        })
        .unwrap();
    assert_eq!(
        prepared
            .read_snapshot()
            .unwrap()
            .document()
            .parameter(&id(99))
            .unwrap()
            .name,
        "preview"
    );
    assert_eq!(
        other.install_preview(prepared).unwrap_err().code,
        "STALE_SOURCE"
    );
    assert!(other.is_active());
    let (prepared, ()) = draft
        .prepare_preview(draft.source(), |c| {
            c.rename(NameTarget::Parameter(&id(99)), "final".into())
        })
        .unwrap();
    draft.install_preview(prepared).unwrap();
    let receipt = session.publish(draft.prepare("preview").unwrap()).unwrap();
    assert!(receipt.writes.records().iter().any(
        |write| write.id == id(99) && write.change == zipzi_core::state::RecordChange::Created
    ));
    session.undo(session.version()).unwrap();
    assert!(session.snapshot().document().parameter(&id(99)).is_none());
}

#[test]
fn preview_tracks_ignored_command_failure_without_mutating_the_live_draft() {
    let session = AuthoringSession::new(snapshot());
    let mut draft = session.begin_draft();
    let source = draft.source();
    let failed = draft.prepare_preview(source, |c| {
        c.rename(NameTarget::Parameter(&id(2)), "temporary".into())?;
        let _ = c.rename(NameTarget::Parameter(&id(99)), "missing".into());
        Ok(())
    });
    assert_eq!(failed.unwrap_err().code, "MISSING_OBJECT");
    assert_eq!(draft.source(), source);
    assert_eq!(
        draft
            .read_snapshot()
            .unwrap()
            .snapshot
            .document()
            .parameter(&id(2))
            .unwrap()
            .name,
        "parameter"
    );
    let (prepared, ()) = draft.prepare_preview(source, |_| Ok(())).unwrap();
    draft
        .batch(source, |c| {
            c.rename(NameTarget::Parameter(&id(2)), "changed".into())
        })
        .unwrap();
    assert_eq!(
        draft.install_preview(prepared).unwrap_err().code,
        "STALE_SOURCE"
    );
    assert!(draft.is_active());
    assert_eq!(
        draft
            .read_snapshot()
            .unwrap()
            .snapshot
            .document()
            .parameter(&id(2))
            .unwrap()
            .name,
        "changed"
    );
}

#[test]
fn texture_snapshot_history_reads_each_captured_asset_version() {
    let store = ResourceStore::default();
    let red = store
        .prepare_rgba(1, 1, Arc::from([255, 0, 0, 255]))
        .unwrap();
    let blue = store
        .prepare_rgba(1, 1, Arc::from([0, 0, 255, 255]))
        .unwrap();
    let mut session = AuthoringSession::new(snapshot());
    let mut first = session.begin_draft();
    first
        .batch(first.source(), |c| c.replace_image(&id(5), red))
        .unwrap();
    session.publish(first.prepare("red").unwrap()).unwrap();
    let old = session.snapshot().clone();
    let mut second = session.begin_draft();
    second
        .batch(second.source(), |c| c.replace_image(&id(5), blue))
        .unwrap();
    let candidate = second.read_snapshot().unwrap().snapshot;
    let pixels = |snapshot: &Snapshot| {
        snapshot
            .resources()
            .read_texture(snapshot.document().asset(&id(5)).unwrap())
            .unwrap()
    };
    let red_pixels = pixels(&old);
    let blue_pixels = pixels(&candidate);
    assert_eq!(&*red_pixels.rgba, &[255, 0, 0, 255]);
    assert_eq!(&*blue_pixels.rgba, &[0, 0, 255, 255]);
    session.publish(second.prepare("blue").unwrap()).unwrap();
    session.undo(session.version()).unwrap();
    assert!(Arc::ptr_eq(
        &pixels(session.snapshot()).rgba,
        &red_pixels.rgba
    ));
    session.redo(session.version()).unwrap();
    assert!(Arc::ptr_eq(
        &pixels(session.snapshot()).rgba,
        &blue_pixels.rgba
    ));
    drop(session);
    drop(store);
    assert!(Arc::ptr_eq(&pixels(&old).rgba, &red_pixels.rgba));
    assert!(Arc::ptr_eq(&pixels(&candidate).rgba, &blue_pixels.rgba));
}

#[test]
fn one_candidate_sequence_has_one_snapshot_through_preview_and_publication() {
    let mut session = AuthoringSession::new(snapshot());
    let mut draft = session.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.rename(NameTarget::Mesh(&id(4)), "first".into())
        })
        .unwrap();
    let first = draft.read_snapshot().unwrap();
    assert_eq!(
        draft.read_snapshot().unwrap().snapshot.content_id(),
        first.snapshot.content_id()
    );
    let (prepared, ()) = draft
        .prepare_preview(draft.source(), |c| {
            c.rename(NameTarget::Mesh(&id(4)), "preview".into())
        })
        .unwrap();
    let displayed = prepared.read_snapshot().unwrap();
    assert_eq!(
        prepared.read_snapshot().unwrap().content_id(),
        displayed.content_id()
    );
    draft.install_preview(prepared).unwrap();
    let installed = draft.read_snapshot().unwrap();
    assert_ne!(installed.source, first.source);
    assert_eq!(installed.snapshot.content_id(), displayed.content_id());
    let receipt = session.publish(draft.prepare("confirm").unwrap()).unwrap();
    assert_eq!(receipt.content, displayed.content_id());
    assert_eq!(session.snapshot().content_id(), displayed.content_id());
}

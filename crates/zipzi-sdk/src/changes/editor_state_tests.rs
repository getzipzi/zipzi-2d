use super::*;
use crate::{AuthoringSession, EditSession};
use zipzi_core::{document::*, Mesh, ModelSetup, Parameter, Part, Transform};

fn id(n: u32) -> String {
    format!("80000000-0000-4000-8000-{n:012x}")
}

fn session() -> AuthoringSession {
    let mut sdk = AuthoringSession::new(&id(1), ModelSetup::default()).unwrap();
    sdk.edit("Fixture", None, |e| {
        e.create_mesh(Mesh {
            id: id(2),
            ..Default::default()
        })?;
        e.create_part(Part {
            id: id(3),
            ..Default::default()
        })?;
        e.create_transform(Transform {
            id: id(4),
            ..Default::default()
        })?;
        e.create_parameter(Parameter {
            id: id(5),
            ..Default::default()
        })
    })
    .unwrap();
    sdk
}

fn check(sdk: &mut AuthoringSession, edit: EditSession<'_>, local: bool) {
    let before = sdk.document();
    let after = edit.candidate_document();
    assert_eq!(after.has_only_editor_state_changes_since(before), local);
    let _ = edit.estimated_bytes();
    assert_eq!(
        edit.candidate_content_bytes.get().copied(),
        Some(after.estimated_content_bytes())
    );
    let changed = !before.clone().same_content(&after.clone());
    let full = if changed {
        summarize_impl(
            before,
            after,
            &crate::session::keys_in(before),
            &crate::session::keys_in(after),
            &edit.object_ids,
            &edit.categories,
            false,
        )
    } else {
        ChangeSet::default()
    };
    assert_eq!(before.same_content(after), !changed);
    #[cfg(feature = "bench-instrumentation")]
    let capture = zipzi_core::benchmark::Capture::start();
    let receipt = edit.commit_to(sdk).unwrap();
    #[cfg(feature = "bench-instrumentation")]
    if local {
        let work = capture.finish();
        assert!(!work.contains_key("core.authoring_identity_scan"));
        assert!(!work.contains_key("core.estimate_content_size"));
        assert!(!work.contains_key("sdk.summarize.fallback"));
    }
    assert_eq!(receipt.changes, full);
    assert_eq!(receipt.changed, changed);
    assert_eq!(
        sdk.estimated_content_bytes(),
        sdk.document().estimated_content_bytes()
    );
    for entry in sdk.done.iter().chain(&sdk.redo) {
        assert_eq!(
            entry.estimated_bytes(),
            entry.checkpoint.estimated_bytes() + entry.metadata_bytes()
        );
    }
}

#[test]
fn sparse_flags_match_full_reports_and_exact_memory_estimates() {
    let mut sdk = session();
    for n in [2, 3, 4, 5] {
        let mut edit = sdk.begin_owned_edit("Flags", None).unwrap();
        let mut state = edit.candidate_document().editor_state().clone();
        state.objects.insert(
            id(n),
            ObjectEditorState {
                locked: true,
                hidden_in_editor: true,
            },
        );
        edit.replace_editor_state(state).unwrap();
        check(&mut sdk, edit, true);
    }
    let saved = sdk.document().editor_state().clone();
    let history = sdk.history_lengths();
    let mut edit = sdk
        .begin_owned_edit("Restore flags within candidate", None)
        .unwrap();
    let mut state = saved.clone();
    state.objects.clear();
    edit.replace_editor_state(state).unwrap();
    edit.replace_editor_state(saved).unwrap();
    check(&mut sdk, edit, true);
    assert_eq!(sdk.history_lengths(), history);
    sdk.undo().unwrap();
    sdk.redo().unwrap();
    assert!(sdk.document().editor_state().get(&id(5)).locked);
}

#[test]
fn mixed_writes_in_either_order_drop_editor_only_coverage_and_match_full_reports() {
    for flags_first in [true, false] {
        let mut sdk = session();
        let mut edit = sdk.begin_owned_edit("Mixed", None).unwrap();
        let mut state = edit.candidate_document().editor_state().clone();
        state.objects.insert(
            id(3),
            ObjectEditorState {
                locked: true,
                ..Default::default()
            },
        );
        if flags_first {
            edit.replace_editor_state(state.clone()).unwrap();
        }
        edit.rename_mesh(&id(2), "Changed").unwrap();
        if !flags_first {
            edit.replace_editor_state(state).unwrap();
        }
        check(&mut sdk, edit, false);
    }
}

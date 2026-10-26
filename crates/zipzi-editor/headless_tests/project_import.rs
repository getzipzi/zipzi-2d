use super::setup;
use crate::{project::ImportKind, settings::Settings};
use gpui_kit::{AppContext as _, TestAppContext};

#[gpui_kit::test]
fn psd_picker_opens_an_unsaved_document_and_save_asks_for_a_path(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/zipzi-psd/tests/fixtures/layered.psd");
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.import(ImportKind::Psd, window, cx));
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && !options.directories && !options.multiple);
        Some(vec![source])
    });
    cx.run_until_parked();
    assert!(!cx.did_prompt_for_new_path());
    let imported = harness.view.read_with(cx, |view, cx| {
        assert!(!view.modal);
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert_eq!(view.editor.documents().len(), 2);
        let document = view.editor.document().unwrap();
        assert!(document.path().is_none());
        assert!(document.modified());
        assert_eq!(document.import_name(), Some("layered"));
        assert_eq!(document.layers().len(), 1);
        assert!(cx.global::<Settings>().recent_files.is_empty());
        view.editor.active_id()
    });
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.save(false, window, cx));
    })
    .unwrap();
    assert!(cx.did_prompt_for_new_path());
    let destination = harness._config_dir.path().join("layers.zipzi");
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.run_until_parked();
    harness.view.read_with(cx, |view, cx| {
        assert_eq!(view.editor.active_id(), imported);
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert!(!view.editor.document().unwrap().modified());
        assert_eq!(
            cx.global::<Settings>().recent_files.paths,
            vec![destination.clone()]
        );
    });
    assert!(destination.join("project.zipzi.json").exists());
}

#[gpui_kit::test]
fn failed_or_cancelled_psd_picker_preserves_the_active_document(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let original = harness
        .view
        .read_with(cx, |view, _| view.editor.active_id());
    for source in [None, Some(harness._config_dir.path().join("bad.psd"))] {
        if let Some(path) = &source {
            std::fs::write(path, b"bad").unwrap();
        }
        cx.update_window(harness.window, |_, window, cx| {
            harness
                .view
                .update(cx, |view, cx| view.import(ImportKind::Psd, window, cx));
        })
        .unwrap();
        cx.simulate_path_prompt_response(|_| source.clone().map(|path| vec![path]));
        cx.run_until_parked();
        assert!(!cx.did_prompt_for_new_path());
        harness.view.read_with(cx, |view, _| {
            assert!(!view.modal);
            assert_eq!(view.editor.active_id(), original);
            assert_eq!(view.editor.documents().len(), 1);
            assert_eq!(view.edit_error.is_some(), source.is_some());
        });
    }
}

#[gpui_kit::test]
fn save_as_replaces_existing_project_after_a_failed_save(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let destination = harness._config_dir.path().join("existing.zipzi");
    let mut existing = zipzi_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::Canvas::new(42., 42., Default::default(), 1.),
    )
    .unwrap();
    existing.save_project(&destination, None).unwrap();
    let manifest = destination.join("project.zipzi.json");
    let before = std::fs::read(&manifest).unwrap();
    let document = harness
        .view
        .read_with(cx, |view, _| view.editor.active_id().unwrap());
    // A save without a replacement confirmation must continue to reject it.
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.save_to(document, destination.clone(), false, window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(view.edit_error.is_some());
        assert!(!view.editor.is_busy());
    });
    assert_eq!(std::fs::read(&manifest).unwrap(), before);

    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.save(true, window, cx));
    })
    .unwrap();
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.run_until_parked();
    harness.view.read_with(cx, |view, cx| {
        assert!(!view.modal);
        assert!(!view.editor.is_busy());
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert!(!view.editor.document().unwrap().modified());
        assert_eq!(view.editor.active_id(), Some(document));
        assert_eq!(
            cx.global::<Settings>().recent_files.paths,
            vec![destination]
        );
    });
    assert_ne!(std::fs::read(&manifest).unwrap(), before);
    assert!(
        existing
            .open_project(&manifest, None)
            .unwrap()
            .resources_complete()
    );
    assert_eq!(existing.mesh_ids().len(), 2);
}

use super::setup;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

#[gpui_kit::test]
fn visibility_eye_updates_its_row_without_changing_selection(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (selected_id, second_id) = harness.view.read_with(cx, |view, _| {
        (
            view.editor.selected_layer().unwrap().id,
            view.editor
                .layers()
                .into_iter()
                .find(|layer| layer.name == "Second")
                .unwrap()
                .id,
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("mesh-vis-{second_id}"), cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.selected_layer().unwrap().id, selected_id);
            assert!(view.editor.selected_layer().unwrap().visible);
            assert!(
                !view
                    .editor
                    .layers()
                    .into_iter()
                    .find(|layer| layer.id == second_id)
                    .unwrap()
                    .visible
            );
        });
        harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            let panel = gpui_kit::component::dock::PanelId::from(dock.deformer_tree.entity_id());
            dock.area
                .update(cx, |area, cx| area.select_panel(panel, window, cx));
        });
        window.render_frame(cx);
        window.click(format!("deformer-vis-{second_id}"), cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.selected_layer().unwrap().id, selected_id);
            assert!(
                view.editor
                    .layers()
                    .iter()
                    .find(|layer| layer.id == second_id)
                    .unwrap()
                    .visible
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn default_layout_has_independent_document_panels(cx: &mut TestAppContext) {
    let harness = setup(cx);
    harness.view.read_with(cx, |view, cx| {
        let dock = view.dock.as_ref().unwrap();
        let layout = dock.area.read(cx).dump(cx);
        assert_eq!(layout.version, Some(3));
        let columns = &layout.center.children;
        assert_eq!(columns.len(), 3);
        assert_eq!(
            columns[0].children[0]
                .children
                .iter()
                .map(|p| p.panel_name.as_str())
                .collect::<Vec<_>>(),
            ["MeshTree", "DrawOrder"]
        );
        assert_eq!(columns[0].children[1].children[0].panel_name, "Deformers");
        assert_eq!(columns[1].children[0].panel_name, "Canvas");
        assert_eq!(
            columns[2].children[0]
                .children
                .iter()
                .map(|p| p.panel_name.as_str())
                .collect::<Vec<_>>(),
            ["Properties", "Relations"]
        );
        assert_eq!(columns[2].children[1].children[0].panel_name, "Parameters");
        let keyforms = gpui_kit::component::dock::PanelId::from(
            dock.document_panels[&crate::panels::DocumentPanelKind::Keyforms].entity_id(),
        );
        assert!(dock.area.read(cx).panel(keyforms).is_none());
        assert_eq!(dock.document_panels.len(), 10);
    });
}

#[gpui_kit::test]
fn reset_layout_saves_measured_center_size(cx: &mut TestAppContext) {
    use crate::docking::LayoutConfig;
    use editor_core::ConfigStore;
    use gpui_kit::component::dock::PanelInfo;

    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.reset_layout(window, cx));
        gpui_kit::test::TestWindowExt::render_frame(window, cx);
        assert!(window.simulate_next_frame(cx) > 0);
    })
    .unwrap();
    let saved = ConfigStore::new(harness._config_dir.path())
        .read::<LayoutConfig>("layout.json")
        .unwrap()
        .unwrap()
        .layout;
    let PanelInfo::Stack { sizes, .. } = &saved.center.info else {
        panic!("default center should be a split");
    };
    assert_eq!(saved.center.info.axis(), Some(gpui_kit::Axis::Horizontal));
    assert_eq!(sizes.len(), 3);
    assert!(sizes.iter().all(|size| size.as_f32() > 0.));
    assert!(sizes[1].as_f32() > sizes[0].as_f32());
    assert!(sizes[1].as_f32() > sizes[2].as_f32());
}

#[gpui_kit::test]
fn mesh_thumbnails_share_images_and_reuse_them_after_selection_and_edits(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.run_until_parked();
    let (cache, id, original) = harness.view.read_with(cx, |view, cx| {
        let id = view.editor.selected_layer().unwrap().id;
        let cache = view.mesh_thumbnails.clone();
        let image = cache
            .read(cx)
            .cached_image(&id)
            .expect("visible row thumbnail completed");
        (cache, id, image)
    });
    cx.update_window(harness.window, |_, window, cx| {
        // Same image is shared by both trees and repeated visible-range requests.
        cache.update(cx, |cache, cx| {
            let images = cache.request(&harness.view, &[&id], window, cx);
            assert!(std::sync::Arc::ptr_eq(
                images[0].as_ref().unwrap(),
                &original
            ));
            assert!(!cache.cache_state().1);
        });
        harness.view.update(cx, |view, cx| {
            view.editor
                .execute(editor_core::EditorCommand::SetLayerVisible {
                    layer_id: id.clone(),
                    visible: false,
                })
                .unwrap();
            view.editor
                .execute(editor_core::EditorCommand::Select(id.clone()))
                .unwrap();
            cx.notify();
        });
        window.render_frame(cx);
        cache.update(cx, |cache, cx| {
            let images = cache.request(&harness.view, &[&id], window, cx);
            assert!(std::sync::Arc::ptr_eq(
                images[0].as_ref().unwrap(),
                &original
            ));
            assert!(
                !cache.cache_state().1,
                "appearance edits must not rasterize again"
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn mesh_thumbnail_cache_is_bounded_and_releases_evicted_atlas_entries(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.run_until_parked();
    let (cache, id) = harness.view.read_with(cx, |view, _| {
        (
            view.mesh_thumbnails.clone(),
            view.editor.selected_layer().unwrap().id,
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let image = cache.read(cx).cached_image(&id).unwrap();
        assert!(window.has_image_atlas_entry(&image));
        for i in 0..600 {
            cache.update(cx, |cache, cx| {
                cache.request(&harness.view, &[&format!("untextured-{i}")], window, cx);
            });
        }
        assert_eq!(cache.read(cx).cache_state(), (512, false));
        assert!(cache.read(cx).cached_image(&id).is_none());
        assert!(
            !window.has_image_atlas_entry(&image),
            "eviction must free the GPU atlas entry too"
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn switching_documents_cancels_pending_mesh_thumbnails(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let cache = harness
        .view
        .read_with(cx, |view, _| view.mesh_thumbnails.clone());
    cx.update_window(harness.window, |_, window, cx| {
        assert!(
            cache.read(cx).cache_state().1,
            "first batch is still pending"
        );
        harness.view.update(cx, |view, cx| {
            view.editor.new_document().unwrap();
            // Clear even when neither tree renders (e.g. both panels are hidden).
            view.refresh_document(window, cx);
        });
        assert_eq!(cache.read(cx).cache_state(), (0, false));
    })
    .unwrap();
    cx.run_until_parked();
    cache.read_with(cx, |cache, _| assert_eq!(cache.cache_state(), (0, false)));
}

#[gpui_kit::test]
fn mesh_thumbnails_only_generate_for_virtualized_rows(cx: &mut TestAppContext) {
    let (document, assets) = super::document_fixture::sample_document().unwrap();
    let mut session =
        kasane_sdk::AuthoringSession::new(&uuid::Uuid::new_v4().to_string(), document.canvas())
            .unwrap();
    session
        .open_project(document.path().unwrap(), None)
        .unwrap();
    let template = session.mesh(document.selected_id().unwrap()).unwrap();
    session
        .edit("Many meshes", None, |edit| {
            for i in 0..200 {
                let mut mesh = template.clone();
                mesh.id = uuid::Uuid::new_v4().to_string();
                mesh.runtime_id = mesh.id.clone();
                mesh.name = format!("Mesh {i}");
                edit.create_mesh(mesh)?;
            }
            Ok(())
        })
        .unwrap();
    let saved = session
        .save_project(document.path().unwrap(), None)
        .unwrap();
    let harness = super::setup_with_document(
        cx,
        editor_core::Document::open(&saved.manifest).unwrap(),
        assets,
    );
    cx.run_until_parked();
    harness.view.read_with(cx, |view, cx| {
        let (entries, _) = view.mesh_thumbnails.read(cx).cache_state();
        assert!(
            entries > 0 && entries < 60,
            "only visible rows should enter the cache, got {entries} out of 201 meshes"
        );
    });
}

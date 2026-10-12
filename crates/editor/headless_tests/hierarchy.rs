use super::setup;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

#[gpui_kit::test]
fn tree_search_reveals_collapsed_matches_and_clear_restores_expansion(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let (diamond, second, selection, revision) = harness.view.read_with(cx, |view, _| {
        let layers = view.editor.layers();
        (
            layers
                .iter()
                .find(|layer| layer.name == "Diamond")
                .unwrap()
                .id
                .clone(),
            layers
                .iter()
                .find(|layer| layer.name == "Second")
                .unwrap()
                .id
                .clone(),
            view.editor.document().unwrap().context().selections.clone(),
            view.editor.tree_revision(),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        window
            .within("mesh-tree-panel")
            .click("tree-expand-Group:unowned", cx);
        assert!(
            window
                .try_find(format!("mesh-item-Mesh:{second}"))
                .is_none()
        );
        window.click("mesh-tree-search", cx);
        window.input("sEcOnD", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        assert!(window.find(format!("mesh-item-Mesh:{second}")).visible());
        assert!(
            window
                .try_find(format!("mesh-item-Mesh:{diamond}"))
                .is_none()
        );
        assert!(window.find("mesh-tree-search").focused().unwrap());
        harness.view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor.document().unwrap().context().selections,
                selection
            );
            assert_eq!(view.editor.tree_revision(), revision);
        });
        window.click("mesh-tree-clear", cx);
        assert_eq!(window.find("mesh-tree-search").value(), Some(""));
        assert!(
            window
                .try_find(format!("mesh-item-Mesh:{second}"))
                .is_none()
        );
        window
            .within("mesh-tree-panel")
            .click("tree-expand-Group:unowned", cx);
        assert!(window.find(format!("mesh-item-Mesh:{diamond}")).visible());
        assert!(window.find(format!("mesh-item-Mesh:{second}")).visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn tree_type_filter_keeps_popover_open_and_empty_results_keep_selection(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let selection = harness.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().context().selections.clone()
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-tree-search", cx);
        window.input("second", cx);
        window.click("mesh-tree-filter", cx);
        assert_eq!(window.find("tree-kind-Mesh").checked(), Some(true));
        window.click("tree-kind-Mesh", cx);
        assert_eq!(window.find("tree-kind-Mesh").checked(), Some(false));
        assert!(window.find("mesh-tree-empty").visible());
        // Continuous multi-select updates in place instead of closing the popup.
        window.click("tree-kind-Part", cx);
        assert_eq!(window.find("tree-kind-Part").checked(), Some(false));
        window.click("tree-all-types", cx);
        assert_eq!(window.find("tree-kind-Mesh").checked(), Some(true));
        assert_eq!(window.find("tree-kind-Part").checked(), Some(true));
        assert!(window.try_find("mesh-tree-empty").is_none());
        assert_eq!(window.find("mesh-tree-search").value(), Some("second"));
        window.press("escape", cx);
        assert!(window.try_find("tree-kind-Mesh").is_none());
        harness.view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor.document().unwrap().context().selections,
                selection
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn tree_conditions_are_independent_between_panels_and_restored_per_document(
    cx: &mut TestAppContext,
) {
    let harness = setup(cx);
    let original = harness
        .view
        .read_with(cx, |view, _| view.editor.active_id().unwrap());
    cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-tree-search", cx);
        window.input("second", cx);
        window.click("mesh-tree-filter", cx);
        window.click("tree-kind-Part", cx);
        window.press("escape", cx);
        harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            let panel = gpui_kit::component::dock::PanelId::from(dock.deformer_tree.entity_id());
            dock.area
                .update(cx, |area, cx| area.select_panel(panel, window, cx));
        });
        window.render_frame(cx);
        assert_eq!(window.find("deformer-tree-search").value(), Some(""));
        window.click("deformer-tree-search", cx);
        window.input("diamond", cx);
        assert_eq!(window.find("mesh-tree-search").value(), Some("second"));
        window.click("mesh-tree-filter", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor.new_document().unwrap();
            cx.notify();
        });
        window.render_frame(cx);
        assert_eq!(window.find("mesh-tree-search").value(), Some(""));
        assert_eq!(window.find("deformer-tree-search").value(), Some(""));
        assert!(window.try_find("tree-kind-Mesh").is_none());
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor.activate(original).unwrap();
            cx.notify();
        });
        window.render_frame(cx);
        assert_eq!(window.find("mesh-tree-search").value(), Some("second"));
        assert_eq!(window.find("deformer-tree-search").value(), Some("diamond"));
        window.click("mesh-tree-filter", cx);
        assert_eq!(window.find("tree-kind-Part").checked(), Some(false));
        assert_eq!(window.find("tree-kind-Mesh").checked(), Some(true));
        window.press("escape", cx);
        window.click("mesh-tree-clear", cx);
        assert_eq!(window.find("deformer-tree-search").value(), Some("diamond"));
        window.click("mesh-tree-filter", cx);
        assert_eq!(window.find("tree-kind-Part").checked(), Some(true));
    })
    .unwrap();
}

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
fn visibility_eyes_toggle_all_scene_object_kinds_without_changing_selection(
    cx: &mut TestAppContext,
) {
    use editor_core::{ObjectKind, ObjectRef};
    let (doc, assets) = super::document_fixture::sample_document_with_scene_objects().unwrap();
    let mesh = doc.layers()[0].id.clone();
    let selection = doc.context().selections.clone();
    let harness = super::setup_with_document(cx, doc, assets);
    for target in [
        ObjectRef::new(ObjectKind::Part, uuid::Uuid::from_u128(101).to_string()),
        ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(501).to_string()),
        ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(502).to_string()),
        ObjectRef::new(
            ObjectKind::Offscreen,
            uuid::Uuid::from_u128(601).to_string(),
        ),
        ObjectRef::new(ObjectKind::Mesh, mesh),
    ] {
        cx.update_window(harness.window, |_, window, cx| {
            for expected in [false, true] {
                window.click(format!("mesh-vis-{}", target.id), cx);
                harness.view.read_with(cx, |view, _| {
                    assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
                    let doc = view.editor.document().unwrap();
                    assert_eq!(doc.object_state(&target).visible(), expected);
                    assert_eq!(doc.object_index().get(&target).unwrap().enabled, Some(true));
                    assert_eq!(doc.context().selections, selection);
                });
            }
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn deformer_tree_controls_do_not_hide_or_lock_children_and_sync_with_part_tree(
    cx: &mut TestAppContext,
) {
    use editor_core::{EditorCommand, ObjectKind, ObjectRef};
    let (doc, assets) = super::document_fixture::sample_document_with_scene_objects().unwrap();
    let parent = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(501).to_string());
    let child = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(502).to_string());
    let mesh = ObjectRef::new(ObjectKind::Mesh, doc.layers()[1].id.clone());
    let harness = super::setup_with_document(cx, doc, assets);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            let panel = gpui_kit::component::dock::PanelId::from(dock.deformer_tree.entity_id());
            dock.area
                .update(cx, |area, cx| area.select_panel(panel, window, cx));
        });
        window.render_frame(cx);
        window.hover(format!("deformer-item-RotationDeformer:{}", parent.id), cx);
        window.click(format!("deformer-lock-{}", parent.id), cx);
        window.click(format!("deformer-vis-{}", parent.id), cx);
        harness.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            assert!(doc.object_state_index().state(&parent).explicit.locked);
            assert!(!doc.deformer_controls_visible(&parent.id));
            assert!(doc.deformer_controls_visible(&child.id));
            assert!(!doc.object_state_index().is_locked(&child));
            assert!(!doc.object_state_index().is_locked(&mesh));
            assert!(doc.layers().iter().all(|layer| layer.visible));
        });
        window.hover(format!("deformer-item-WarpDeformer:{}", child.id), cx);
        window.click(format!("deformer-lock-{}", child.id), cx);
        harness.view.read_with(cx, |view, _| {
            assert!(
                view.editor
                    .document()
                    .unwrap()
                    .object_state_index()
                    .state(&child)
                    .explicit
                    .locked
            );
        });
        harness.view.update(cx, |view, cx| {
            view.editor.execute(EditorCommand::Undo).unwrap();
            let dock = view.dock.as_ref().unwrap();
            let panel = gpui_kit::component::dock::PanelId::from(dock.mesh_tree.entity_id());
            dock.area
                .update(cx, |area, cx| area.select_panel(panel, window, cx));
            cx.notify();
        });
        window.render_frame(cx);
        window.click(format!("mesh-vis-{}", parent.id), cx);
        window.click(format!("mesh-lock-{}", parent.id), cx);
        harness.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            assert!(view.edit_error.is_none());
            assert!(doc.deformer_controls_visible(&parent.id));
            assert!(!doc.object_state_index().is_locked(&parent));
            assert!(!doc.object_state_index().is_locked(&child));
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
        .read::<LayoutConfig>("layout-modeling.json")
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
                .execute(editor_core::EditorCommand::SetObjectVisible {
                    object: editor_core::ObjectRef::new(editor_core::ObjectKind::Mesh, id.clone()),
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

#[gpui_kit::test]
fn tree_lock_button_preserves_selection_and_other_tree_can_unlock(cx: &mut TestAppContext) {
    use editor_core::{EditorCommand, ObjectKind, ObjectRef};
    let harness = setup(cx);
    let (target, original) = harness.view.read_with(cx, |view, _| {
        (
            ObjectRef::new(
                ObjectKind::Mesh,
                view.editor
                    .layers()
                    .iter()
                    .find(|l| l.name == "Second")
                    .unwrap()
                    .id
                    .clone(),
            ),
            view.editor.document().unwrap().context().selections.clone(),
        )
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.hover(format!("mesh-item-Mesh:{}", target.id), cx);
        window.click(format!("mesh-lock-{}", target.id), cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert!(doc.object_state_index().state(&target).explicit.locked);
        assert_eq!(doc.context().selections, original);
        assert_eq!(doc.history_lengths(), (1, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            let panel = gpui_kit::component::dock::PanelId::from(dock.deformer_tree.entity_id());
            dock.area
                .update(cx, |area, cx| area.select_panel(panel, window, cx));
        });
        window.render_frame(cx);
        window.click(format!("deformer-lock-{}", target.id), cx);
    })
    .unwrap();
    harness.view.update(cx, |view, cx| {
        assert!(
            !view
                .editor
                .document()
                .unwrap()
                .object_state_index()
                .is_locked(&target)
        );
        view.editor.execute(EditorCommand::Undo).unwrap();
        assert!(
            view.editor
                .document()
                .unwrap()
                .object_state_index()
                .is_locked(&target)
        );
        cx.notify();
    });
}

#[gpui_kit::test]
fn inherited_lock_button_opens_options_without_selecting_or_unlocking_parent(
    cx: &mut TestAppContext,
) {
    use editor_core::{ObjectKind, ObjectRef};
    let (mut doc, assets) = super::document_fixture::sample_document_with_parts().unwrap();
    let parent = ObjectRef::new(ObjectKind::Part, uuid::Uuid::from_u128(101).to_string());
    let mesh = doc.layers()[0].id.clone();
    doc.set_objects_locked(&[parent.clone()], true).unwrap();
    let harness = super::setup_with_document(cx, doc, assets);
    let selection = harness.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().context().selections.clone()
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("mesh-lock-{mesh}"), cx);
        assert!(
            window
                .find(format!("locate-state-source-{}", parent.id))
                .visible()
        );
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.context().selections, selection);
        assert!(doc.object_state_index().state(&parent).explicit.locked);
        assert!(
            !doc.object_state_index()
                .state(&ObjectRef::new(ObjectKind::Mesh, mesh))
                .explicit
                .locked
        );
        assert_eq!(doc.history_lengths(), (1, 0));
    });
}

#[gpui_kit::test]
fn inherited_eye_explains_source_and_only_changes_it_on_explicit_choice(cx: &mut TestAppContext) {
    use editor_core::{ObjectKind, ObjectRef};
    let (mut doc, assets) = super::document_fixture::sample_document_with_parts().unwrap();
    let part = ObjectRef::new(ObjectKind::Part, uuid::Uuid::from_u128(101).to_string());
    let mesh = ObjectRef::new(ObjectKind::Mesh, doc.layers()[0].id.clone());
    doc.set_object_visible(&part, false).unwrap();
    doc.select(&doc.layers()[1].id.clone()).unwrap();
    let selection = doc.context().selections.clone();
    let harness = super::setup_with_document(cx, doc, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("mesh-vis-{}", mesh.id), cx);
        assert!(
            window
                .find(format!("locate-state-source-{}", part.id))
                .visible()
        );
        harness.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            assert_eq!(doc.context().selections, selection);
            assert_eq!(doc.history_lengths(), (1, 0));
            assert!(!doc.object_state(&mesh).visible());
            assert!(!doc.object_state(&mesh).explicit.hidden_in_editor);
        });
        window.click(format!("change-state-source-{}", part.id), cx);
        harness.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            assert_eq!(doc.context().selections, selection);
            assert!(doc.object_state(&mesh).visible());
            assert_eq!(doc.history_lengths(), (2, 0));
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn model_enabled_property_is_read_only_and_independent_of_the_editor_eye(cx: &mut TestAppContext) {
    use editor_core::{ObjectKind, ObjectRef};
    for enabled in [true, false] {
        let (mut doc, assets) = super::document_fixture::sample_document_with_parts().unwrap();
        let mesh = ObjectRef::new(ObjectKind::Mesh, doc.layers()[0].id.clone());
        doc.select(&mesh.id).unwrap();
        if !enabled {
            doc.set_object_property(&mesh, "enabled", enabled.into())
                .unwrap();
        }
        let history = doc.history_lengths();
        let harness = super::setup_with_document(cx, doc, assets);
        cx.update_window(harness.window, |_, window, cx| {
            assert!(window.try_find("object-model-enabled").is_none());
            assert_eq!(window.find("object-property-enabled").checked(), None);
            harness.view.read_with(cx, |view, _| {
                let doc = view.editor.document().unwrap();
                assert_eq!(
                    doc.object_index().get(&mesh).unwrap().enabled,
                    Some(enabled)
                );
                assert_eq!(doc.history_lengths(), history);
            });

            for (edit, visible) in [(1, false), (2, true)] {
                window.click(format!("mesh-vis-{}", mesh.id), cx);
                assert!(window.try_find("object-model-enabled").is_none());
                assert_eq!(window.find("object-property-enabled").checked(), None);
                harness.view.read_with(cx, |view, _| {
                    let doc = view.editor.document().unwrap();
                    assert_eq!(
                        doc.object_index().get(&mesh).unwrap().enabled,
                        Some(enabled)
                    );
                    assert_eq!(doc.object_state(&mesh).visible(), visible);
                    assert_eq!(doc.context().inspected.as_ref(), Some(&mesh));
                    assert_eq!(doc.history_lengths(), (history.0 + edit, 0));
                });
            }
        })
        .unwrap();
    }
}

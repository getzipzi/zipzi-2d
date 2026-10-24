use super::{
    Harness, document_fixture,
    properties::{fill, show},
    setup_with_document,
};
use crate::authoring::{AuthoringContext, texture::Control};
use gpui_kit::{AppContext as _, TestAppContext, px, size, test::TestWindowExt};
use zipzi_editor_core::{EditorCommand, ObjectKind as K, ObjectRef as R, PanelKind};

fn setup(cx: &mut TestAppContext) -> Harness {
    let (doc, assets) = document_fixture::sample_document_with_two_layers().unwrap();
    let h = setup_with_document(cx, doc, assets);
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(h.window, |_, window, cx| {
        window.resize(size(px(1200.), px(1000.)));
        h.view.update(cx, |v, cx| {
            v.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::Resources, None, window, cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    h
}
fn click(h: &Harness, id: impl Into<gpui_kit::ElementId>, cx: &mut TestAppContext) {
    let id = id.into();
    cx.update_window(h.window, |_, window, cx| window.click(id, cx))
        .unwrap();
    cx.run_until_parked();
}
fn selected(h: &Harness, cx: &mut TestAppContext) -> R {
    h.view.read_with(cx, |v, _| {
        v.editor.active_inspection().unwrap().object().unwrap()
    })
}
fn run(h: &Harness, asset: Option<String>, control: Control, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            let context = AuthoringContext::capture(&v.editor).unwrap();
            v.run_texture_control(context, asset, control, window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
}
fn inspect(h: &Harness, object: R, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |v, cx| {
            v.inspect_authoring_object(v.editor.active_id().unwrap(), object, false, window, cx)
                .unwrap();
            v.redraw(cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
}
fn png(h: &Harness) -> std::path::PathBuf {
    let path = h._assets.path().join("replacement-image.png");
    std::fs::write(
        &path,
        include_bytes!("../../zipzi-editor-core/tests/fixtures/accent.png"),
    )
    .unwrap();
    path
}
fn choose(h: &Harness, id: &str, query: &str, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        window.within(id.to_owned()).click("input", cx);
        window.input(query, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(160));
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
}
#[gpui_kit::test]
fn new_texture_inspects_immediately_and_cancelled_chooser_keeps_empty_resource(
    cx: &mut TestAppContext,
) {
    let h = setup(cx);
    click(&h, "texture-new", cx);
    let asset = selected(&h, cx);
    assert_eq!(asset.kind, K::Texture);
    show(&h, "texture-status", cx);
    click(&h, format!("texture-load-{}", asset.id), cx);
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && !options.directories && !options.multiple);
        None
    });
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(!v.modal);
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        assert_eq!(d.history_lengths(), (1, 0));
        let info = d.texture_info(&asset.id).unwrap();
        assert!(info.asset.content.is_none() && info.asset.unresolved_source.is_none());
        assert!(v.texture_loads.is_empty());
    });
    run(&h, Some(asset.id.clone()), Control::Delete, cx);
    click(&h, "texture-delete-submit", cx);
    h.view.update(cx, |v, _| {
        assert!(
            v.editor
                .document()
                .unwrap()
                .texture_info(&asset.id)
                .is_err()
        );
        assert_eq!(v.editor.document().unwrap().history_lengths(), (2, 0));
        v.editor.execute(EditorCommand::Undo).unwrap();
        assert!(
            v.editor
                .document()
                .unwrap()
                .texture_info(&asset.id)
                .unwrap()
                .asset
                .content
                .is_none()
        );
    });
}
#[gpui_kit::test]
fn direct_import_cancel_invalid_png_and_success_have_atomic_history(cx: &mut TestAppContext) {
    let h = setup(cx);
    let count = h.view.read_with(cx, |v, _| {
        v.editor
            .document()
            .unwrap()
            .resource_collection()
            .unwrap()
            .len()
    });
    click(&h, "texture-import", cx);
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    let invalid = h._assets.path().join("invalid.png");
    std::fs::write(&invalid, b"not a PNG").unwrap();
    run(&h, None, Control::Import, cx);
    cx.simulate_path_prompt_response(|_| Some(vec![invalid]));
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(v.edit_error.is_some());
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0));
        assert_eq!(
            v.editor
                .document()
                .unwrap()
                .resource_collection()
                .unwrap()
                .len(),
            count
        );
    });
    run(&h, None, Control::Import, cx);
    let path = png(&h);
    cx.simulate_path_prompt_response(|_| Some(vec![path]));
    cx.run_until_parked();
    let asset = selected(&h, cx);
    h.view.update(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        assert_eq!(d.history_lengths(), (1, 0));
        assert!(d.texture_info(&asset.id).unwrap().asset.content.is_some());
        v.editor.execute(EditorCommand::Undo).unwrap();
        assert_eq!(
            v.editor
                .document()
                .unwrap()
                .resource_collection()
                .unwrap()
                .len(),
            count
        );
        v.editor.execute(EditorCommand::Redo).unwrap();
        assert!(
            v.editor
                .document()
                .unwrap()
                .texture_info(&asset.id)
                .unwrap()
                .asset
                .content
                .is_some()
        );
    });
}
#[gpui_kit::test]
fn mesh_selects_existing_empty_texture_and_replacement_refreshes_the_same_resource(
    cx: &mut TestAppContext,
) {
    let h = setup(cx);
    let mesh = h.view.read_with(cx, |v, _| {
        R::new(K::Mesh, &v.editor.document().unwrap().layers()[0].id)
    });
    run(&h, None, Control::New, cx);
    let asset = selected(&h, cx);
    fill(&h, "property-input-name", "TextureSearchTarget", cx);
    cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    inspect(&h, mesh.clone(), cx);
    show(&h, "property-select-texture", cx);
    choose(&h, "property-select-texture", "TextureSearchTarget", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(
            v.editor
                .document()
                .unwrap()
                .texture_info(&asset.id)
                .unwrap()
                .users,
            [mesh.clone()]
        )
    });
    inspect(&h, asset.clone(), cx);
    click(&h, format!("texture-load-{}", asset.id), cx);
    let path = png(&h);
    cx.simulate_path_prompt_response(|_| Some(vec![path]));
    cx.run_until_parked();
    h.view.update(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        assert_eq!(d.texture_info(&asset.id).unwrap().users, [mesh.clone()]);
        assert!(
            d.texture_info(&asset.id).unwrap().asset.content.is_some(),
            "loaded content missing"
        );
        assert!(
            v.editor
                .preview()
                .unwrap()
                .scene
                .textures
                .iter()
                .any(|t| t.asset_id == asset.id)
        );
        assert!(
            v.editor
                .preview()
                .unwrap()
                .scene
                .runtime_frame
                .drawables
                .iter()
                .find(|m| m.id == mesh.id)
                .unwrap()
                .renderable
        );
    });
}
#[gpui_kit::test]
fn delete_reviews_real_users_requires_replacement_and_can_cancel(cx: &mut TestAppContext) {
    let h = setup(cx);
    let source = h.view.read_with(cx, |v, _| {
        v.editor.document().unwrap().resource_collection().unwrap()[0].clone()
    });
    run(&h, None, Control::New, cx);
    let replacement = selected(&h, cx);
    fill(&h, "property-input-name", "DeleteReplacement", cx);
    cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    let history = h
        .view
        .read_with(cx, |v, _| v.editor.document().unwrap().history_lengths());
    run(&h, Some(source.id.clone()), Control::Delete, cx);
    cx.update_window(h.window, |_, window, _| {
        let users = window.find("texture-delete-users").bounds();
        for i in 0..2usize {
            let row = window.find(("texture-delete-user", i)).bounds();
            assert!(row.top() >= users.top() && row.bottom() <= users.bottom());
        }
    })
    .unwrap();
    click(&h, "texture-delete-submit", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), history)
    });
    click(&h, "texture-delete-cancel", cx);
    run(&h, Some(source.id.clone()), Control::Delete, cx);
    choose(&h, "texture-replacement", "DeleteReplacement", cx);
    click(&h, "texture-delete-submit", cx);
    h.view.update(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let d = v.editor.document().unwrap();
        assert!(d.texture_info(&source.id).is_err());
        assert_eq!(d.texture_info(&replacement.id).unwrap().users.len(), 2);
        v.editor.execute(EditorCommand::Undo).unwrap();
        assert_eq!(
            v.editor
                .document()
                .unwrap()
                .texture_info(&source.id)
                .unwrap()
                .users
                .len(),
            2
        );
    });
}

#[gpui_kit::test]
fn dependency_review_virtualizes_large_lists_and_keeps_the_last_user_reachable(
    cx: &mut TestAppContext,
) {
    use zipzi_sdk::{AuthoringSession, prepare_png_asset, rectangle_mesh};
    const USERS: usize = 256;
    let assets = tempfile::tempdir().unwrap();
    let path = assets.path().join("texture.png");
    std::fs::write(
        &path,
        include_bytes!("../../zipzi-editor-core/tests/fixtures/accent.png"),
    )
    .unwrap();
    let asset_id = uuid::Uuid::new_v4().to_string();
    let asset = prepare_png_asset(&asset_id, "Shared", &path).unwrap();
    let mut session = AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::Canvas::default(),
    )
    .unwrap();
    session
        .edit("Many texture users", None, |edit| {
            edit.create_asset(asset)?;
            for i in 0..USERS {
                edit.create_mesh(rectangle_mesh(
                    &uuid::Uuid::new_v4().to_string(),
                    &format!("Mesh {i}"),
                    &asset_id,
                    zipzi_core::Vec2::new(0., 0.),
                    zipzi_core::Vec2::new(1., 1.),
                )?)?;
            }
            Ok(())
        })
        .unwrap();
    let saved = session
        .save_project(&assets.path().join("many-users.zipzi"), None)
        .unwrap();
    let document = zipzi_editor_core::Document::open(&saved.manifest).unwrap();
    let h = setup_with_document(cx, document, assets);
    run(&h, Some(asset_id), Control::Delete, cx);
    for height in [1000., 720.] {
        cx.update_window(h.window, |_, window, cx| {
            window.resize(size(px(1100.), px(height)));
            window.render_frame(cx);
            let count = (0..USERS)
                .filter(|i| window.try_find(("texture-delete-user", *i)).is_some())
                .count();
            assert!(count > 0 && count < 20, "laid out {count} of {USERS} users");
            window.scroll(
                "texture-delete-users",
                gpui_kit::ScrollDelta::Pixels(gpui_kit::point(px(0.), px(-100_000.))),
                cx,
            );
            window.render_frame(cx);
            let users = window.find("texture-delete-users").bounds();
            let last = window.find(("texture-delete-user", USERS - 1)).bounds();
            assert!(last.top() >= users.top() && last.bottom() <= users.bottom() + px(1.));
            let count = (0..USERS)
                .filter(|i| window.try_find(("texture-delete-user", *i)).is_some())
                .count();
            assert!(
                count > 0 && count < 20,
                "laid out {count} users after scrolling"
            );
        })
        .unwrap();
    }
    click(&h, "texture-delete-cancel", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0))
    });
}
#[gpui_kit::test]
fn image_result_after_document_switch_back_is_discarded(cx: &mut TestAppContext) {
    let h = setup(cx);
    run(&h, None, Control::New, cx);
    let asset = selected(&h, cx);
    run(&h, Some(asset.id.clone()), Control::Load, cx);
    h.view.update(cx, |v, _| {
        let document = v.editor.active_id().unwrap();
        v.editor.new_document().unwrap();
        v.editor.activate(document).unwrap();
    });
    let path = png(&h);
    cx.simulate_path_prompt_response(|_| Some(vec![path]));
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(
            v.editor
                .document()
                .unwrap()
                .texture_info(&asset.id)
                .unwrap()
                .asset
                .content
                .is_none()
        );
        assert_eq!(v.editor.document().unwrap().history_lengths(), (1, 0));
        assert!(v.texture_loads.is_empty());
    });
}
#[gpui_kit::test]
fn image_result_after_undo_does_not_publish(cx: &mut TestAppContext) {
    let h = setup(cx);
    run(&h, None, Control::New, cx);
    let asset = selected(&h, cx);
    run(&h, Some(asset.id.clone()), Control::Load, cx);
    h.view.update(cx, |v, _| {
        v.editor.execute(EditorCommand::Undo).unwrap();
        v.editor.execute(EditorCommand::Redo).unwrap();
    });
    let path = png(&h);
    cx.simulate_path_prompt_response(|_| Some(vec![path]));
    cx.run_until_parked();
    h.view.read_with(cx, |v, _| {
        assert!(
            v.editor
                .document()
                .unwrap()
                .texture_info(&asset.id)
                .unwrap()
                .asset
                .content
                .is_none()
        );
        assert_eq!(v.editor.document().unwrap().history_lengths(), (1, 0));
    });
}

#[gpui_kit::test]
fn missing_source_can_be_repaired_without_replacing_the_resource_or_meshes(
    cx: &mut TestAppContext,
) {
    let (mut doc, assets) = document_fixture::sample_document_with_two_layers().unwrap();
    let asset = doc.resource_collection().unwrap()[0].clone();
    let saved = doc
        .save(&assets.path().join("missing-source.zipzi"))
        .unwrap();
    let mut session = zipzi_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::ModelSetup::default(),
    )
    .unwrap();
    session.open_project(&saved.manifest, None).unwrap();
    let mut missing = session.asset(&asset.id).unwrap();
    missing.content = None;
    missing.unresolved_source = Some("lost-image.png".into());
    session
        .edit("Missing source fixture", None, |e| e.replace_asset(missing))
        .unwrap();
    let saved = session.save_project(&saved.manifest, None).unwrap();
    let mut doc = zipzi_editor_core::Document::open(&saved.manifest).unwrap();
    doc.navigate(asset.clone()).unwrap();
    let h = setup_with_document(cx, doc, assets);
    show(&h, "texture-status", cx);
    click(&h, format!("texture-load-{}", asset.id), cx);
    let path = png(&h);
    cx.simulate_path_prompt_response(|_| Some(vec![path]));
    cx.run_until_parked();
    h.view.update(cx, |v, _| {
        assert!(v.edit_error.is_none(), "{:?}", v.edit_error);
        let info = v
            .editor
            .document()
            .unwrap()
            .texture_info(&asset.id)
            .unwrap();
        assert!(info.asset.content.is_some() && info.asset.unresolved_source.is_none());
        assert_eq!(info.users.len(), 2);
        v.editor.execute(EditorCommand::Undo).unwrap();
        let info = v
            .editor
            .document()
            .unwrap()
            .texture_info(&asset.id)
            .unwrap();
        assert!(info.asset.content.is_none());
        assert_eq!(
            info.asset.unresolved_source.as_deref(),
            Some("lost-image.png")
        );
        v.editor.execute(EditorCommand::Redo).unwrap();
        let frame = v.editor.preview().unwrap();
        assert!(
            frame
                .scene
                .runtime_frame
                .drawables
                .iter()
                .all(|d| d.renderable)
        );
        assert!(frame.scene.textures.iter().any(|t| t.asset_id == asset.id));
    });
}

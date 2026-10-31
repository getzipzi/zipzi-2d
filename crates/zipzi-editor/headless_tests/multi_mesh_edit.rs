use super::{
    Harness,
    manual_mesh::{pointer_gesture, present},
    setup_with_document,
};
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};
use zipzi_editor_core::{Document, ModelPoint, ObjectKind, ObjectRef};
#[path = "../../../fixtures/multi-mesh-edit.rs"]
mod fixture;

fn enter(cx: &mut TestAppContext) -> Harness {
    let assets = tempfile::tempdir().unwrap();
    let mut session = fixture::create(
        assets.path(),
        include_bytes!("../../zipzi-editor-core/tests/fixtures/alpha-asymmetric.png"),
    )
    .unwrap();
    let saved = session
        .save_project(&assets.path().join("multi.zipzi"), None)
        .unwrap();
    let mut document = Document::open(&saved.manifest).unwrap();
    document
        .select_objects(
            [2, 4]
                .map(|n| ObjectRef::new(ObjectKind::Mesh, fixture::id(n)))
                .into(),
        )
        .unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.click("tool_manual_mesh", cx);
    })
    .unwrap();
    present(&h, cx);
    assert!(h.view.read_with(cx, |view, _| view.editor.manual_active()));
    h
}
fn meshes(h: &Harness, cx: &TestAppContext) -> Vec<zipzi_core::Mesh> {
    h.view.read_with(cx, |view, _| {
        let document = view.editor.document().unwrap();
        let host = document.runtime_host();
        let runtime = host.read();
        let model = runtime.document(document.runtime_session_id()).unwrap();
        [2, 4]
            .map(|n| model.get_mesh(&fixture::id(n)).unwrap().clone())
            .into()
    })
}
#[gpui_kit::test]
fn multi_mesh_native_selection_drag_undo_and_tool_exit(cx: &mut TestAppContext) {
    let h = enter(cx);
    let original = meshes(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        let label = window.find("mesh-edit-active-target");
        assert!(label.visible());
        assert_eq!(label.label(), Some("Mesh 2"));
        assert!(label.bounds().size.width > gpui_kit::px(8.));
        assert_eq!(h.view.read(cx).editor.manual_targets().len(), 2);
    })
    .unwrap();
    let a = ModelPoint { x: 20., y: 20. };
    let b = ModelPoint { x: 140., y: 140. };
    pointer_gesture(&h, cx, a, a, Default::default());
    pointer_gesture(
        &h,
        cx,
        b,
        b,
        gpui_kit::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    pointer_gesture(
        &h,
        cx,
        b,
        ModelPoint { x: 146., y: 144. },
        Default::default(),
    );
    present(&h, cx);
    let moved = meshes(&h, cx);
    cx.update_window(h.window, |_, window, _| {
        assert_eq!(
            window.find("mesh-edit-active-target").label(),
            Some("Mesh 4")
        );
    })
    .unwrap();
    for (before, after) in original.iter().zip(&moved) {
        assert!((after.base_positions[0].x - before.base_positions[0].x - 6.).abs() < 1e-3);
        assert!((after.base_positions[0].y - before.base_positions[0].y - 4.).abs() < 1e-3);
    }
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0))
    });
    cx.update_window(h.window, |_, window, cx| window.press("cmd-z", cx))
        .unwrap();
    present(&h, cx);
    assert_eq!(meshes(&h, cx), original);
    assert_eq!(
        h.view
            .read_with(cx, |view, _| view.editor.manual_targets().len()),
        2
    );
    cx.update_window(h.window, |_, window, cx| window.press("cmd-shift-z", cx))
        .unwrap();
    present(&h, cx);
    assert_eq!(meshes(&h, cx), moved);
    cx.update_window(h.window, |_, window, cx| window.click("tool_select", cx))
        .unwrap();
    assert!(!h.view.read_with(cx, |view, _| view.editor.manual_active()));
    assert_eq!(meshes(&h, cx), moved);
}

#[gpui_kit::test]
fn multi_mesh_native_padding_uses_current_target_and_source_view_is_disabled(
    cx: &mut TestAppContext,
) {
    let h = enter(cx);
    let originals = h.view.read_with(cx, |view, _| view.editor.manual_targets());
    let point = ModelPoint { x: 140., y: 140. };
    pointer_gesture(&h, cx, point, point, Default::default());
    present(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        assert_eq!(
            h.view.read(cx).editor.manual_target().unwrap().object.id,
            fixture::id(4)
        );
        assert!(
            !h.view.read(cx).editor.command_available(
                h.view
                    .read(cx)
                    .editor
                    .command(zipzi_editor_core::CommandId::ManualSourceView)
            )
        );
        window.click("tool_manual_pad_source", cx);
    })
    .unwrap();
    present(&h, cx);
    h.view.read_with(cx, |view, _| {
        for (before, after) in originals.iter().zip(view.editor.manual_targets()) {
            assert_eq!(
                after.texture.width,
                before.texture.width
                    + if after.object.id == fixture::id(4) {
                        8
                    } else {
                        0
                    }
            );
        }
        assert!(view.edit_error.is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
    cx.update_window(h.window, |_, window, cx| window.press("cmd-z", cx))
        .unwrap();
    present(&h, cx);
    h.view.read_with(cx, |view, _| {
        for (before, after) in originals.iter().zip(view.editor.manual_targets()) {
            assert_eq!(after.texture.rgba, before.texture.rgba);
        }
    });
}

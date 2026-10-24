use super::{document_fixture, setup_with_document};
use gpui_kit::component::{
    Theme,
    scroll::{ScrollbarHandle, ScrollbarMode},
};
use gpui_kit::{AppContext as _, TestAppContext, point, px, test::TestWindowExt};

#[gpui_kit::test]
fn parts_tree_scrollbar_drag_reaches_virtualized_rows_without_selecting_them(
    cx: &mut TestAppContext,
) {
    let mut document = zipzi_editor_core::Document::new_blank().unwrap();
    let mut objects = Vec::new();
    for i in 0..60 {
        let plan = document
            .plan_scene_object_creation(
                zipzi_editor_core::SceneObjectType::Part,
                format!("Part {i}"),
                &[],
            )
            .unwrap();
        objects.push(
            document
                .execute_scene_object_creation(&plan)
                .unwrap()
                .suggested_selection[0]
                .clone(),
        );
    }
    let last = document.part_tree().last().unwrap().id.clone();
    let selection = document.context().selections.clone();
    let history = document.history_lengths();
    let h = setup_with_document(cx, document, tempfile::tempdir().unwrap());
    let scroll = h.view.read_with(cx, |v, cx| {
        v.dock
            .as_ref()
            .unwrap()
            .part_tree
            .read(cx)
            .sorting_scroll_handle()
    });
    cx.update_window(h.window, |_, window, cx| {
        cx.set_reduce_motion(true);
        Theme::set_scrollbar_mode(ScrollbarMode::Always, cx);
        window.refresh();
        window.render_frame(cx);
        scroll.set_offset(point(px(0.), px(0.)));
        window.render_frame(cx);
        let bounds = window.find("parts-tree-scroll-frame").bounds();
        assert_eq!(
            bounds.size,
            scroll.viewport_bounds().size,
            "frame vs list viewport"
        );
        assert!(
            window
                .try_find(format!("parts-item-Part:{}", last))
                .is_none()
        );
        window.drag(
            point(bounds.right() - px(8.), bounds.top() + px(12.)),
            point(bounds.right() - px(8.), bounds.bottom() - px(4.)),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find(format!("parts-item-Part:{}", last))
                .is_some_and(|row| row.visible()),
            "offset {:?}, content {:?}, viewport {:?}",
            scroll.offset(),
            scroll.content_size(),
            scroll.viewport_bounds()
        );
        h.view.read_with(cx, |v, _| {
            let d = v.editor.document().unwrap();
            assert_eq!(d.context().selections, selection);
            assert_eq!(d.history_lengths(), history);
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn parameter_scrollbar_drag_uses_the_existing_list_handle(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(60).unwrap();
    let h = setup_with_document(cx, document, assets);
    let scroll = h.view.read_with(cx, |v, cx| {
        v.dock
            .as_ref()
            .unwrap()
            .parameters
            .read(cx)
            .sorting_scroll_handle()
    });
    cx.update_window(h.window, |_, window, cx| {
        cx.set_reduce_motion(true);
        Theme::set_scrollbar_mode(ScrollbarMode::Always, cx);
        window.refresh();
        window.render_frame(cx);
        scroll.set_offset(point(px(0.), px(0.)));
        window.render_frame(cx);
        let bounds = window.find("parameters-scroll-frame").bounds();
        assert_eq!(
            bounds.size,
            scroll.viewport_bounds().size,
            "frame vs list viewport"
        );
        let before = scroll.offset().y;
        window.drag(
            point(bounds.right() - px(8.), bounds.top() + px(12.)),
            point(bounds.right() - px(8.), bounds.bottom() - px(4.)),
            cx,
        );
        assert!(
            scroll.offset().y < before,
            "before {before:?}, after {:?}, viewport {:?}, content {:?}",
            scroll.offset(),
            scroll.viewport_bounds(),
            scroll.content_size()
        );
    })
    .unwrap();
}

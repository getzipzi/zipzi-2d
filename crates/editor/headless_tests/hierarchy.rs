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
fn default_layout_stacks_trees_left_and_inspectors_right(cx: &mut TestAppContext) {
    use gpui_kit::component::dock::PanelInfo;
    let harness = setup(cx);
    harness.view.read_with(cx, |view, cx| {
        let layout = view.dock.as_ref().unwrap().area.read(cx).dump(cx);
        assert_eq!(layout.version, Some(2));
        let columns = &layout.center.children;
        assert_eq!(columns.len(), 3);
        assert!(matches!(&columns[0].info, PanelInfo::Stack { axis: 1, .. }));
        assert!(matches!(&columns[2].info, PanelInfo::Stack { axis: 1, .. }));
        assert_eq!(columns[0].children[0].children[0].panel_name, "MeshTree");
        assert_eq!(columns[0].children[1].children[0].panel_name, "Deformers");
        assert_eq!(columns[1].children[0].panel_name, "Canvas");
        assert_eq!(columns[2].children[0].children[0].panel_name, "Properties");
        assert_eq!(columns[2].children[1].children[0].panel_name, "Parameters");
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
    let PanelInfo::Stack { sizes, .. } = saved.center.info else {
        panic!("default center should be a split");
    };
    assert!(sizes[1].as_f32() > sizes[0].as_f32());
    assert!(sizes[1].as_f32() > sizes[2].as_f32());
}

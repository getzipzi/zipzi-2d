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
        let columns = &layout.center.children[0].children;
        assert_eq!(
            columns[0]
                .children
                .iter()
                .map(|p| p.panel_name.as_str())
                .collect::<Vec<_>>(),
            ["MeshTree", "Deformers", "DrawOrder"]
        );
        assert_eq!(columns[1].children[0].panel_name, "Canvas");
        assert_eq!(
            columns[2]
                .children
                .iter()
                .map(|p| p.panel_name.as_str())
                .collect::<Vec<_>>(),
            ["Properties", "Relations"]
        );
        assert_eq!(
            layout.center.children[1].children[0].panel_name,
            "Parameters"
        );
        assert_eq!(layout.center.children[2].children[0].panel_name, "Keyforms");
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
    let PanelInfo::Stack { sizes, .. } = saved.center.children[0].info.clone() else {
        panic!("default center should be a split");
    };
    assert!(sizes[1].as_f32() > sizes[0].as_f32());
    assert!(sizes[1].as_f32() > sizes[2].as_f32());
}

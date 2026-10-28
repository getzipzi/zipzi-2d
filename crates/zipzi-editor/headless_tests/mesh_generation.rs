use super::{Harness, document_fixture, setup_with_document};
use gpui_kit::component::WindowExt;
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};
use zipzi_editor_core::{CommandId, EditorCommand};

fn settle(cx: &mut TestAppContext) {
    for _ in 0..3 {
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(110));
    }
    cx.run_until_parked();
}

fn open(harness: &Harness, cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(harness.window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::GenerateMesh, window, cx)
        });
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        assert!(window.find("mesh-generation-form").visible());
    })
    .unwrap();
}
fn fill(harness: &Harness, field: &'static str, value: &str, cx: &mut TestAppContext) {
    cx.update_window(harness.window, |_, window, cx| {
        window.click(field, cx);
        window.press("cmd-a", cx);
        window.input(value, cx);
    })
    .unwrap();
    settle(cx);
}

#[gpui_kit::test]
fn generation_submits_once_and_refreshes_history(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::rectangle_document(false).unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open_grid(&harness, cx);
    fill(&harness, "mesh-generation-columns", "4.9", cx);
    fill(&harness, "mesh-generation-rows", "3.9", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
        harness
            .view
            .read(cx)
            .editor
            .document()
            .map(|d| {
                assert_eq!((d.layers()[0].vertices, d.layers()[0].triangles), (20, 24));
                assert_eq!(d.history_lengths(), (1, 0));
            })
            .unwrap();
        harness
            .view
            .update(cx, |view, cx| view.run_command(CommandId::Undo, window, cx));
        assert_eq!(harness.view.read(cx).editor.layers()[0].vertices, 4);
        harness
            .view
            .update(cx, |view, cx| view.run_command(CommandId::Redo, window, cx));
        assert_eq!(harness.view.read(cx).editor.layers()[0].vertices, 20);
    })
    .unwrap();
}

#[gpui_kit::test]
fn invalid_bound_counts_and_cancel_leave_mesh_unchanged(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::rectangle_document(true).unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open(&harness, cx);
    // Existing deformation bindings cannot use alpha generation: default to grid.
    cx.update_window(harness.window, |_, window, _cx| {
        assert!(window.find("mesh-generation-columns").visible());
        assert!(window.try_find("mesh-generation-edge-spacing").is_none());
    })
    .unwrap();
    for value in ["0", "-1", "abc", "256", "4"] {
        fill(&harness, "mesh-generation-columns", value, cx);
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("mesh-generation-error").visible());
            window.press("enter", cx);
        })
        .unwrap();
        settle(cx);
        harness.view.read_with(cx, |v, _| {
            assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0));
        });
    }
    cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-cancel", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        assert!(!window.has_active_dialog(cx));
        assert_eq!(harness.view.read(cx).editor.layers()[0].vertices, 4);
    })
    .unwrap();
}

#[gpui_kit::test]
fn changed_document_blocks_dialog_submission(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::rectangle_document(false).unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open_grid(&harness, cx);
    cx.update_window(harness.window, |_, _, cx| {
        harness.view.update(cx, |view, cx| {
            let target = view
                .editor
                .document()
                .unwrap()
                .context()
                .selection
                .clone()
                .unwrap();
            view.editor
                .execute(EditorCommand::SetObjectsLocked {
                    objects: vec![target],
                    locked: true,
                })
                .unwrap();
            cx.notify();
        });
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("mesh-generation-error").visible());
        window.press("enter", cx);
    })
    .unwrap();
    settle(cx);
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.layers()[0].vertices, 4);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
}

#[gpui_kit::test]
fn live_preview_presets_and_cancel_do_not_edit_document(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::rectangle_document(false).unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open_grid(&harness, cx);
    for preset in [
        "mesh-generation-dense",
        "mesh-generation-sparse",
        "mesh-generation-medium",
    ] {
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            window.click(preset, cx);
        })
        .unwrap();
        settle(cx);
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("mesh-generation-preview").visible());
            assert!(window.find("mesh-generation-counts").visible());
            let doc = harness.view.read(cx).editor.document().unwrap();
            assert_eq!(doc.layers()[0].vertices, 4);
            assert_eq!(doc.history_lengths(), (0, 0));
            assert!(!doc.modified());
        })
        .unwrap();
    }
    cx.update_window(harness.window, |_, window, cx| {
        window.click("preview-wireframe", cx);
        window.render_frame(cx);
        assert!(window.try_find("mesh-generation-quality-help").is_none());
        window.click("mesh-generation-cancel", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        assert!(!window.has_active_dialog(cx));
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn changed_inputs_remove_old_preview_until_valid_again(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::rectangle_document(false).unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open_grid(&harness, cx);
    fill(&harness, "mesh-generation-columns", "0", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("mesh-generation-counts").is_none());
        window.click("mesh-generation-submit", cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0)
        );
    })
    .unwrap();
    fill(&harness, "mesh-generation-columns", "4", cx);
    fill(&harness, "mesh-generation-rows", "4", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("mesh-generation-counts").visible());
        window.click("mesh-generation-submit", cx);
    })
    .unwrap();
    settle(cx);
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().layers()[0].vertices, 25);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
}

#[gpui_kit::test]
fn scaled_generator_fits_the_window_and_keeps_preview_actions_accessible(cx: &mut TestAppContext) {
    use crate::settings::{Settings, UiScale};
    use gpui_kit::{prelude::*, px, size};
    let (doc, assets) = document_fixture::rectangle_document(false).unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open_grid(&harness, cx);
    for scale in UiScale::ALL {
        cx.update_window(harness.window, |_, window, cx| {
            cx.update_global::<Settings, _>(|settings, _| settings.set_ui_scale(scale).unwrap());
            crate::theme::change(gpui_kit::component::ThemeMode::Dark, cx);
            window.refresh();
        })
        .unwrap();
        for (width, height) in [(800., 480.), (900., 620.), (1100., 740.)] {
            cx.simulate_window_resize(harness.window, size(px(width), px(height)));
            settle(cx);
            cx.update_window(harness.window, |_, window, cx| {
                window.render_frame(cx);
                let form = window.find("mesh-generation-form").bounds();
                let preview = window.find("mesh-generation-preview").bounds();
                let settings = window.find("mesh-generation-settings").bounds();
                assert!(
                    preview.right() <= form.right() + px(1.),
                    "preview overflows form at {scale:?}: {preview:?} in {form:?}"
                );
                assert!(settings.right() <= preview.left());
                for id in [
                    "mesh-generation-preview",
                    "mesh-generation-submit",
                    "mesh-generation-cancel",
                ] {
                    let bounds = window.find(id).bounds();
                    assert!(
                        bounds.left() >= px(0.)
                            && bounds.top() >= px(0.)
                            && bounds.right() <= px(width)
                            && bounds.bottom() <= px(height),
                        "{id} outside {width}x{height} at {scale:?}: {bounds:?}"
                    );
                }
            })
            .unwrap();
        }
    }
}

#[gpui_kit::test]
fn generator_uses_square_preview_and_keeps_actions_visible_when_resized(cx: &mut TestAppContext) {
    use gpui_kit::{px, size};
    let (doc, assets) = document_fixture::rectangle_document(false).unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open_grid(&harness, cx);
    for (width, height) in [(1100., 740.), (900., 620.), (1500., 1000.)] {
        cx.simulate_window_resize(harness.window, size(px(width), px(height)));
        settle(cx);
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            let preview = window.find("preview-viewport").bounds();
            let settings = window.find("mesh-generation-settings").bounds();
            assert!((preview.size.width - preview.size.height).as_f32().abs() < 1.);
            assert!(settings.right() <= preview.left());
            assert!(preview.size.height.as_f32() >= height - 210.);
            assert!(window.find("mesh-generation-submit").visible());
            assert!(window.find("mesh-generation-cancel").visible());
            assert!(window.try_find("mesh-generation-help").is_none());
            assert!(preview.bottom() < px(height));
        })
        .unwrap();
    }
}

fn open_grid(harness: &Harness, cx: &mut TestAppContext) {
    open(harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        if window.try_find("mesh-generation-columns").is_none() {
            window.click("mesh-generation-algorithm", cx);
            window.press("down", cx);
            window.press("enter", cx);
        }
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("mesh-generation-columns").visible());
    })
    .unwrap();
}

fn assert_alpha_default(harness: &Harness, cx: &mut TestAppContext) {
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("mesh-generation-edge-spacing").visible());
        assert!(window.try_find("mesh-generation-columns").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn alpha_controls_preview_apply_and_reopen(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::alpha_document().unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open(&harness, cx);
    assert_alpha_default(&harness, cx);
    for id in [
        "mesh-generation-alpha-standard",
        "mesh-generation-alpha-heavy",
        "mesh-generation-alpha-light",
    ] {
        cx.update_window(harness.window, |_, window, cx| window.click(id, cx))
            .unwrap();
        settle(cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        });
    }
    for value in ["0", "NaN", "-1"] {
        fill(&harness, "mesh-generation-edge-spacing", value, cx);
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("mesh-generation-counts").is_none());
            window.click("mesh-generation-submit", cx);
            assert_eq!(
                harness
                    .view
                    .read(cx)
                    .editor
                    .document()
                    .unwrap()
                    .history_lengths(),
                (0, 0)
            );
        })
        .unwrap();
    }
    fill(&harness, "mesh-generation-edge-spacing", "25", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-alpha-heavy", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("mesh-generation-counts").visible());
        window.click("mesh-generation-submit", cx);
    })
    .unwrap();
    settle(cx);
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
        assert!(view.editor.layers()[0].vertices > 4);
    });
    open(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        // Generated contours reopen directly in Alpha mode.
        assert!(window.find("mesh-generation-edge-spacing").visible());
        window.click("mesh-generation-cancel", cx);
        harness
            .view
            .update(cx, |view, cx| view.run_command(CommandId::Undo, window, cx));
        assert_eq!(harness.view.read(cx).editor.layers()[0].vertices, 4);
        harness
            .view
            .update(cx, |view, cx| view.run_command(CommandId::Redo, window, cx));
        assert!(harness.view.read(cx).editor.layers()[0].vertices > 4);
    })
    .unwrap();
}

#[gpui_kit::test]
fn alpha_small_window_keeps_preview_and_actions_accessible(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::alpha_document().unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open(&harness, cx);
    assert_alpha_default(&harness, cx);
    cx.simulate_window_resize(
        harness.window,
        gpui_kit::size(gpui_kit::px(900.), gpui_kit::px(620.)),
    );
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("mesh-generation-submit").visible());
        assert!(window.find("mesh-generation-cancel").visible());
        assert!(window.find("preview-viewport").bounds().bottom() < gpui_kit::px(620.));
        window.click("mesh-generation-cancel", cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0)
        );
    })
    .unwrap();
}

fn number_value(harness: &Harness, field: &'static str, cx: &mut TestAppContext) -> String {
    cx.update_window(harness.window, |_, window, cx| {
        window.click(field, cx);
        let value = window.focused_input(cx).unwrap().value(cx).to_string();
        window.blur(cx);
        value
    })
    .unwrap()
}

fn scrub_number(
    harness: &Harness,
    field: &'static str,
    distance: f32,
    shift: bool,
    cx: &mut TestAppContext,
) {
    use super::slide_number_input::{press, release, slide};
    use gpui_kit::{point, px};
    let origin = cx
        .update_window(harness.window, |_, window, cx| {
            window.blur(cx);
            window.render_frame(cx);
            assert!(
                window
                    .within(field)
                    .try_find("slide-number-input")
                    .is_some()
            );
            let origin = window.find(field).bounds().center();
            press(window, origin, cx);
            origin
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        let position = origin + point(px(distance), px(0.));
        // Many small moves must accumulate even when each emits no new integer.
        for i in 1..=5 {
            slide(
                window,
                origin + point(px(distance * i as f32 / 5.), px(0.)),
                shift,
                cx,
            );
        }
        release(window, position, cx);
    })
    .unwrap();
    settle(cx);
}

#[gpui_kit::test]
fn grid_number_sliding_keeps_integer_counts_and_refreshes_preview(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::rectangle_document(false).unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open_grid(&harness, cx);
    scrub_number(&harness, "mesh-generation-columns", 20., false, cx);
    assert_eq!(number_value(&harness, "mesh-generation-columns", cx), "10");
    scrub_number(&harness, "mesh-generation-rows", 100., true, cx);
    assert_eq!(number_value(&harness, "mesh-generation-rows", cx), "9");
    cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-columns", cx);
        window.press("up", cx);
        assert_eq!(window.focused_input(cx).unwrap().value(cx).as_ref(), "11");
        window.press("down", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("mesh-generation-counts").visible());
        let document = harness.view.read(cx).editor.document().unwrap();
        assert_eq!(document.history_lengths(), (0, 0));
        assert_eq!(document.layers()[0].vertices, 4);
        window.click("mesh-generation-cancel", cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn alpha_number_sliding_and_escape_only_cancels_the_drag(cx: &mut TestAppContext) {
    use super::slide_number_input::{press, release, slide};
    use gpui_kit::{point, px};
    let (doc, assets) = document_fixture::alpha_document().unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open(&harness, cx);
    scrub_number(&harness, "mesh-generation-edge-spacing", 20., false, cx);
    assert!(
        number_value(&harness, "mesh-generation-edge-spacing", cx)
            .parse::<f64>()
            .unwrap()
            > 6.
    );
    scrub_number(&harness, "mesh-generation-alpha-threshold", 20., false, cx);
    let threshold = number_value(&harness, "mesh-generation-alpha-threshold", cx)
        .parse::<f64>()
        .unwrap();
    assert!(threshold > 8.);
    assert_eq!(threshold.fract(), 0.);
    let original = number_value(&harness, "mesh-generation-outer-margin", cx);
    let origin = cx
        .update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            let origin = window
                .find("mesh-generation-outer-margin")
                .bounds()
                .center();
            press(window, origin, cx);
            origin
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin + point(px(100.), px(0.)), false, cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.press("escape", cx);
        release(window, origin, cx);
        assert!(window.has_active_dialog(cx));
    })
    .unwrap();
    assert_eq!(
        number_value(&harness, "mesh-generation-outer-margin", cx),
        original
    );
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("mesh-generation-counts").visible());
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0)
        );
        window.click("mesh-generation-cancel", cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn alpha_decimal_inputs_preserve_precision_and_generate(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::alpha_document().unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open(&harness, cx);
    for (field, value, expected) in [
        ("mesh-generation-edge-spacing", "12.9", "12.9"),
        ("mesh-generation-interior-spacing", "40.9", "40.9"),
        ("mesh-generation-outer-margin", "1.9", "1.9"),
        ("mesh-generation-alpha-threshold", "8", "8"),
    ] {
        fill(&harness, field, value, cx);
        cx.update_window(harness.window, |_, window, cx| window.blur(cx))
            .unwrap();
        settle(cx);
        assert_eq!(number_value(&harness, field, cx), expected);
    }
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("mesh-generation-counts").visible());
        assert!(window.try_find("mesh-generation-error").is_none());
        window.click("mesh-generation-submit", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        assert!(!window.has_active_dialog(cx));
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (1, 0)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn edge_and_fill_choices_are_independent_and_retain_fill_settings(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::alpha_document().unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open(&harness, cx);
    fill(&harness, "mesh-generation-edge-ratio", "1.5", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-edge-mode", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("mesh-generation-edge-width").visible());
        assert!(window.try_find("mesh-generation-outer-margin").is_none());
        window.click("mesh-generation-fill-algorithm", cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    settle(cx);
    assert_eq!(
        number_value(&harness, "mesh-generation-edge-ratio", cx),
        "2"
    );
    fill(&harness, "mesh-generation-edge-ratio", "3", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-fill-algorithm", cx);
        window.press("up", cx);
        window.press("up", cx);
        window.press("enter", cx);
    })
    .unwrap();
    settle(cx);
    assert_eq!(
        number_value(&harness, "mesh-generation-edge-ratio", cx),
        "1.5"
    );
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("mesh-generation-edge-width").visible());
        assert!(window.find("mesh-generation-counts").visible());
        window.click("mesh-generation-cancel", cx);
    })
    .unwrap();
}

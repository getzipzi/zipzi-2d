//! Exercise the production window's pointer/key paths and inspect real Metal frames.
use super::super::{fixture, harness::Harness};
use super::common::save;
use gpui_kit::{point, px, test::TestWindowExt};
use zipzi_editor_core::{Editor, ModelPoint};

fn click_model(h: &mut Harness, p: ModelPoint) -> anyhow::Result<()> {
    h.cx.update_window(h.window, |_, window, cx| {
        let p = h
            .view
            .read(cx)
            .modeling_frame()
            .unwrap()
            .canvas
            .transform
            .to_viewport(p);
        // Keyboard helpers retain modifier state after Undo/Redo. This is a
        // plain click, so explicitly release those modifiers for the event.
        window.click_with_options(
            "canvas-body",
            gpui_kit::test::ClickOptions::new().with_offset(point(px(p.x), px(p.y))),
            cx,
        );
    })?;
    Ok(())
}
fn gesture_model(
    h: &mut Harness,
    a: ModelPoint,
    b: ModelPoint,
    modifiers: gpui_kit::Modifiers,
) -> anyhow::Result<()> {
    use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
    h.cx.update_window(h.window, |_, window, cx| {
        let t = h.view.read(cx).modeling_frame().unwrap().canvas.transform;
        let origin = window.find("canvas-body").bounds().origin;
        let screen = |p| {
            let p = t.to_viewport(p);
            origin + point(px(p.x), px(p.y))
        };
        let a = screen(a);
        let b = screen(b);
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: a,
                modifiers,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        if a != b {
            window.dispatch_event(
                MouseMoveEvent {
                    position: b,
                    pressed_button: Some(MouseButton::Left),
                    modifiers,
                    ..Default::default()
                }
                .to_platform_input(),
                cx,
            );
        }
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: b,
                modifiers,
                click_count: 1,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
    })?;
    Ok(())
}
fn delete_edge(h: &mut Harness, point: ModelPoint) -> anyhow::Result<()> {
    gesture_model(
        h,
        point,
        point,
        gpui_kit::Modifiers {
            shift: true,
            ..Default::default()
        },
    )?;
    h.cx.update_window(h.window, |_, window, cx| window.press("delete", cx))?;
    Ok(())
}
fn click(h: &mut Harness, id: &'static str) -> anyhow::Result<()> {
    h.cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(id, gpui_kit::test::ClickOptions::new(), cx);
    })?;
    Ok(())
}
fn check(h: &Harness, vertices: usize, history: (usize, usize)) {
    h.view.read_with(&h.cx, |view, _| {
        assert!(view.editor.manual_active());
        assert_eq!(view.editor.layers()[0].vertices, vertices);
        assert_eq!(view.editor.document().unwrap().history_lengths(), history);
    });
}

fn meshes(document: &zipzi_editor_core::Document) -> Vec<zipzi_core::Mesh> {
    let host = document.runtime_host();
    let runtime = host.read();
    let model = runtime.document(document.runtime_session_id()).unwrap();
    model
        .mesh_order()
        .iter()
        .map(|id| model.get_mesh(id).unwrap().clone())
        .collect()
}

pub(crate) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    let mut h = Harness::with_size(1200., 820.)?;
    let (document, assets) = fixture::alpha_document()?;
    h._assets = assets;
    h.cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        crate::i18n::activate(Some("zh-CN"));
        h.view.update(cx, |v, cx| {
            v.replace_editor(Editor::from_document(document), cx);
            v.editor
                .enter_manual_mesh(v.editor.active_id().unwrap())
                .unwrap();
            v.redraw(cx);
        });
    })?;
    let image = h.capture()?;
    save(&image, directory, "manual-mesh-01-enter.png")?;
    click_model(&mut h, ModelPoint { x: 128., y: 128. })?;
    let image = h.capture()?;
    check(&h, 5, (1, 0));
    save(&image, directory, "manual-mesh-02-add.png")?;
    gesture_model(
        &mut h,
        ModelPoint { x: 128., y: 128. },
        ModelPoint { x: 155., y: 140. },
        gpui_kit::Modifiers::default(),
    )?;
    let image = h.capture()?;
    check(&h, 5, (2, 0));
    save(&image, directory, "manual-mesh-03-drag.png")?;
    gesture_model(
        &mut h,
        ModelPoint { x: 24., y: 24. },
        ModelPoint { x: 155., y: 140. },
        gpui_kit::Modifiers {
            platform: true,
            ..Default::default()
        },
    )?;
    let image = h.capture()?;
    check(&h, 5, (3, 0));
    save(&image, directory, "manual-mesh-04-constraint.png")?;
    delete_edge(&mut h, ModelPoint { x: 89.5, y: 82. })?;
    h.capture()?;
    check(&h, 5, (4, 0));
    h.cx.update_window(h.window, |_, window, cx| window.press("cmd-z", cx))?;
    h.capture()?;
    check(&h, 5, (3, 1));
    h.cx.update_window(h.window, |_, window, cx| window.press("cmd-shift-z", cx))?;
    h.capture()?;
    check(&h, 5, (4, 0));
    click_model(&mut h, ModelPoint { x: 128., y: 244. })?;
    let image = h.capture()?;
    assert!(
        h.view
            .read_with(&h.cx, |v, _| v.editor.canvas_notice().is_some())
    );
    save(&image, directory, "manual-mesh-05-invalid.png")?;
    click(&mut h, "tool_manual_preview")?;
    let image = h.capture()?;
    save(&image, directory, "manual-mesh-06-result.png")?;
    click(&mut h, "tool_generate_mesh")?;
    let image = h.capture()?;
    save(&image, directory, "manual-mesh-07-generation.png")?;
    click(&mut h, "mesh-generation-cancel")?;
    h.capture()?;
    check(&h, 5, (4, 0));
    click(&mut h, "btn_isolate_selection")?;
    h.capture()?;
    check(&h, 5, (4, 0));
    h.cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |v, cx| {
            let binding = v.editor.primary_viewport_binding().unwrap();
            assert!(v.editor.viewport_isolated(binding));
            v.editor
                .set_viewport_isolation_display(binding, zipzi_editor_core::IsolationDisplay::Hide)
                .unwrap();
            v.redraw(cx);
        });
    })?;
    // Return to full-source display while isolated; it must remain visible.
    click(&mut h, "tool_manual_preview")?;
    let image = h.capture()?;
    save(&image, directory, "manual-mesh-08-isolated.png")?;
    delete_edge(&mut h, ModelPoint { x: 128., y: 24. })?;
    let image = h.capture()?;
    check(&h, 5, (5, 0));
    save(&image, directory, "manual-mesh-09-outline-delete.png")?;
    gesture_model(
        &mut h,
        ModelPoint { x: 24., y: 24. },
        ModelPoint { x: 128., y: 60. },
        gpui_kit::Modifiers {
            platform: true,
            ..Default::default()
        },
    )?;
    let image = h.capture()?;
    check(&h, 6, (6, 0));
    save(&image, directory, "manual-mesh-10-outline-extend.png")?;
    h.cx.update_window(h.window, |_, window, cx| window.press("cmd-z", cx))?;
    h.capture()?;
    check(&h, 5, (5, 1));
    h.cx.update_window(h.window, |_, window, cx| window.press("cmd-shift-z", cx))?;
    h.capture()?;
    check(&h, 6, (6, 0));
    let (path, expected_meshes) = h.view.read_with(&h.cx, |v, _| {
        let doc = v.editor.document().unwrap();
        (doc.path().unwrap().to_path_buf(), meshes(doc))
    });
    h.cx.update_window(h.window, |_, window, cx| window.press("cmd-s", cx))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.view.read_with(&h.cx, |v, _| {
        v.editor
            .document()
            .is_none_or(|document| document.modified())
    }) {
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "Native Save did not complete"
        );
        h.cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let image = h.capture()?;
    check(&h, 6, (6, 0));
    save(&image, directory, "manual-mesh-11-saved.png")?;
    let reopened = zipzi_editor_core::Document::open(&path)?;
    assert_eq!(
        meshes(&reopened),
        expected_meshes,
        "Save and reopen preserve exact geometry and constraints"
    );
    Ok(())
}

//! Exercise the production window's pointer/key paths and inspect real Metal frames.
use super::super::{fixture, harness::Harness};
use super::common::save;
use gpui_kit::{point, px, test::TestWindowExt};
use zipzi_editor_core::{Editor, ModelPoint};

#[path = "../../../../fixtures/multi-mesh-edit.rs"]
mod multi_fixture;

pub(super) fn details(directory: &std::path::Path) -> anyhow::Result<()> {
    use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, MouseUpEvent};
    use zipzi_editor_core::{Document, ObjectKind, ObjectRef, ViewportPoint};
    let mut h = Harness::with_size(1200., 820.)?;
    let mut session = multi_fixture::create(
        h._assets.path(),
        include_bytes!("../../../zipzi-editor-core/tests/fixtures/alpha-asymmetric.png"),
    )?;
    let path = h._assets.path().join("mesh-details.zipzi");
    crate::test_fixture::save_fixture(&mut session, &path)?;
    let mut document = Document::open(&path)?;
    document.select_objects(
        [2, 4]
            .map(|n| ObjectRef::new(ObjectKind::Mesh, multi_fixture::id(n)))
            .into(),
    )?;
    h.cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        h.view.update(cx, |view, cx| {
            view.replace_editor(Editor::from_document(document), cx);
            view.editor.reset_view();
            view.editor.pan_by(ViewportPoint { x: -100., y: 120. });
            view.refresh_document(window, cx);
        });
    })?;
    h.capture()?;
    h.view.update(&mut h.cx, |view, cx| {
        view.editor.set_zoom_at(2.5, None);
        view.redraw(cx);
    });
    save(
        &h.capture()?,
        directory,
        "mesh-details-object-move-handle.png",
    )?;
    let at = h.view.read_with(&h.cx, |view, _| {
        let frame = &view.modeling_frame().unwrap().canvas;
        (1..8)
            .flat_map(|x| {
                (1..8).map(move |y| ModelPoint {
                    x: 20. + x as f32 * 10.,
                    y: 20. + y as f32 * 10.,
                })
            })
            .find(|&p| {
                frame
                    .geometry
                    .mesh_hits(p)
                    .iter()
                    .any(|id| *id == multi_fixture::id(2))
            })
            .unwrap()
    });
    click_model(&mut h, at)?;
    h.capture()?;
    h.cx.update_window(h.window, |_, window, cx| {
        let frame = &h.view.read(cx).modeling_frame().unwrap().canvas;
        let p = frame.transform.to_viewport(at);
        let position = window.find("canvas-body").bounds().origin + point(px(p.x), px(p.y));
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 2,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 2,
            }
            .to_platform_input(),
            cx,
        );
    })?;
    h.capture()?;
    h.view.read_with(&h.cx, |view, _| {
        assert!(view.editor.manual_active());
        assert_eq!(view.editor.manual_targets().len(), 2);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
    click_model(&mut h, ModelPoint { x: 20., y: 20. })?;
    h.capture()?;
    gesture_model(
        &mut h,
        ModelPoint { x: 140., y: 140. },
        ModelPoint { x: 140., y: 140. },
        gpui_kit::Modifiers {
            shift: true,
            ..Default::default()
        },
    )?;
    save(
        &h.capture()?,
        directory,
        "mesh-details-multi-mesh-colors.png",
    )?;
    h.view.read_with(&h.cx, |view, _| {
        let overlay = view.editor.overlay();
        assert_eq!(overlay.mesh_styles.len(), 2);
        assert_ne!(
            overlay.mesh_styles[0].color_index,
            overlay.mesh_styles[1].color_index
        );
        assert_eq!(overlay.vertices.iter().filter(|v| v.selected).count(), 2);
    });
    h.cx.update_window(h.window, |_, _, cx| {
        crate::theme::change(gpui_kit::component::ThemeMode::Light, cx);
        h.view.update(cx, |view, cx| view.redraw(cx));
    })?;
    save(
        &h.capture()?,
        directory,
        "mesh-details-multi-mesh-colors-light.png",
    )?;
    h.cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |view, cx| {
            view.editor.exit_manual_mesh();
            view.redraw(cx);
        });
    })?;
    save(
        &h.capture()?,
        directory,
        "mesh-details-object-move-handle-light.png",
    )?;
    Ok(())
}

pub(super) fn click_model(h: &mut Harness, p: ModelPoint) -> anyhow::Result<()> {
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
pub(super) fn gesture_model(
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
    let snapshot =
        crate::test_fixture::published_snapshot(&host, document.runtime_session_id()).unwrap();
    let model = snapshot.document();
    model
        .mesh_order()
        .iter()
        .map(|id| model.mesh(id).unwrap().clone())
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
    click(&mut h, "tool_manual_source")?;
    let image = h.capture()?;
    save(&image, directory, "manual-mesh-06-source.png")?;
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
    // Return to model-space editing while retaining explicit viewport isolation.
    click(&mut h, "tool_manual_source")?;
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
    let (document, assets) = fixture::mixed_document()?;
    h._assets = assets;
    h.cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |v, cx| {
            v.replace_editor(Editor::from_document(document), cx);
            v.editor
                .enter_manual_mesh(v.editor.active_id().unwrap())
                .unwrap();
            v.redraw(cx);
        });
    })?;
    save(&h.capture()?, directory, "manual-mesh-12-mixed-model.png")?;
    let point = h.view.read_with(&h.cx, |v, _| {
        let frame = &v.modeling_frame().unwrap().canvas;
        let mesh = frame
            .geometry
            .mesh("a8210000-0000-4000-8000-000000000002")
            .unwrap();
        ModelPoint {
            x: (mesh.vertices[0].1.x + mesh.vertices[1].1.x + mesh.vertices[4].1.x) / 3.,
            y: (mesh.vertices[0].1.y + mesh.vertices[1].1.y + mesh.vertices[4].1.y) / 3.,
        }
    });
    click_model(&mut h, point)?;
    save(&h.capture()?, directory, "manual-mesh-13-mixed-insert.png")?;
    h.view.read_with(&h.cx, |v, _| {
        let d = v.editor.document().unwrap();
        let host = d.runtime_host();
        let snapshot =
            crate::test_fixture::published_snapshot(&host, d.runtime_session_id()).unwrap();
        let model = snapshot.document();
        assert_eq!(d.history_lengths(), (1, 0));
        assert_eq!(
            model
                .mesh("a8210000-0000-4000-8000-000000000002")
                .unwrap()
                .vertex_ids
                .len(),
            6
        );
        assert_eq!(
            model
                .glue("a8210000-0000-4000-8000-000000000020")
                .unwrap()
                .pairs[0]
                .vertex_b,
            None
        );
    });
    click(&mut h, "tool_manual_source")?;
    h.cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |v, cx| {
            v.editor.fit_view();
            v.redraw(cx);
        });
    })?;
    save(&h.capture()?, directory, "manual-mesh-14-mixed-source.png")?;
    Ok(())
}

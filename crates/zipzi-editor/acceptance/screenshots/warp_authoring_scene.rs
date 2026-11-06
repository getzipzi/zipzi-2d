//! Actual Metal comparison of controller influence over a textured synthetic mesh.
use super::super::harness::Harness;
use super::{common::save, properties_scene::show};
use gpui_kit::{
    InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, point,
    px, test::TestWindowExt,
};
use zipzi_core::{
    Canvas, Mesh, Transform, TransformData, Vec2, WarpAuthoringSettings, WarpBezierEditMode,
    WarpBezierGrid, WarpTransform,
};
use zipzi_editor_core::{
    Document, Editor, EditorCommand, ModelPoint, ObjectKind, ObjectRef, ViewportPoint,
};

pub(super) fn fixture(
    mode: WarpBezierEditMode,
) -> anyhow::Result<(Document, tempfile::TempDir, ObjectRef)> {
    fixture_with_density(mode, 8, 3)
}

pub(super) fn fixture_with_density(
    mode: WarpBezierEditMode,
    grid: u32,
    bezier: u32,
) -> anyhow::Result<(Document, tempfile::TempDir, ObjectRef)> {
    let assets = tempfile::tempdir()?;
    let path = assets.path().join("checker.png");
    image::RgbaImage::from_fn(256, 256, |x, y| {
        let rgba = if x % 32 < 2 || y % 32 < 2 {
            [48, 76, 110, 255]
        } else if (x / 32 + y / 32) % 2 == 0 {
            [177, 204, 226, 255]
        } else {
            [221, 231, 239, 255]
        };
        image::Rgba(rgba)
    })
    .save(&path)?;
    let id = |n| uuid::Uuid::from_u128(n).to_string();
    let object = ObjectRef::new(ObjectKind::Deformer, id(502));
    let mut sdk = crate::test_fixture::fixture_session(
        &uuid::Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::default(), 1.),
    )?;
    crate::test_fixture::publish_fixture(&mut sdk, "Warp editing fixture", |edit| {
        crate::test_fixture::insert_image(
            edit,
            crate::test_fixture::image_fixture(&id(2), "Checker", &path)?,
        )?;
        crate::test_fixture::insert_transform(
            edit,
            Transform {
                id: object.id.clone(),
                name: "Warp".into(),
                warp_authoring: Some(WarpAuthoringSettings {
                    bezier_grid: Some(WarpBezierGrid {
                        rows: bezier,
                        columns: bezier,
                    }),
                    edit_mode: mode,
                    ..Default::default()
                }),
                data: TransformData::Warp(WarpTransform {
                    rows: grid,
                    columns: grid,
                    quad: true,
                    bezier: None,
                    points: (0..=grid)
                        .flat_map(|y| {
                            (0..=grid).map(move |x| {
                                Vec2::new(
                                    32. + 192. * x as f32 / grid as f32,
                                    32. + 192. * y as f32 / grid as f32,
                                )
                            })
                        })
                        .collect(),
                }),
                ..Default::default()
            },
        )?;
        crate::test_fixture::insert_fixture_mesh(
            edit,
            Mesh {
                id: id(3),
                name: "Checker".into(),
                texture_asset_id: id(2).into(),
                deformer_id: object.id.clone(),
                vertex_ids: (1..=81).collect(),
                base_positions: (0..=8)
                    .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8., y as f32 / 8.)))
                    .collect(),
                uvs: (0..=8)
                    .flat_map(|y| (0..=8).map(move |x| Vec2::new(x as f32 / 8., y as f32 / 8.)))
                    .collect(),
                triangles: (0..8)
                    .flat_map(|y| {
                        (0..8).flat_map(move |x| {
                            let i = y * 9 + x + 1;
                            [[i, i + 1, i + 10], [i, i + 10, i + 9]]
                        })
                    })
                    .collect(),
                ..Default::default()
            },
        )
    })?;
    let saved = crate::test_fixture::save_fixture(&mut sdk, &assets.path().join("authoring"))?;
    Ok((Document::open(&saved.path)?, assets, object))
}

pub(super) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    for locale in ["en", "zh-CN", "ja"] {
        for (name, mode) in [
            ("local", WarpBezierEditMode::Local),
            ("smooth-local", WarpBezierEditMode::SmoothLocal),
            ("smooth-global", WarpBezierEditMode::SmoothGlobal),
            ("structure", WarpBezierEditMode::PreserveStructure),
        ] {
            let mut h = Harness::new()?;
            crate::i18n::activate(Some(locale));
            let (document, _assets, object) = fixture(mode)?;
            h.cx.update_window(h.window, |_, window, cx| {
                h.view.update(cx, |view, cx| {
                    view.replace_editor(Editor::from_document(document), cx);
                    view.editor
                        .execute(EditorCommand::SelectObject(object.clone()))
                        .unwrap();
                    view.editor
                        .execute(EditorCommand::ToggleDeformerGrid)
                        .unwrap();
                    view.editor.reset_view();
                    view.editor.pan_by(ViewportPoint { x: -128., y: 128. });
                    view.refresh_document(window, cx);
                })
            })?;
            h.capture()?;
            h.cx.update_window(h.window, |_, window, cx| {
                window.activate_window();
                let start = h.view.read_with(cx, |view, _| {
                    view.modeling_frame()
                        .unwrap()
                        .canvas
                        .transform
                        .to_viewport(ModelPoint {
                            x: 32. + 64. + 64. / 3.,
                            y: 96.,
                        })
                });
                let from =
                    window.find("canvas-body").bounds().origin + point(px(start.x), px(start.y));
                let end = from + point(px(0.), px(-30.));
                window.dispatch_event(
                    MouseDownEvent {
                        button: MouseButton::Left,
                        position: from,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                        first_mouse: false,
                    }
                    .to_platform_input(),
                    cx,
                );
                window.dispatch_event(
                    MouseMoveEvent {
                        position: end,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Modifiers::default(),
                    }
                    .to_platform_input(),
                    cx,
                );
                window.dispatch_event(
                    MouseUpEvent {
                        button: MouseButton::Left,
                        position: end,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                    }
                    .to_platform_input(),
                    cx,
                );
                h.view.update(cx, |view, cx| {
                    view.editor.execute(EditorCommand::ToggleWarpWhole).unwrap();
                    view.redraw(cx);
                });
                show(window, "property-select-bezier_edit_mode", cx);
            })?;
            h.view.read_with(&h.cx, |view, _| {
                assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0))
            });
            save(
                &h.capture()?,
                directory,
                &format!("warp-authoring-{name}-{locale}.png"),
            )?;
            if mode == WarpBezierEditMode::PreserveStructure {
                h.cx.update_window(h.window, |_, window, cx| {
                    window.render_frame(cx);
                    for field in [
                        "property-input-bezier_columns",
                        "property-input-bezier_rows",
                    ] {
                        window.click(field, cx);
                        window.press("cmd-a", cx);
                        window.input("2", cx);
                        window.press("enter", cx);
                        window.render_frame(cx);
                    }
                })?;
                save(
                    &h.capture()?,
                    directory,
                    &format!("warp-bezier-density-{locale}.png"),
                )?;
            }
        }
    }
    Ok(())
}

pub(super) fn run_keep_children(directory: &std::path::Path) -> anyhow::Result<()> {
    for locale in ["en", "zh-CN", "ja"] {
        let mut h = Harness::new()?;
        crate::i18n::activate(Some(locale));
        let (document, _assets, object) = fixture(WarpBezierEditMode::Local)?;
        h.cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |view, cx| {
                view.replace_editor(Editor::from_document(document), cx);
                view.editor
                    .execute(EditorCommand::SelectObject(object.clone()))
                    .unwrap();
                view.editor
                    .execute(EditorCommand::ToggleDeformerGrid)
                    .unwrap();
                view.editor.reset_view();
                view.editor.pan_by(ViewportPoint { x: -128., y: 128. });
                view.refresh_document(window, cx);
            })
        })?;
        h.capture()?;
        let before = h.view.read_with(&h.cx, |view, _| {
            view.modeling_frame()
                .unwrap()
                .canvas
                .geometry
                .meshes
                .iter()
                .flat_map(|m| m.vertices.iter().map(|(_, p)| *p))
                .collect::<Vec<_>>()
        });
        h.cx.update_window(h.window, |_, window, cx| {
            window.activate_window();
            show(window, "property-input-bezier_rows", cx);
            assert!(window.try_find("property-warp-keep-children").is_none());
            assert!(window.try_find("deformer-keep-children").is_none());
            let start = h.view.read_with(cx, |view, _| {
                let frame = &view.modeling_frame().unwrap().canvas;
                let c = frame
                    .geometry
                    .controls
                    .iter()
                    .find(|c| c.object == object)
                    .unwrap();
                let g = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
                let (a, b) = g.outline[0];
                ViewportPoint {
                    x: (a.x + b.x) * 0.5,
                    y: (a.y + b.y) * 0.5,
                }
            });
            let from = window.find("canvas-body").bounds().origin + point(px(start.x), px(start.y));
            let end = from + point(px(12.), px(18.));
            let modifiers = Modifiers {
                platform: true,
                ..Default::default()
            };
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position: from,
                    modifiers,
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseMoveEvent {
                    position: end,
                    pressed_button: Some(MouseButton::Left),
                    modifiers,
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position: end,
                    modifiers,
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
        })?;
        let pixels = h.capture()?;
        h.view.read_with(&h.cx, |view, _| {
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
            let frame = &view.modeling_frame().unwrap().canvas;
            let after = frame
                .geometry
                .meshes
                .iter()
                .flat_map(|m| m.vertices.iter().map(|(_, p)| *p))
                .collect::<Vec<_>>();
            for (a, b) in before.iter().zip(after) {
                assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
            }
        });
        save(
            &pixels,
            directory,
            &format!("warp-keep-children-{locale}.png"),
        )?;
    }
    Ok(())
}

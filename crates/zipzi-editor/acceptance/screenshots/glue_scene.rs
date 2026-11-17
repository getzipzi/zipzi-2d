//! Native Glue tool coverage with two independently authored source images.
use super::super::harness::{Harness, update};
use super::{
    common::save,
    manual_mesh_scene::{click_model, gesture_model},
};
use gpui_kit::test::TestWindowExt;
use zipzi_core::Vec2;
use zipzi_editor_core::{
    BrushSelectionSettings, Document, Editor, EditorCommand, ModelPoint, ObjectKind, ObjectRef,
};

#[path = "../../../../fixtures/multi-mesh-edit.rs"]
mod fixture;

pub(super) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    weight_selection(directory)?;
    separated_mesh_edit(directory)?;
    let mut h = Harness::with_size(1100., 790.)?;
    let mut session = fixture::create(
        h._assets.path(),
        include_bytes!("../../../zipzi-editor-core/tests/fixtures/alpha-asymmetric.png"),
    )?;
    let a = ObjectRef::new(ObjectKind::Mesh, fixture::id(2));
    let b = ObjectRef::new(ObjectKind::Mesh, fixture::id(4));
    let mesh = session.snapshot().document().mesh(&b.id).unwrap();
    let positions = mesh
        .vertex_ids
        .iter()
        .zip(&mesh.base_positions)
        .map(|(&id, p)| (id, Vec2::new(p.x - 100., p.y - 100.)))
        .collect::<Vec<_>>();
    crate::test_fixture::publish_fixture(&mut session, "Overlap", |edit| {
        edit.set_vertex_positions(&b.id, &positions)
    })?;
    let path = h._assets.path().join("glue.zipzi");
    crate::test_fixture::save_fixture(&mut session, &path)?;
    let mut document = Document::open(&path)?;
    document.select_objects(Vec::new())?;
    update(&mut h, |view, window, cx| {
        view.replace_editor(Editor::from_document(document), cx);
        view.refresh_document(window, cx);
    })?;
    h.capture()?;
    h.cx.update_window(h.window, |_, window, cx| {
        let editor = &h.view.read(cx).editor;
        assert!(
            !editor.command_available(editor.command(zipzi_editor_core::CommandId::GlueStitch))
        );
        window.hover("tool_glue", cx);
    })?;
    h.cx.advance_clock(std::time::Duration::from_secs(1));
    save(&h.capture()?, directory, "glue-disabled-hint.png")?;
    update(&mut h, |view, window, cx| -> anyhow::Result<()> {
        view.editor
            .execute(EditorCommand::SelectObjects(vec![a, b.clone()]))?;
        view.refresh_document(window, cx);
        Ok(())
    })??;
    h.capture()?;
    let click = |h: &mut Harness, id: &str| {
        h.cx.update_window(h.window, |_, window, cx| window.click(id.to_owned(), cx))
    };
    click(&mut h, "tool_manual_mesh")?;
    h.capture()?;
    assert!(
        h.view
            .read_with(&h.cx, |view, _| view.editor.manual_active())
    );
    click(&mut h, "mesh-glue-add-point")?;
    let joint = ModelPoint { x: 60., y: 65. };
    click_model(&mut h, joint)?;
    save(&h.capture()?, directory, "glue-point-created.png")?;
    h.view.read_with(&h.cx, |view, _| {
        let binding = view.editor.primary_viewport_binding().unwrap();
        let controls = view.editor.manual_glue_controls(binding).unwrap();
        assert_eq!(controls.bind_count, 1);
        assert_eq!(controls.pair_count, 1);
    });
    h.view.read_with(&h.cx, |view, _| {
        assert!(
            view.editor
                .overlay()
                .glues
                .iter()
                .flat_map(|g| &g.points)
                .any(|p| p.state == zipzi_editor_core::GluePointState::Binding)
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
    // Add another point directly, retaining the same seam and insertion tool.
    click_model(&mut h, ModelPoint { x: 70., y: 75. })?;
    save(&h.capture()?, directory, "glue-point-continued.png")?;
    h.view.read_with(&h.cx, |view, _| {
        let binding = view.editor.primary_viewport_binding().unwrap();
        assert_eq!(
            view.editor
                .manual_glue_controls(binding)
                .unwrap()
                .pair_count,
            2
        );
        assert!(
            view.editor
                .manual_glue_controls(binding)
                .unwrap()
                .add_glue_point
        );
    });
    update(&mut h, |view, _, cx| -> anyhow::Result<()> {
        view.editor.execute(EditorCommand::Undo)?;
        view.redraw(cx);
        Ok(())
    })??;
    h.capture()?;
    click_model(&mut h, joint)?;
    h.capture()?;
    click(&mut h, "mesh-glue-unbind")?;
    save(&h.capture()?, directory, "glue-mesh-unbound.png")?;
    click(&mut h, "mesh-glue-bind")?;
    h.capture()?;
    for locale in ["zh-CN", "ja"] {
        crate::i18n::activate(Some(locale));
        update(&mut h, |view, _, cx| view.redraw(cx))?;
        save(&h.capture()?, directory, &format!("glue-mesh-{locale}.png"))?;
    }
    crate::i18n::activate(Some("en"));
    click(&mut h, "mesh-glue-weights")?;
    save(&h.capture()?, directory, "glue-weight-workflow.png")?;
    click(&mut h, "glue-tab-points")?;
    h.capture()?;
    assert!(
        h.view
            .read_with(&h.cx, |view, _| view.editor.manual_active())
    );
    click(&mut h, "mesh-glue-weights")?;
    h.capture()?;
    click(&mut h, "tool_select")?;
    save(&h.capture()?, directory, "glue-follow-controls.png")?;
    let (glue, host, session) = h.view.read_with(&h.cx, |view, _| {
        let doc = view.editor.document().unwrap();
        let host = doc.runtime_host();
        let session = doc.runtime_session_id();
        let snapshot = crate::test_fixture::published_snapshot(&host, session).unwrap();
        let model = snapshot.document();
        (
            model.glue(&model.glue_order()[0]).unwrap().clone(),
            host,
            session,
        )
    });
    let glue_label = format!("glue-label-{}", glue.id);
    click(&mut h, &glue_label)?;
    h.capture()?;
    let version = host
        .read()
        .session_read(host.client_id(), session)?
        .version();
    {
        let client = host.client_id();
        let mut runtime = host.write();
        let draft = runtime.begin_draft(client, version)?;
        let (draft, ()) = runtime.batch(client, draft, |edit| {
            edit.set_vertex_positions(
                &b.id,
                &[(
                    glue.pairs[0].vertex_b.unwrap(),
                    Vec2::new(joint.x + 8., joint.y),
                )],
            )
        })?;
        runtime.publish(client, draft, "Move peer endpoint")?;
    }
    update(&mut h, |view, _, cx| -> anyhow::Result<()> {
        view.editor.sync_runtime()?;
        view.editor.execute(EditorCommand::SetGlueBrushSettings(
            BrushSelectionSettings {
                diameter: 96.,
                weight: 1.,
                ..Default::default()
            },
        ))?;
        view.redraw(cx);
        Ok(())
    })??;
    h.capture()?;
    for (button, expected, file) in [
        ("glue-mode-follow-b", [1., 0.], "glue-follow-peer.png"),
        ("glue-mode-release", [0., 0.], "glue-release-both.png"),
        ("glue-mode-reattach", [0.5, 0.5], "glue-restore-both.png"),
    ] {
        click(&mut h, button)?;
        click_model(
            &mut h,
            ModelPoint {
                x: joint.x + 4.,
                ..joint
            },
        )?;
        save(&h.capture()?, directory, file)?;
        assert_eq!(
            h.view
                .read_with(&h.cx, |view, _| view.editor.overlay().glue_weights[0]
                    .weights),
            expected
        );
    }
    click(&mut h, "brush-selection-more-button")?;
    save(&h.capture()?, directory, "glue-advanced-controls.png")?;
    click(&mut h, "glue-mode-a")?;
    click(&mut h, "brush-selection-more-button")?;
    click_model(
        &mut h,
        ModelPoint {
            x: joint.x + 4.,
            ..joint
        },
    )?;
    save(&h.capture()?, directory, "glue-02-separated.png")?;
    assert_eq!(
        h.view
            .read_with(&h.cx, |view, _| view.editor.overlay().glue_weights[0]
                .weights),
        [0., 0.5]
    );

    click(&mut h, "glue-mode-reattach")?;
    click_model(
        &mut h,
        ModelPoint {
            x: joint.x + 2.,
            ..joint
        },
    )?;
    save(&h.capture()?, directory, "glue-03-reattached.png")?;
    assert_eq!(
        h.view
            .read_with(&h.cx, |view, _| view.editor.overlay().glue_weights[0]
                .weights),
        [0., 1.]
    );

    update(&mut h, |view, _, cx| -> anyhow::Result<()> {
        view.editor.execute(EditorCommand::SetObjectsLocked {
            objects: vec![b],
            locked: true,
        })?;
        view.redraw(cx);
        Ok(())
    })??;
    save(&h.capture()?, directory, "glue-04-locked.png")?;
    assert!(
        h.view
            .read_with(&h.cx, |view, _| view.editor.overlay().glue_labels[0].locked)
    );
    Ok(())
}

fn weight_selection(directory: &std::path::Path) -> anyhow::Result<()> {
    let mut h = Harness::with_size(1100., 790.)?;
    let mut session = fixture::create(
        h._assets.path(),
        include_bytes!("../../../zipzi-editor-core/tests/fixtures/alpha-asymmetric.png"),
    )?;
    let targets = [fixture::id(30), fixture::id(31)];
    crate::test_fixture::publish_fixture(&mut session, "Two Glues", |edit| {
        for (index, (a, b)) in [(1, 0), (2, 3)].into_iter().enumerate() {
            crate::test_fixture::insert_glue(
                edit,
                zipzi_core::Glue {
                    id: targets[index].clone(),
                    name: format!("Glue {}", index + 1),
                    mesh_a_id: fixture::id(2).into(),
                    mesh_b_id: fixture::id(4).into(),
                    intensity: 1.,
                    pairs: vec![zipzi_core::GlueVertexPair {
                        id: fixture::id(32 + index as u128),
                        vertex_a: Some(a),
                        vertex_b: Some(b),
                        weight_a: 0.5,
                        weight_b: 0.5,
                    }],
                    ..Default::default()
                },
            )?;
        }
        Ok(())
    })?;
    let path = h._assets.path().join("glue-weight-selection.zipzi");
    crate::test_fixture::save_fixture(&mut session, &path)?;
    let mut document = Document::open(&path)?;
    document.select_objects(vec![])?;
    update(&mut h, |view, window, cx| {
        view.replace_editor(Editor::from_document(document), cx);
        view.refresh_document(window, cx);
    })?;
    h.capture()?;
    let click = |h: &mut Harness, id: &str| {
        h.cx.update_window(h.window, |_, window, cx| window.click(id.to_owned(), cx))
    };
    click(&mut h, "tool_glue")?;
    save(&h.capture()?, directory, "glue-weight-choose.png")?;
    h.cx.update_window(h.window, |_, window, _| {
        assert!(window.try_find("brush-selection-controls").is_none());
        for id in &targets {
            assert!(window.find(format!("glue-label-{id}")).visible());
        }
    })?;
    for (index, id) in targets.iter().enumerate() {
        let history = h
            .view
            .read_with(&h.cx, |v, _| v.editor.document().unwrap().history_lengths());
        click(&mut h, &format!("glue-label-{id}"))?;
        save(
            &h.capture()?,
            directory,
            &format!("glue-weight-target-{}.png", index + 1),
        )?;
        h.view.read_with(&h.cx, |view, _| {
            assert!(view.editor.glue_editing_weights());
            assert_eq!(
                view.editor
                    .document()
                    .unwrap()
                    .context()
                    .selection
                    .as_ref()
                    .unwrap()
                    .id,
                *id
            );
            assert_eq!(view.editor.overlay().glue_weight_meshes.len(), 2);
            assert_eq!(view.editor.document().unwrap().history_lengths(), history);
        });
        if index != 0 {
            continue;
        }
        let original = h
            .view
            .read_with(&h.cx, |view, _| view.editor.overlay().glue_weight_meshes);
        update(&mut h, |view, _, cx| -> anyhow::Result<()> {
            view.editor.execute(EditorCommand::SetGlueBrushSettings(
                BrushSelectionSettings {
                    weight: 1.,
                    ..Default::default()
                },
            ))?;
            view.redraw(cx);
            Ok(())
        })??;
        let joint = h.view.read_with(&h.cx, |view, _| {
            let frame = &view.modeling_frame().unwrap().canvas;
            frame
                .transform
                .to_model(view.editor.overlay().glue_weights[0].a)
        });
        click_model(&mut h, joint)?;
        save(&h.capture()?, directory, "glue-weight-mesh-painted.png")?;
        h.view.read_with(&h.cx, |view, _| {
            let overlay = view.editor.overlay();
            assert_eq!(overlay.glue_weights[0].weights, [0., 1.]);
            assert_ne!(overlay.glue_weight_meshes, original);
        });
        update(&mut h, |view, _, cx| -> anyhow::Result<()> {
            view.editor.execute(EditorCommand::Undo)?;
            view.redraw(cx);
            Ok(())
        })??;
        h.capture()?;
        assert_eq!(
            h.view
                .read_with(&h.cx, |view, _| view.editor.overlay().glue_weight_meshes),
            original
        );
        update(&mut h, |view, _, cx| -> anyhow::Result<()> {
            view.editor.execute(EditorCommand::SetGlueBrushSettings(
                BrushSelectionSettings {
                    color: [1., 0., 0., 0.],
                    ..Default::default()
                },
            ))?;
            view.redraw(cx);
            Ok(())
        })??;
        save(&h.capture()?, directory, "glue-weight-color-hidden.png")?;
        update(&mut h, |view, _, cx| -> anyhow::Result<()> {
            view.editor.execute(EditorCommand::SetGlueBrushSettings(
                BrushSelectionSettings::default(),
            ))?;
            view.redraw(cx);
            Ok(())
        })??;
        h.capture()?;
    }
    crate::i18n::activate(Some("zh-CN"));
    h.cx.update(|cx| crate::theme::change(crate::theme::ColorTheme::Light, cx));
    update(&mut h, |view, _, cx| view.redraw(cx))?;
    save(&h.capture()?, directory, "glue-weight-mesh-zh-CN-light.png")?;
    weight_toolbar_appearances(&mut h, directory)?;
    click(&mut h, "tool_select")?;
    h.capture()?;
    assert!(h.view.read_with(&h.cx, |view, _| {
        view.editor.overlay().glue_weight_meshes.is_empty()
    }));
    crate::i18n::activate(Some("en"));
    Ok(())
}

fn weight_toolbar_appearances(h: &mut Harness, directory: &std::path::Path) -> anyhow::Result<()> {
    use crate::{
        settings::{Settings, UiScale},
        theme::{self, ColorTheme},
    };
    use gpui_kit::BorrowAppContext as _;
    let (original_scale, original_theme) =
        h.cx.update(|cx| (cx.global::<Settings>().ui_scale, theme::current(cx)));
    let original = h
        .view
        .read_with(&h.cx, |view, _| view.editor.glue_brush_settings());
    update(h, |view, _, cx| -> anyhow::Result<()> {
        view.editor.execute(EditorCommand::SetGlueBrushSettings(
            BrushSelectionSettings {
                diameter: BrushSelectionSettings::MAX_DIAMETER,
                weight: 1.,
                ..original
            },
        ))?;
        view.redraw(cx);
        Ok(())
    })??;
    for (theme, name) in [(ColorTheme::Dark, "dark"), (ColorTheme::Light, "light")] {
        for scale in UiScale::ALL {
            h.cx.update(|cx| {
                cx.update_global::<Settings, _>(|settings, _| settings.set_ui_scale(scale))
                    .unwrap();
                theme::change(theme, cx);
            });
            update(h, |view, _, cx| view.redraw(cx))?;
            let frame = h.capture()?;
            let (bar, device_scale) = h.cx.update_window(h.window, |_, window, _| {
                let bar = window.find("brush-selection-settings").bounds();
                let viewport = window.find("canvas-body").bounds();
                let primary = window.find("canvas-toolbar").bounds();
                let button = window.find("glue-tab-points").bounds();
                assert!(bar.left() >= primary.right() && bar.right() <= viewport.right());
                assert!(
                    bar.size.height < button.size.height * 1.5,
                    "weight toolbar must be one row"
                );
                for id in ["glue-active-target", "glue-brush-restriction"] {
                    assert!(window.try_find(id).is_none());
                }
                assert!(window.try_find("glue-stitch-action").is_none());
                assert!(
                    window.find("tool_manual_mesh").bounds().bottom()
                        <= window.find("tool_glue").bounds().top()
                );
                let controls = [
                    "glue-tab-points",
                    "glue-mode-ratio",
                    "glue-mode-follow-b",
                    "glue-mode-reattach",
                    "glue-mode-release",
                    "brush-selection-weight",
                    "brush-selection-size",
                    "brush-selection-more-button",
                ]
                .map(|id| window.find(id).bounds());
                for (index, control) in controls.iter().enumerate() {
                    assert!((control.center().y - button.center().y).abs().as_f32() < 0.5);
                    assert!(control.left() >= bar.left() && control.right() <= bar.right());
                    assert!(
                        controls[index + 1..]
                            .iter()
                            .all(|other| control.right() <= other.left())
                    );
                }
                (bar, window.scale_factor())
            })?;
            let crop = image::imageops::crop_imm(
                &frame,
                (bar.left().as_f32() * device_scale).round() as u32,
                (bar.top().as_f32() * device_scale).round() as u32,
                (bar.size.width.as_f32() * device_scale).floor() as u32,
                (bar.size.height.as_f32() * device_scale).floor() as u32,
            )
            .to_image();
            save(
                &crop,
                directory,
                &format!(
                    "glue-weight-toolbar-{name}-{}.png",
                    scale.label().trim_end_matches('%')
                ),
            )?;
        }
    }
    h.cx.update(|cx| {
        cx.update_global::<Settings, _>(|settings, _| settings.set_ui_scale(original_scale))
            .unwrap();
        theme::change(original_theme, cx);
    });
    update(h, |view, _, cx| -> anyhow::Result<()> {
        view.editor
            .execute(EditorCommand::SetGlueBrushSettings(original))?;
        view.redraw(cx);
        Ok(())
    })??;
    h.capture()?;
    Ok(())
}

fn separated_mesh_edit(directory: &std::path::Path) -> anyhow::Result<()> {
    let mut h = Harness::with_size(1100., 790.)?;
    let mut session = fixture::create(
        h._assets.path(),
        include_bytes!("../../../zipzi-editor-core/tests/fixtures/alpha-asymmetric.png"),
    )?;
    let path = h._assets.path().join("glue-separated.zipzi");
    crate::test_fixture::save_fixture(&mut session, &path)?;
    let mut document = Document::open(&path)?;
    document.select_objects(
        [2, 4]
            .map(|n| ObjectRef::new(ObjectKind::Mesh, fixture::id(n)))
            .into(),
    )?;
    update(&mut h, |view, window, cx| {
        view.replace_editor(Editor::from_document(document), cx);
        view.refresh_document(window, cx);
    })?;
    h.capture()?;
    let click = |h: &mut Harness, id: &str| {
        h.cx.update_window(h.window, |_, window, cx| window.click(id.to_owned(), cx))
    };
    let a = ModelPoint { x: 20., y: 20. };
    let b = ModelPoint { x: 140., y: 140. };
    click(&mut h, "tool_manual_mesh")?;
    h.capture()?;
    click_model(&mut h, a)?;
    gesture_model(
        &mut h,
        b,
        b,
        gpui_kit::Modifiers {
            shift: true,
            ..Default::default()
        },
    )?;
    h.capture()?;
    click(&mut h, "mesh-glue-bind")?;
    save(&h.capture()?, directory, "glue-separated-edit-bound.png")?;
    let assert_points = |h: &Harness, expected: [ModelPoint; 2]| {
        h.view.read_with(&h.cx, |view, _| {
            let canvas = &view.modeling_frame().unwrap().canvas;
            for (id, expected) in [2, 4].into_iter().zip(expected) {
                let p = canvas.geometry.mesh(&fixture::id(id)).unwrap().vertices[0].1;
                assert!(
                    (p.x - expected.x).hypot(p.y - expected.y) < 1e-4,
                    "{p:?} vs {expected:?}"
                );
                let drawable = canvas
                    .geometry
                    .source
                    .viewport_frame
                    .drawables
                    .iter()
                    .find(|d| d.id == fixture::id(id))
                    .unwrap();
                assert_eq!(
                    (drawable.positions[0].x, drawable.positions[0].y),
                    (p.x, p.y)
                );
            }
            if view.editor.manual_active() {
                let overlay = view.editor.overlay();
                let joint = overlay
                    .glues
                    .iter()
                    .flat_map(|g| &g.points)
                    .find(|p| p.state == zipzi_editor_core::GluePointState::Binding)
                    .unwrap()
                    .position;
                for style in &overlay.mesh_styles {
                    let mesh = canvas.geometry.mesh(&style.object.id).unwrap();
                    let lines: Vec<_> = overlay.lines[style.lines.clone()]
                        .iter()
                        .chain(&overlay.automatic_lines[style.automatic_lines.clone()])
                        .copied()
                        .collect();
                    assert_eq!(lines.len(), mesh.edges.len());
                    let project = |slot: usize| {
                        if slot == 0 {
                            joint
                        } else {
                            canvas.transform.to_viewport(mesh.vertices[slot].1)
                        }
                    };
                    let close =
                        |a: zipzi_editor_core::ViewportPoint,
                         b: zipzi_editor_core::ViewportPoint| {
                            (a.x - b.x).hypot(a.y - b.y) < 1e-4
                        };
                    for &(a, b) in mesh.edges.iter() {
                        let (a, b) = (project(a), project(b));
                        assert!(lines.iter().any(|&(p, q)| {
                            (close(a, p) && close(b, q)) || (close(a, q) && close(b, p))
                        }));
                    }
                }
            }
        });
    };
    assert_points(&h, [a, b]);
    h.view.read_with(&h.cx, |view, _| {
        let overlay = view.editor.overlay();
        assert_eq!(overlay.vertices.len(), 6);
        assert_eq!(
            overlay.glues.iter().map(|g| g.points.len()).sum::<usize>(),
            1
        );
        assert!(overlay.glues.iter().all(|g| g.pairs.is_empty()));
    });
    toolbar_appearances(&mut h, directory)?;
    click(&mut h, "mesh-glue-unbind")?;
    save(&h.capture()?, directory, "glue-unbound-vertices.png")?;
    h.view.read_with(&h.cx, |view, _| {
        assert_eq!(view.editor.overlay().vertices.len(), 8)
    });
    update(&mut h, |view, _, cx| -> anyhow::Result<()> {
        view.editor.execute(EditorCommand::Undo)?;
        view.redraw(cx);
        Ok(())
    })??;
    h.capture()?;

    for locale in ["zh-CN", "ja"] {
        crate::i18n::activate(Some(locale));
        update(&mut h, |view, _, cx| view.redraw(cx))?;
        save(
            &h.capture()?,
            directory,
            &format!("glue-separated-edit-{locale}.png"),
        )?;
    }
    crate::i18n::activate(Some("en"));
    let center = ModelPoint { x: 80., y: 80. };
    gesture_model(
        &mut h,
        center,
        ModelPoint { x: 84., y: 84. },
        Default::default(),
    )?;
    let moved = ModelPoint { x: 24., y: 24. };
    save(&h.capture()?, directory, "glue-separated-edit-drag.png")?;
    assert_points(&h, [moved, ModelPoint { x: 144., y: 144. }]);
    click(&mut h, "mesh-glue-weights")?;
    save(&h.capture()?, directory, "glue-separated-final.png")?;
    assert_points(&h, [ModelPoint { x: 84., y: 84. }; 2]);
    click(&mut h, "glue-tab-points")?;
    save(&h.capture()?, directory, "glue-separated-reopened.png")?;
    assert_points(&h, [moved, ModelPoint { x: 144., y: 144. }]);
    let center = ModelPoint { x: 84., y: 84. };
    gesture_model(
        &mut h,
        center,
        center,
        gpui_kit::Modifiers {
            alt: true,
            ..Default::default()
        },
    )?;
    click_model(&mut h, center)?;
    save(
        &h.capture()?,
        directory,
        "glue-binding-control-selected.png",
    )?;
    h.view.read_with(&h.cx, |view, _| {
        assert!(
            view.editor
                .overlay()
                .glues
                .iter()
                .flat_map(|g| &g.points)
                .any(|p| p.state == zipzi_editor_core::GluePointState::Binding && p.selected)
        );
    });
    let target = ModelPoint { x: 88., y: 88. };
    gesture_model(&mut h, center, target, Default::default())?;
    save(&h.capture()?, directory, "glue-binding-control-drag.png")?;
    assert_points(
        &h,
        [
            ModelPoint { x: 28., y: 28. },
            ModelPoint { x: 148., y: 148. },
        ],
    );
    crate::i18n::activate(Some("zh-CN"));
    update(&mut h, |view, _, cx| view.redraw(cx))?;
    save(&h.capture()?, directory, "glue-binding-control-zh-CN.png")?;
    crate::i18n::activate(Some("en"));
    click(&mut h, "mesh-glue-weights")?;
    save(&h.capture()?, directory, "glue-binding-control-final.png")?;
    assert_points(&h, [target; 2]);
    Ok(())
}

fn toolbar_appearances(h: &mut Harness, directory: &std::path::Path) -> anyhow::Result<()> {
    use crate::{
        settings::{Settings, UiScale},
        theme::{self, ColorTheme},
    };
    use gpui_kit::BorrowAppContext as _;
    let (original_scale, original_theme) =
        h.cx.update(|cx| (cx.global::<Settings>().ui_scale, theme::current(cx)));
    crate::i18n::activate(Some("zh-CN"));
    for (theme, name) in [(ColorTheme::Dark, "dark"), (ColorTheme::Light, "light")] {
        for scale in UiScale::ALL {
            h.cx.update(|cx| {
                cx.update_global::<Settings, _>(|settings, _| settings.set_ui_scale(scale))
                    .unwrap();
                theme::change(theme, cx);
            });
            update(h, |view, _, cx| view.redraw(cx))?;
            let frame = h.capture()?;
            let (bar, device_scale) = h.cx.update_window(h.window, |_, window, _| {
                assert!(window.try_find("mesh-glue-target").is_none());
                let mesh = window.find("tool_manual_source").bounds();
                let bar = window.find("mesh-edit-actions").bounds();
                for id in [
                    "mesh-glue-add-point",
                    "mesh-glue-bind",
                    "mesh-glue-unbind",
                    "mesh-glue-weights",
                ] {
                    let button = window.find(id).bounds();
                    assert!((mesh.top() - button.top()).abs().as_f32() < 0.5);
                    assert_eq!(mesh.size, button.size);
                    assert!(button.right() <= bar.right());
                }
                assert!(bar.size.height < mesh.size.height * 1.5);
                (bar, window.scale_factor())
            })?;
            let crop = image::imageops::crop_imm(
                &frame,
                (bar.left().as_f32() * device_scale).round() as u32,
                (bar.top().as_f32() * device_scale).round() as u32,
                (bar.size.width.as_f32() * device_scale).floor() as u32,
                (bar.size.height.as_f32() * device_scale).floor() as u32,
            )
            .to_image();
            save(
                &crop,
                directory,
                &format!(
                    "glue-toolbar-{name}-{}.png",
                    scale.label().trim_end_matches('%')
                ),
            )?;
        }
    }
    h.cx.update(|cx| {
        cx.update_global::<Settings, _>(|settings, _| settings.set_ui_scale(original_scale))
            .unwrap();
        theme::change(original_theme, cx);
    });
    crate::i18n::activate(Some("en"));
    update(h, |view, _, cx| view.redraw(cx))?;
    h.capture()?;
    Ok(())
}

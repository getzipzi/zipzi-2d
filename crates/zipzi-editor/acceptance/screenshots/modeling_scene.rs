use super::super::{fixture, harness::Harness};
use super::common::save;
use crate::{i18n, theme};
use gpui_kit::{point, px, test::TestWindowExt};
use zipzi_editor_core::Editor;

pub(crate) fn empty_titlebar_screenshots(directory: &std::path::Path) -> anyhow::Result<()> {
    let mut empty = Harness::new()?;
    empty.cx.update_window(empty.window, |_, window, cx| {
        empty.view.update(cx, |view, cx| {
            view.run_command(zipzi_editor_core::CommandId::Close, window, cx);
        });
    })?;
    save(&empty.capture()?, directory, "titlebar-empty.png")?;
    empty.cx.update_window(empty.window, |_, window, cx| {
        window.click("project-select", cx);
    })?;
    save(&empty.capture()?, directory, "titlebar-project-menu.png")?;
    Ok(())
}

pub(crate) fn modeling_and_parameters_screenshots(
    harness: &mut Harness,
    directory: &std::path::Path,
) -> anyhow::Result<()> {
    let baseline = harness.capture()?;
    anyhow::ensure!(
        baseline.dimensions() == (2200, 1480),
        "Unexpected screenshot size"
    );
    anyhow::ensure!(
        baseline == harness.capture()?,
        "Unchanged editor must render identical pixels"
    );
    save(&baseline, directory, "editor.png")?;
    let baseline_canvas = harness.canvas_image(&baseline)?;

    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        anyhow::ensure!(
            window.try_find("animation-timeline").is_some(),
            "Animation transport is missing"
        );
        anyhow::ensure!(
            window.try_find("tool_glue").is_none(),
            "Animation must hide modeling tools"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    let animation = harness.capture()?;
    save(&animation, directory, "workspace-animation.png")?;
    anyhow::ensure!(
        animation != baseline,
        "Workspace switch must change the visible layout"
    );
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-modeling", cx);
    })?;
    let restored = harness.capture()?;
    anyhow::ensure!(
        harness.canvas_image(&restored)? == baseline_canvas,
        "Returning to Modeling must restore its canvas"
    );

    let mesh = harness
        .view
        .read_with(&harness.cx, |view, _| view.editor.layers()[0].id.clone());
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("canvas-body", cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.selected_layer_id(), Some(mesh.as_str()));
        });
    })?;
    let selected = harness.capture()?;
    let selected_canvas = harness.canvas_image(&selected)?;
    let marker = harness.view.read_with(&mut harness.cx, |view, _| {
        let position = view.editor.overlay().vertices[0].position;
        let scale = view
            .modeling_frame()
            .as_ref()
            .unwrap()
            .canvas
            .transform
            .metrics
            .scale_factor;
        (
            (position.x * scale).round() as u32,
            (position.y * scale).round() as u32,
        )
    });
    anyhow::ensure!(
        image::imageops::crop_imm(&selected_canvas, marker.0 - 8, marker.1 - 8, 16, 16).to_image()
            != image::imageops::crop_imm(&baseline_canvas, marker.0 - 8, marker.1 - 8, 16, 16)
                .to_image(),
        "Selecting the mesh must paint a vertex marker at its canvas position"
    );
    save(&selected, directory, "editor-selected.png")?;

    harness.cx.update_window(harness.window, |_, window, cx| {
        window.hover(format!("parts-item-Mesh:{mesh}"), cx);
        window
            .within("parts-tree-panel")
            .click(format!("parts-lock-{mesh}"), cx);
        harness.view.read_with(cx, |view, _| {
            assert!(view.editor.overlay().vertices.is_empty());
            assert!(
                view.editor
                    .document()
                    .unwrap()
                    .object_state_index()
                    .is_locked(&zipzi_editor_core::ObjectRef::new(
                        zipzi_editor_core::ObjectKind::Mesh,
                        &mesh
                    ))
            );
        });
    })?;
    let locked = harness.capture()?;
    let locked_canvas = harness.canvas_image(&locked)?;
    anyhow::ensure!(
        image::imageops::crop_imm(&locked_canvas, marker.0 - 8, marker.1 - 8, 16, 16).to_image()
            != image::imageops::crop_imm(&selected_canvas, marker.0 - 8, marker.1 - 8, 16, 16)
                .to_image(),
        "Locking must remove the editable vertex marker"
    );
    let (w, h) = locked_canvas.dimensions();
    anyhow::ensure!(
        image::imageops::crop_imm(&locked_canvas, w / 2 - 10, h / 2 - 10, 20, 20).to_image()
            == image::imageops::crop_imm(&selected_canvas, w / 2 - 10, h / 2 - 10, 20, 20)
                .to_image(),
        "Locking must preserve the rendered artwork"
    );
    save(&locked, directory, "editor-locked.png")?;

    harness.cx.update_window(harness.window, |_, window, cx| {
        window
            .within("parts-tree-panel")
            .click(format!("parts-vis-{mesh}"), cx);
    })?;
    let hidden = harness.capture()?;
    let hidden_canvas = harness.canvas_image(&hidden)?;
    let changed = hidden_canvas
        .pixels()
        .zip(baseline_canvas.pixels())
        .filter(|(a, b)| a != b)
        .count();
    anyhow::ensure!(
        // The diamond occupies over a quarter of its 256x256 canvas at actual pixel size.
        changed > 256 * 256 / 4,
        "Hiding the model must remove its filled pixels, not just the overlay"
    );
    save(&hidden, directory, "editor-hidden.png")?;

    let (document, assets) = fixture::sample_document_with_parameter_groups()?;
    let sliding_parameter = document.parameters()[0].id.clone();
    harness._assets = assets;
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor = Editor::from_document(document);
            let dock = view.dock.as_ref().unwrap();
            // Give the grouped list a full-height, standard-width panel for visual QA.
            use gpui_kit::component::dock::{DockLayout, panel_handle};
            dock.area.update(cx, |area, cx| {
                area.set_center(
                    DockLayout::h_split()
                        .child(
                            DockLayout::tabs().panel_view(dock.viewport_panel(), cx),
                            None,
                        )
                        .child(
                            DockLayout::tabs()
                                .panel_view(panel_handle(dock.parameters.clone()), cx),
                            Some(px(360.)),
                        ),
                    window,
                    cx,
                );
            });
            view.redraw(cx);
        });
    })?;
    let grouped = harness.capture()?;
    save(
        &harness.element_image(&grouped, "parameters-panel")?,
        directory,
        "parameters-grouped.png",
    )?;
    let number_origin = harness.cx.update_window(harness.window, |_, window, cx| {
        use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent};
        let position = window
            .find(format!("parameter-value-{sliding_parameter}"))
            .bounds()
            .center();
        window.dispatch_event(
            MouseMoveEvent {
                position,
                pressed_button: None,
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        position
    })?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        use gpui_kit::{InputEvent as _, Modifiers, MouseButton, MouseMoveEvent};
        window.dispatch_event(
            MouseMoveEvent {
                position: number_origin + point(px(16.), px(0.)),
                pressed_button: Some(MouseButton::Left),
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
            }
            .to_platform_input(),
            cx,
        );
    })?;
    let sliding = harness.capture()?;
    let row_id = format!("parameter-row-{sliding_parameter}");
    let sliding_row = harness.element_image(&sliding, &row_id)?;
    anyhow::ensure!(
        sliding_row != harness.element_image(&grouped, &row_id)?,
        "Sliding must display the live value and interaction border"
    );
    save(&sliding_row, directory, "parameter-number-sliding.png")?;
    harness
        .cx
        .update_window(harness.window, |_, window, cx| window.press("escape", cx))?;
    let face = uuid::Uuid::from_u128(101).to_string();
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-group-toggle-{face}"), cx);
        assert!(
            !window
                .find(format!("parameter-group-{face}"))
                .expanded()
                .unwrap()
        );
        assert!(
            window
                .try_find(format!("parameter-name-{}", uuid::Uuid::from_u128(3)))
                .is_none()
        );
    })?;
    let collapsed = harness.capture()?;
    let collapsed_panel = harness.element_image(&collapsed, "parameters-panel")?;
    anyhow::ensure!(
        collapsed != grouped,
        "Collapsing a parameter group must change its rendered rows"
    );
    save(&collapsed_panel, directory, "parameters-collapsed.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-group-toggle-{face}"), cx);
        theme::change(gpui_kit::component::ThemeMode::Light, cx);
    })?;
    let light = harness.capture()?;
    save(
        &harness.element_image(&light, "parameters-panel")?,
        directory,
        "parameters-grouped-light.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        cx.set_reduce_motion(true);
        theme::change(gpui_kit::component::ThemeMode::Dark, cx);
        window.click("new-parameter", cx);
    })?;
    let popup = harness.capture()?;
    harness.cx.update_window(harness.window, |_, window, _| {
        assert!(window.try_find("new-parameter-advanced").is_none());
        for field in [
            "property-input-decimal_places",
            "property-select-kind",
            "property-select-group_id",
            "property-toggle-repeat",
        ] {
            assert!(
                window.find(field).visible(),
                "{field} must be visible on open"
            );
        }
    })?;
    save(
        &harness.element_image(&popup, "properties-panel")?,
        directory,
        "new-parameter.png",
    )?;
    let created_parameter = harness.view.read_with(&harness.cx, |view, _| {
        view.editor
            .document()
            .unwrap()
            .context()
            .parameter_focus
            .as_ref()
            .unwrap()
            .id
            .clone()
    });
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("property-select-group_id", cx);
        window.input("Body", cx);
    })?;
    save(&harness.capture()?, directory, "parameter-group-search.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
    })?;
    harness.capture()?;
    anyhow::ensure!(
        harness.view.read_with(&harness.cx, |view, _| {
            view.editor
                .document()
                .unwrap()
                .parameter_draft(&created_parameter)
                .unwrap()
                .group_id
                .as_deref()
                == Some(uuid::Uuid::from_u128(103).to_string().as_str())
        }),
        "Parameter group search did not commit its selection"
    );

    harness.cx.update_window(harness.window, |_, window, cx| {
        window.press("escape", cx);
        i18n::activate(Some("zh-CN"));
        theme::change(gpui_kit::component::ThemeMode::Light, cx);
        window.click("new-parameter", cx);
    })?;
    let popup_zh = harness.capture()?;
    save(
        &harness.element_image(&popup_zh, "properties-panel")?,
        directory,
        "new-parameter-zh-light.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.press("escape", cx);
    })?;
    for (action, filename) in [
        (
            crate::panels::parameters::ParameterAction::Edit(uuid::Uuid::from_u128(3).to_string()),
            "edit-parameter-zh-light.png",
        ),
        (
            crate::panels::parameters::ParameterAction::CreateGroup,
            "new-parameter-group-zh-light.png",
        ),
        (
            crate::panels::parameters::ParameterAction::DeleteGroup(
                uuid::Uuid::from_u128(101).to_string(),
            ),
            "delete-parameter-group-zh-light.png",
        ),
    ] {
        harness.cx.update_window(harness.window, |_, window, cx| {
            let panel = harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .parameters
                .clone();
            panel.update(cx, |panel, cx| panel.open_management(action, window, cx));
        })?;
        let popup = harness.capture()?;
        save(
            &harness.element_image(&popup, "properties-panel")?,
            directory,
            filename,
        )?;
        harness
            .cx
            .update_window(harness.window, |_, window, cx| window.press("escape", cx))?;
    }
    Ok(())
}

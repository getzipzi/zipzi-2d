use super::super::{fixture, harness::Harness};
use super::common::save;
use crate::{i18n, theme};
use gpui_kit::{
    InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, point,
    px, test::TestWindowExt,
};
use zipzi_editor_core::Editor;

fn pan_curve(harness: &mut Harness, dy: f32) -> anyhow::Result<()> {
    harness.cx.update_window(harness.window, |_, window, cx| {
        let origin = window.find("timeline-graph").bounds().center();
        let end = origin + point(px(0.), px(dy));
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Middle,
                position: origin,
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
                pressed_button: Some(MouseButton::Middle),
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Middle,
                position: end,
                modifiers: Modifiers::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
    })?;
    Ok(())
}

pub(crate) fn timeline_screenshots(directory: &std::path::Path) -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let (document, assets) = fixture::animation_document()?;
    harness._assets = assets;
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.replace_editor(Editor::from_document(document), cx);
            view.redraw(cx);
        });
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(zipzi_editor_core::CommandId::FitView, window, cx);
            let projection = view.editor.timeline_projection().unwrap();
            let row = projection
                .rows
                .iter()
                .find(|row| row.track_id.is_some())
                .unwrap();
            let key = row.keys[0].clone();
            let row = row.id.clone();
            let d = view.editor.active_id().unwrap();
            view.editor
                .update_timeline_view(d, |state| {
                    state.channels.insert(row);
                    state.keys.insert(key);
                })
                .unwrap();
            view.redraw(cx);
        });
    })?;
    let overview = harness.capture()?;
    save(&overview, directory, "timeline-overview.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("timeline-playback-speed", cx);
        window.render_frame(cx);
        for _ in 0..3 {
            window.press("up", cx);
        }
    })?;
    save(
        &harness.capture()?,
        directory,
        "timeline-playback-speed-options.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
    })?;
    save(&harness.capture()?, directory, "timeline-quarter-speed.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        let bounds = window.find("editor-bottom-dock").bounds();
        let scale = window.scale_factor();
        let background = gpui_kit::Rgba::from(crate::ui::compact::workspace(cx));
        let expected = [background.r, background.g, background.b]
            .map(|channel| (channel * 255.).round() as u8);
        for fraction in [0.25, 0.5, 0.75] {
            let x = ((bounds.left().as_f32() + bounds.size.width.as_f32() * fraction) * scale)
                .floor() as u32;
            let y = (bounds.top().as_f32() * scale).ceil() as u32;
            let pixel = overview.get_pixel(x, y);
            anyhow::ensure!(
                pixel.0[..3]
                    .iter()
                    .zip(expected)
                    .all(|(channel, expected)| channel.abs_diff(expected) <= 3),
                "Extra line above Timeline at ({x}, {y}): {:?}",
                pixel.0
            );
        }
        Ok::<_, anyhow::Error>(())
    })??;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("property-select-interpolation", cx);
        window.render_frame(cx);
        window.press("down", cx);
        window.press("enter", cx);
    })?;
    harness.capture()?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        if window.try_find("timeline-view").is_some() {
            window.click("timeline-view", cx);
        } else {
            window.click("timeline-more", cx);
            window.render_frame(cx);
            window.click("timeline-menu-view", cx);
        }
        window.render_frame(cx);
    })?;
    let curves = harness.capture()?;
    save(&curves, directory, "timeline-curves.png")?;
    pan_curve(&mut harness, 8.)?;
    save(&harness.capture()?, directory, "timeline-curves-panned.png")?;
    pan_curve(&mut harness, -8.)?;
    harness.capture()?;
    anyhow::ensure!(
        overview != curves,
        "Timeline mode did not alter the rendered scene"
    );
    let canvas_before = harness.canvas_image(&curves)?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("property-input-selection_value", cx);
        window.press("cmd-a", cx);
        window.input("0.6", cx);
    })?;
    let candidate = harness.capture()?;
    save(&candidate, directory, "timeline-candidate.png")?;
    anyhow::ensure!(
        harness.canvas_image(&candidate)? != canvas_before,
        "Candidate key edit did not alter the rendered image"
    );
    harness.cx.update_window(harness.window, |_, window, cx| {
        let view = harness.view.read(cx);
        anyhow::ensure!(
            view.editor.timeline_edit().is_some(),
            "Candidate was committed during preview"
        );
        anyhow::ensure!(
            view.editor.document().unwrap().history_lengths() == (1, 0),
            "Preview added history"
        );
        window.press("escape", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    let restored = harness.capture()?;
    save(&restored, directory, "timeline-cancel-restored.png")?;
    anyhow::ensure!(
        harness.canvas_image(&restored)? == canvas_before,
        "Cancel did not restore the canvas image"
    );
    harness.cx.update_window(harness.window, |_, _window, cx| {
        i18n::activate(Some("zh-CN"));
        theme::change(gpui_kit::component::ThemeMode::Light, cx);
        harness.view.update(cx, |view, cx| view.redraw(cx));
    })?;
    save(
        &harness.capture()?,
        directory,
        "timeline-curves-zh-light.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        use gpui_kit::component::dock::{DockPlacement, InsertTarget, PanelId};
        let dock = harness.view.read(cx).dock.as_ref().unwrap();
        let panel = PanelId::from(dock.timeline.entity_id());
        let info = PanelId::from(dock.properties.entity_id());
        let area = dock.area.clone();
        let node = area
            .read(cx)
            .layout(DockPlacement::Center)
            .unwrap()
            .find_panel_node(info)
            .unwrap();
        area.update(cx, |area, cx| {
            area.move_panel(
                panel,
                InsertTarget::Tabs {
                    node,
                    ix: None,
                    activate: true,
                },
                window,
                cx,
            )
        });
    })?;
    save(
        &harness.capture()?,
        directory,
        "timeline-narrow-zh-light.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        anyhow::ensure!(
            window.find("timeline-graph").bounds().size.width >= px(150.),
            "Narrow dock lost its editable graph"
        );
        anyhow::ensure!(
            window.find("timeline-graph").bounds().size.height >= px(50.),
            "Narrow dock chrome consumed the graph height"
        );
        window.click(("animation-motion", 0usize), cx);
        let dock = harness.view.read(cx).dock.as_ref().unwrap();
        let area = dock.area.clone();
        let properties = gpui_kit::component::dock::PanelId::from(dock.properties.entity_id());
        area.update(cx, |area, cx| area.select_panel(properties, window, cx));
        Ok::<_, anyhow::Error>(())
    })??;
    save(
        &harness.capture()?,
        directory,
        "timeline-narrow-properties-zh-light.png",
    )?;
    i18n::activate(Some("en"));
    curve_axis_screenshots(directory)?;
    Ok(())
}

fn curve_axis_screenshots(directory: &std::path::Path) -> anyhow::Result<()> {
    use zipzi_editor_core::{
        NewParameter, TimelineChannelId, TimelineCommand, TimelineDisplayMode,
    };
    let (mut document, assets) = fixture::animation_document()?;
    let parameter = document.parameters()[0].id.clone();
    let mut draft = document.parameter_draft(&parameter)?;
    draft.minimum = -30.;
    draft.maximum = 30.;
    document.update_parameter(&parameter, draft, document.parameter_model_revision())?;
    let second = document.create_parameter(NewParameter {
        name: "Wide range".into(),
        runtime_id: "ParamWide".into(),
        minimum: 0.,
        maximum: 100.,
        ..Default::default()
    })?;
    let mut harness = Harness::new()?;
    harness._assets = assets;
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.replace_editor(Editor::from_document(document), cx);
            view.redraw(cx);
        });
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        harness.view.update(cx, |view, cx| {
            let document = view.editor.active_id().unwrap();
            let projection = view.editor.timeline_projection().unwrap();
            let track = projection.tracks.values().next().unwrap();
            for (key, value) in [
                (track.keys[0].reference.clone(), -30.),
                (track.keys[1].reference.clone(), 30.),
            ] {
                let time = projection.key(&key).unwrap().time;
                view.editor
                    .execute_timeline(document, TimelineCommand::SetKey { key, time, value })
                    .unwrap();
            }
            for (time, value) in [(0., 0.), (1., 100.)] {
                view.editor.seek_timeline(document, time).unwrap();
                view.editor
                    .execute_timeline(
                        document,
                        TimelineCommand::WriteValue {
                            channel: TimelineChannelId::Parameter(second.clone()),
                            track_id: None,
                            selected_key: None,
                            value,
                        },
                    )
                    .unwrap();
            }
            let projection = view.editor.timeline_projection().unwrap();
            let row = projection
                .rows
                .iter()
                .find(|row| {
                    row.channel.as_ref() == Some(&TimelineChannelId::Parameter(parameter.clone()))
                        && row.track_id.is_some()
                })
                .unwrap()
                .id
                .clone();
            view.editor
                .update_timeline_view(document, |state| {
                    state.channels = [row].into_iter().collect();
                    state.mode = TimelineDisplayMode::Curves;
                    state.time_range = (0., 2.5);
                })
                .unwrap();
            view.redraw(cx);
        });
    })?;
    save(&harness.capture()?, directory, "timeline-axis-single.png")?;
    harness.cx.update_window(harness.window, |_, _, cx| {
        harness.view.update(cx, |view, cx| {
            let document = view.editor.active_id().unwrap();
            let projection = view.editor.timeline_projection().unwrap();
            view.editor
                .update_timeline_view(document, |state| {
                    state.channels = projection
                        .rows
                        .iter()
                        .filter(|row| row.track_id.is_some())
                        .map(|row| row.id.clone())
                        .collect();
                    state.curve_focus = None;
                })
                .unwrap();
            view.redraw(cx);
        });
    })?;
    save(&harness.capture()?, directory, "timeline-axis-multiple.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        let row = projection
            .rows
            .iter()
            .find(|row| {
                row.channel.as_ref() == Some(&TimelineChannelId::Parameter(parameter.clone()))
                    && row.track_id.is_some()
            })
            .unwrap();
        window.click(format!("timeline-row-{:?}", row.id), cx);
    })?;
    save(&harness.capture()?, directory, "timeline-axis-focused.png")?;
    pan_curve(&mut harness, 24.)?;
    save(
        &harness.capture()?,
        directory,
        "timeline-axis-focused-panned.png",
    )?;
    Ok(())
}

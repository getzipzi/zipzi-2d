//! Two real panels on the production CPU and Metal workers, with pixel isolation checks.
use super::{Harness, common::save};
use editor_core::{ViewportId, ViewportKind, ViewportPoint, WorkspaceKind};
use gpui_kit::{Bounds, Pixels, px, test::TestWindowExt};
use image::RgbaImage;
use std::{
    path::Path,
    time::{Duration, Instant},
};

fn settled(h: &mut Harness, ids: &[ViewportId]) -> anyhow::Result<RgbaImage> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        h.cx.run_until_parked();
        let ready = h.cx.update_window(h.window, |_, window, cx| {
            window.render_frame(cx);
            h.view.read_with(cx, |view, _| {
                ids.iter().all(|&id| {
                    let Some(binding) = view.editor.viewport_binding(id) else {
                        return false;
                    };
                    if view.viewport_error(id).is_some() {
                        return false;
                    }
                    match binding.kind {
                        ViewportKind::Modeling => {
                            view.modeling_frame_for(id).is_some_and(|frame| {
                                view.editor
                                    .accepts_viewport_preview(binding, frame.canvas.version.request)
                            })
                        }
                        ViewportKind::Animation => {
                            view.animation_frame_for(id).is_some_and(|frame| {
                                view.editor
                                    .can_display_animation_viewport(frame.state.viewport)
                                    && view
                                        .editor
                                        .viewport_state(binding.document, id)
                                        .is_some_and(|state| {
                                            state.revision == frame.state.viewport.viewport_revision
                                        })
                                    && view.editor.animation().is_some_and(|animation| {
                                        animation.candidate == frame.state.identity.candidate
                                            && animation.time == frame.state.snapshot.time
                                    })
                            })
                        }
                    }
                })
            })
        })?;
        if ready {
            return h.cx.capture_screenshot(h.window);
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "Dual viewport presentation timed out: {:?}",
            h.view.read_with(&h.cx, |view, _| ids
                .iter()
                .map(|id| (
                    id,
                    view.viewport_error(*id).cloned(),
                    view.editor.viewport_binding(*id)
                ))
                .collect::<Vec<_>>())
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}
fn changed_region(
    before: &RgbaImage,
    after: &RgbaImage,
    bounds: Bounds<Pixels>,
    scale: f32,
) -> usize {
    let left = (bounds.left().as_f32() * scale).ceil().max(0.) as u32;
    let right = (bounds.right().as_f32() * scale)
        .floor()
        .min(before.width() as f32) as u32;
    let top = (bounds.top().as_f32() * scale).ceil().max(0.) as u32;
    let bottom = (bounds.bottom().as_f32() * scale)
        .floor()
        .min(before.height() as f32) as u32;
    (top..bottom)
        .flat_map(|y| (left..right).map(move |x| (x, y)))
        .filter(|&(x, y)| before.get_pixel(x, y) != after.get_pixel(x, y))
        .count()
}

pub(super) fn viewport_screenshots(directory: &Path) -> anyhow::Result<()> {
    origin_crosshair_screenshots(directory)?;
    transparency_grid_screenshots(directory)?;
    for kind in [ViewportKind::Modeling, ViewportKind::Animation] {
        let mut h = Harness::new()?;
        if kind == ViewportKind::Animation {
            let (document, assets) = crate::test_fixture::animation_document()?;
            h._assets = assets;
            h.cx.update_window(h.window, |_, window, cx| {
                h.view.update(cx, |view, cx| {
                    view.editor = editor_core::Editor::from_document(document);
                    view.editor.set_workspace(WorkspaceKind::Animation).unwrap();
                    let selection = view.editor.document().unwrap().motion_resources()[0]
                        .selection
                        .clone();
                    view.editor
                        .select_motion(view.editor.active_id().unwrap(), selection)
                        .unwrap();
                    view.refresh_document(window, cx);
                })
            })?;
        }
        let ids = h.cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |view, cx| {
                view.install_viewport_scene(&[kind, kind], window, cx)
            })
        })?;
        let before = settled(&mut h, &ids)?;
        let (bounds, second_request) = h.view.read_with(&h.cx, |view, cx| {
            let bounds = ids
                .iter()
                .map(|&id| {
                    view.viewport_surface(id, cx)
                        .unwrap()
                        .read(cx)
                        .benchmark_bounds()
                        .unwrap()
                })
                .collect::<Vec<_>>();
            let second_request = match kind {
                ViewportKind::Modeling => {
                    view.modeling_frame_for(ids[1])
                        .unwrap()
                        .canvas
                        .version
                        .request
                }
                ViewportKind::Animation => {
                    view.animation_frame_for(ids[1])
                        .unwrap()
                        .state
                        .viewport
                        .request
                }
            };
            (bounds, second_request)
        });
        anyhow::ensure!(
            bounds[0].right() <= bounds[1].left() + px(1.),
            "Viewport surfaces overlap"
        );
        h.cx.update_window(h.window, |_, _, cx| {
            h.view.update(cx, |view, cx| {
                let binding = view.editor.viewport_binding(ids[0]).unwrap();
                view.editor
                    .pan_viewport(binding, ViewportPoint { x: 41., y: -23. })
                    .unwrap();
                view.redraw_canvas(cx);
            })
        })?;
        let after = settled(&mut h, &ids)?;
        let scale =
            h.cx.update_window(h.window, |_, window, _| window.scale_factor())?;
        save(&before, directory, &format!("viewport-{kind:?}-before.png"))?;
        save(&after, directory, &format!("viewport-{kind:?}-after.png"))?;
        println!(
            "Dual viewport {kind:?}: bounds={bounds:?}, changed=({}, {}), pixels={}x{}",
            changed_region(&before, &after, bounds[0], scale),
            changed_region(&before, &after, bounds[1], scale),
            before.width(),
            before.height()
        );
        anyhow::ensure!(
            changed_region(&before, &after, bounds[0], scale) > 100,
            "Navigation did not change the initiating viewport pixels"
        );
        anyhow::ensure!(
            changed_region(&before, &after, bounds[1], scale) == 0,
            "Navigation changed another viewport's pixels"
        );
        h.view.read_with(&h.cx, |view, _| {
            let current = match kind {
                ViewportKind::Modeling => {
                    view.modeling_frame_for(ids[1])
                        .unwrap()
                        .canvas
                        .version
                        .request
                }
                ViewportKind::Animation => {
                    view.animation_frame_for(ids[1])
                        .unwrap()
                        .state
                        .viewport
                        .request
                }
            };
            assert_eq!(
                current, second_request,
                "Unchanged viewport submitted another GPU request"
            );
        });
        save(
            &after,
            directory,
            match kind {
                ViewportKind::Modeling => "viewport-pair-modeling.png",
                ViewportKind::Animation => "viewport-pair-animation.png",
            },
        )?;
        // Continuous camera input on one owner must still deliver frames for its peer.
        for step in 0..12 {
            h.cx.update_window(h.window, |_, _, cx| {
                h.view.update(cx, |view, cx| {
                    for (&id, delta) in ids.iter().zip([1., -2.]) {
                        view.editor
                            .pan_viewport(
                                view.editor.viewport_binding(id).unwrap(),
                                ViewportPoint {
                                    x: delta,
                                    y: step as f32 * 0.1,
                                },
                            )
                            .unwrap();
                    }
                    view.redraw_canvas(cx);
                })
            })?;
            h.cx.run_until_parked();
        }
        settled(&mut h, &ids)?;
    }
    Ok(())
}

fn origin_crosshair_screenshots(directory: &Path) -> anyhow::Result<()> {
    let mut h = Harness::new()?;
    h.capture()?;
    h.cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            let frame = &view.modeling_frame().unwrap().canvas;
            let origin = frame
                .transform
                .to_viewport(editor_core::ModelPoint::default());
            view.editor
                .pan_viewport(
                    frame.version.binding,
                    ViewportPoint {
                        x: frame.transform.metrics.width / 2. - origin.x,
                        y: frame.transform.metrics.height / 2. - origin.y,
                    },
                )
                .unwrap();
            view.redraw_canvas(cx);
        });
        window.render_frame(cx);
    })?;
    let visible = h.capture()?;
    let (origin, request) = h.view.read_with(&h.cx, |view, _| {
        let frame = &view.modeling_frame().unwrap().canvas;
        (
            frame
                .transform
                .to_viewport(editor_core::ModelPoint::default()),
            frame.version.request,
        )
    });
    let (region, scale) = h.cx.update_window(h.window, |_, window, cx| {
        assert_eq!(window.find("btn_origin_crosshair").checked(), Some(true));
        let canvas = window.find("canvas-body").bounds();
        let region = Bounds::new(
            canvas.origin + gpui_kit::point(px(origin.x - 11.), px(origin.y - 11.)),
            gpui_kit::size(px(22.), px(22.)),
        );
        assert!(canvas.contains(&region.origin) && canvas.contains(&region.bottom_right()));
        window.click("btn_origin_crosshair", cx);
        (region, window.scale_factor())
    })?;
    let hidden = h.capture()?;
    anyhow::ensure!(
        changed_region(&visible, &hidden, region, scale) > 10,
        "Hiding the origin crosshair did not change its pixels"
    );
    h.cx.update_window(h.window, |_, window, cx| {
        assert_eq!(window.find("btn_origin_crosshair").checked(), Some(false));
        window.click("btn_origin_crosshair", cx);
    })?;
    let restored = h.capture()?;
    anyhow::ensure!(
        h.canvas_image(&visible)? == h.canvas_image(&restored)?,
        "Restoring the origin crosshair changed the canvas"
    );
    h.view.read_with(&h.cx, |view, _| {
        assert_eq!(
            view.modeling_frame().unwrap().canvas.version.request,
            request
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
    save(&visible, directory, "viewport-origin-visible.png")?;
    save(&hidden, directory, "viewport-origin-hidden.png")?;
    Ok(())
}

fn transparency_grid_screenshots(directory: &Path) -> anyhow::Result<()> {
    use gpui_kit::{component::ThemeMode, point};

    let mut h = Harness::new()?;
    for (mode, appearance) in [(ThemeMode::Dark, "dark"), (ThemeMode::Light, "light")] {
        h.cx.update(|cx| crate::theme::change(mode, cx));
        let before = h.capture()?;
        let before_canvas = h.canvas_image(&before)?;
        let request = h.view.read_with(&h.cx, |view, _| {
            view.modeling_frame().unwrap().canvas.version.request
        });
        h.cx.update_window(h.window, |_, window, cx| {
            let button = window.find("btn_transparency_grid").bounds();
            let header = window.find("canvas-zoom-toolbar").bounds();
            let zoom = window.find("canvas-zoom-controls").bounds();
            assert_eq!(button.left(), header.left());
            assert!(button.right() < zoom.left());
            window.click("btn_transparency_grid", cx);
        })?;
        let enabled = h.capture()?;
        let (bounds, scale) = h.cx.update_window(h.window, |_, window, _| {
            (window.find("canvas-body").bounds(), window.scale_factor())
        })?;
        let x = ((bounds.size.width.as_f32() / 12.).floor() - 4.) * 12. + 6.;
        let pixel = |dx: f32, dy: f32| {
            let p = bounds.origin + point(px(x + dx), px(66. + dy));
            *enabled.get_pixel((p.x.as_f32() * scale) as u32, (p.y.as_f32() * scale) as u32)
        };
        anyhow::ensure!(pixel(0., 0.) != pixel(12., 0.), "Missing checker squares");
        anyhow::ensure!(pixel(0., 0.) == pixel(24., 0.), "Wrong horizontal period");
        anyhow::ensure!(pixel(0., 0.) == pixel(12., 12.), "Wrong checker parity");
        h.view.read_with(&h.cx, |view, _| {
            assert_eq!(
                view.modeling_frame().unwrap().canvas.version.request,
                request
            );
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        });
        save(
            &enabled,
            directory,
            &format!("viewport-transparency-{appearance}.png"),
        )?;

        // The Animation viewport keeps its own default backdrop, and returning
        // to Modeling preserves this viewport's selected display option.
        h.cx.update_window(h.window, |_, window, cx| {
            window.click("workspace-animation", cx);
        })?;
        let animation = h.capture()?;
        let animation_canvas = h.canvas_image(&animation)?;
        let x = animation_canvas.width() - (54. * scale) as u32;
        let y = (66. * scale) as u32;
        anyhow::ensure!(
            animation_canvas.get_pixel(x, y)
                == animation_canvas.get_pixel(x + (12. * scale) as u32, y),
            "Modeling background toggle changed the Animation viewport"
        );
        h.cx.update_window(h.window, |_, window, cx| {
            window.click("workspace-modeling", cx);
        })?;
        let restored = h.capture()?;
        anyhow::ensure!(
            h.canvas_image(&restored)? == h.canvas_image(&enabled)?,
            "Workspace switch lost the viewport background"
        );
        h.cx.update_window(h.window, |_, window, cx| {
            window.click("btn_transparency_grid", cx);
        })?;
        let disabled = h.capture()?;
        anyhow::ensure!(
            h.canvas_image(&disabled)? == before_canvas,
            "Disabling the transparency grid did not restore the solid backdrop"
        );
    }
    Ok(())
}

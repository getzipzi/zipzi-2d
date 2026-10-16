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

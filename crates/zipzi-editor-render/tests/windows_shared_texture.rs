#![cfg(all(target_os = "windows", feature = "wgpu-backend"))]
use anyhow::{Context, Result};
use gpui::{
    Bounds, ContentMask, PaintSurface, Quad, ScaledPixels, Scene, SurfaceSource, point, size,
};
use std::sync::Arc;
use windows::Win32::{
    Foundation::HMODULE,
    Graphics::{Direct3D::*, Direct3D11::*},
};
use zipzi_editor_render::{
    D3d11Preview, WgpuSceneRenderer, WgpuTargetConfig, WindowsPresenter, windows_context,
};
use zipzi_render_wgpu::WgpuContext;

fn device() -> Result<ID3D11Device> {
    let mut device = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&[D3D_FEATURE_LEVEL_11_0]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            None,
        )?;
    }
    device.context("D3D11 hardware device")
}
fn bounds(x: f32, y: f32, w: f32, h: f32) -> Bounds<ScaledPixels> {
    Bounds::new(
        point(ScaledPixels(x), ScaledPixels(y)),
        size(ScaledPixels(w), ScaledPixels(h)),
    )
}
fn frame(
    context: &WgpuContext,
    presenter: &mut WindowsPresenter,
    extent: [u32; 2],
    color: [f64; 4],
) -> Result<Arc<gpui::DirectXSurface>> {
    let slot = presenter.acquire(extent)?.context("free slot")?;
    let texture = context
        .output_texture(WgpuTargetConfig {
            width: extent[0],
            height: extent[1],
            format: wgpu::TextureFormat::Rgba8Unorm,
        })
        .map_err(|e| anyhow::anyhow!("{}", e.message))?;
    let mut encoder = context.device().create_command_encoder(&Default::default());
    {
        let view = texture.create_view(&Default::default());
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: color[0],
                        g: color[1],
                        b: color[2],
                        a: color[3],
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    context.queue().submit([encoder.finish()]);
    presenter.publish(context, slot, texture)
}
fn scene(image: Arc<gpui::DirectXSurface>, opacity: f32) -> Scene {
    let mut scene = Scene::default();
    scene.insert_primitive(PaintSurface {
        order: 0,
        bounds: bounds(2., 2., 20., 20.),
        content_mask: ContentMask {
            bounds: bounds(4., 4., 16., 16.),
        },
        source: SurfaceSource::DirectX(image),
        opacity,
    });
    // A subsequent GPUI primitive verifies state is restored after surface draws.
    scene.insert_primitive(Quad {
        bounds: bounds(8., 8., 4., 4.),
        content_mask: ContentMask {
            bounds: bounds(0., 0., 32., 32.),
        },
        background: gpui::rgba(0x00ff00ff).into(),
        ..Default::default()
    });
    scene.finish();
    scene
}
fn close(actual: [u8; 4], expected: [u8; 4]) {
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
        "{actual:?} != {expected:?}"
    );
}

fn full_surface(image: Arc<gpui::DirectXSurface>, [width, height]: [u32; 2]) -> Scene {
    let bounds = bounds(0., 0., width as f32, height as f32);
    let mut scene = Scene::default();
    scene.insert_primitive(PaintSurface {
        order: 0,
        bounds,
        content_mask: ContentMask { bounds },
        source: SurfaceSource::DirectX(image),
        opacity: 1.,
    });
    scene.finish();
    scene
}

#[test]
fn fitting_large_previews_preserves_logical_coordinates_and_bounds_raster_bytes() -> Result<()> {
    use zipzi_editor_core::{ModelPoint, ViewportMetrics, ViewportPoint, ViewportTransform};
    for [width, height] in [
        [6000, 6000],
        [7680, 4320],
        [16384, 16384],
        [16384, 1],
        [1, 16384],
        [3840, 2160],
        [4096, 2048],
        [4097, 2048],
        [1, 1],
    ] {
        let target = WgpuTargetConfig {
            width,
            height,
            format: wgpu::TextureFormat::Rgba8Unorm,
        };
        let transform = ViewportTransform {
            scale: 1.75,
            offset: ViewportPoint { x: 120., y: 240. },
            metrics: ViewportMetrics {
                width: width as f32 / 2.,
                height: height as f32 / 2.,
                scale_factor: 2.,
            },
        };
        let (fitted, raster) = WindowsPresenter::fit_preview(target, transform)?;
        assert!(
            u64::from(fitted.width) * u64::from(fitted.height) * 4
                <= WindowsPresenter::MAX_FRAME_BYTES
        );
        assert!(fitted.width > 0 && fitted.height > 0);
        assert_eq!(
            raster.to_viewport(ModelPoint { x: 12., y: -34. }),
            transform.to_viewport(ModelPoint { x: 12., y: -34. })
        );
        assert_eq!(raster.metrics.width, transform.metrics.width);
        assert_eq!(raster.metrics.height, transform.metrics.height);
        let scale = raster.metrics.scale_factor / transform.metrics.scale_factor;
        // Integer texture extents may round down by less than one raster pixel.
        assert!((fitted.width as f32 - width as f32 * scale).abs() < 1.);
        assert!((fitted.height as f32 - height as f32 * scale).abs() < 1.);
        if u64::from(width) * u64::from(height) * 4 <= WindowsPresenter::MAX_FRAME_BYTES {
            assert_eq!([fitted.width, fitted.height], [width, height]);
            assert_eq!(raster, transform);
        }
    }
    Ok(())
}

#[test]
fn large_previews_keep_replacing_retained_frames_within_the_pool_budget() -> Result<()> {
    use std::collections::VecDeque;
    use zipzi_editor_core::{ViewportMetrics, ViewportPoint, ViewportTransform};
    let consumer = device()?;
    let context = windows_context(&consumer)?;
    let mut presenter = WindowsPresenter::new(&context, consumer.clone())?;
    let mut gpui = gpui_windows::SurfaceTestRenderer::new(consumer)?;
    // Previously accepted a 137 MiB frame, then retried its replacement forever.
    assert!(presenter.acquire([6000, 6000]).is_err());
    let mut retained = VecDeque::new();
    for i in 0..WindowsPresenter::MAX_SLOTS * 2 {
        let [width, height] = if i % 2 == 0 {
            [6000, 6000]
        } else {
            [7680, 4320]
        };
        let (target, _) = WindowsPresenter::fit_preview(
            WgpuTargetConfig {
                width,
                height,
                format: wgpu::TextureFormat::Rgba8Unorm,
            },
            ViewportTransform {
                scale: 1.,
                offset: ViewportPoint::default(),
                metrics: ViewportMetrics {
                    width: width as f32,
                    height: height as f32,
                    scale_factor: 1.,
                },
            },
        )?;
        let color = if i % 2 == 0 {
            [1., 0., 0., 1.]
        } else {
            [0., 0., 1., 1.]
        };
        // Keep the old displayed scenes until their replacement is ready, just like the UI.
        let next = scene(
            frame(
                &context,
                &mut presenter,
                [target.width, target.height],
                color,
            )?,
            1.,
        );
        let pixels = gpui.render(&next, [32, 32], [1.; 4])?;
        close(
            pixels.get_pixel(5, 5).0,
            if i % 2 == 0 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            },
        );
        if retained.len() == WindowsPresenter::MAX_SLOTS - 1 {
            retained.pop_front();
        }
        retained.push_back(next);
    }
    Ok(())
}

#[test]
fn shared_texture_composes_clips_redraws_and_recycles_without_overwriting_retained_frames()
-> Result<()> {
    let consumer = device()?;
    let context = windows_context(&consumer)?;
    println!(
        "adapter={} backend={}",
        context.adapter_name(),
        context.adapter_backend()
    );
    let mut presenter = WindowsPresenter::new(&context, consumer.clone())?;
    let mut gpui = gpui_windows::SurfaceTestRenderer::new(consumer)?;
    let retained = frame(&context, &mut presenter, [67, 64], [0.5, 0., 0., 0.5])?;
    let original = scene(retained.clone(), 1.);
    for _ in 0..3 {
        let pixels = gpui.render(&original, [32, 32], [1.; 4])?;
        close(pixels.get_pixel(5, 5).0, [255, 127, 127, 255]);
        close(pixels.get_pixel(3, 5).0, [255; 4]);
        close(pixels.get_pixel(10, 10).0, [0, 255, 0, 255]);
    }
    let translucent = gpui.render(&scene(retained.clone(), 0.5), [32, 32], [0.; 4])?;
    close(translucent.get_pixel(5, 5).0, [64, 0, 0, 64]);
    for i in 0..100 {
        let extent = if i % 2 == 0 { [67, 64] } else { [91, 55] };
        let image = frame(&context, &mut presenter, extent, [0., 0., 1., 1.])?;
        let pixels = gpui.render(&scene(image, 1.), [32, 32], [1.; 4])?;
        close(pixels.get_pixel(5, 5).0, [0, 0, 255, 255]);
        let _ = context.device().poll(wgpu::PollType::Poll)?;
    }
    let pixels = gpui.render(&original, [32, 32], [1.; 4])?;
    close(pixels.get_pixel(5, 5).0, [255, 127, 127, 255]);
    assert!(retained.is_idle());
    // Every live immutable owner pins its slot, even after GPU completion.
    let mut pinned = vec![];
    for _ in 1..WindowsPresenter::MAX_SLOTS {
        let image = frame(&context, &mut presenter, [67, 64], [0.; 4])?;
        gpui.render(&scene(image.clone(), 1.), [32, 32], [1.; 4])?;
        pinned.push(image);
    }
    assert!(presenter.acquire([67, 64])?.is_none());
    pinned.clear();
    assert!(presenter.acquire([67, 64])?.is_some());
    assert!(presenter.acquire([16384, 16384]).is_err());
    Ok(())
}

#[test]
fn model_adapter_hands_premultiplied_output_to_gpui_without_readback() -> Result<()> {
    use zipzi_core::{Drawable, DrawableFrame, Vec2};
    use zipzi_editor_core::{
        SceneSnapshot, SceneTexture, ViewportMetrics, ViewportPoint, ViewportTransform,
    };
    let frame = Arc::new(DrawableFrame {
        drawables: vec![Drawable {
            id: "triangle".into(),
            texture_asset_id: "color".into(),
            positions: vec![Vec2::new(0., 0.), Vec2::new(60., 0.), Vec2::new(0., -60.)],
            uvs: Arc::from([Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)]),
            indices: Arc::from([0, 1, 2]),
            ..Default::default()
        }],
        ..Default::default()
    });
    let scene = SceneSnapshot {
        model_opacity: 1.,
        runtime_frame: frame.clone(),
        viewport_frame: frame,
        textures: Arc::from([SceneTexture {
            asset_id: "color".into(),
            cache_key: "red".into(),
            width: 1,
            height: 1,
            rgba: Arc::from([255, 0, 0, 128]),
        }]),
    };

    let consumer = device()?;
    let context = windows_context(&consumer)?;
    let target = WgpuTargetConfig {
        width: 67,
        height: 64,
        format: wgpu::TextureFormat::Rgba8Unorm,
    };
    let mut presenter = WindowsPresenter::new(&context, consumer.clone())?;
    let mut renderer = WgpuSceneRenderer::with_context(context, target)?;
    let slot = presenter.acquire([67, 64])?.context("slot")?;
    let transform = ViewportTransform {
        scale: 1.,
        offset: ViewportPoint::default(),
        metrics: ViewportMetrics {
            width: 67.,
            height: 64.,
            scale_factor: 1.,
        },
    };
    let output = renderer.submit_transformed(target, &scene, transform)?;
    let image = presenter.publish(renderer.context(), slot, output.image)?;
    let gpui_scene = full_surface(image, [67, 64]);
    let mut gpui = gpui_windows::SurfaceTestRenderer::new(consumer.clone())?;
    let pixels = gpui.render(&gpui_scene, [67, 64], [1.; 4])?;
    close(pixels.get_pixel(5, 5).0, [255, 127, 127, 255]);
    close(pixels.get_pixel(66, 63).0, [255; 4]);
    // Upper right is inside the triangle; lower right is outside. This catches
    // UV inversion/channel errors which a solid-color transfer cannot reveal.
    close(pixels.get_pixel(45, 5).0, [255, 127, 127, 255]);
    close(pixels.get_pixel(45, 45).0, [255; 4]);
    let mut native = D3d11Preview::new(consumer.clone())?;
    let recorded = native
        .record([67, 64], &scene, transform, |_| {})?
        .context("native slot")?;
    unsafe {
        consumer
            .GetImmediateContext()?
            .ExecuteCommandList(&recorded.commands, true)
    };
    let native_scene = full_surface(recorded.surface, [67, 64]);
    assert_eq!(gpui.render(&native_scene, [67, 64], [1.; 4])?, pixels);
    let mut moved = scene.clone();
    let mut moved_frame = (*moved.viewport_frame).clone();
    for point in &mut moved_frame.drawables[0].positions {
        point.x += 16.;
    }
    moved.viewport_frame = Arc::new(moved_frame);
    // Worker recording and UI submission remain separate. The next pose must
    // not overwrite a retained, already-submitted surface.
    let (mut native, moved_record) = std::thread::spawn(move || -> Result<_> {
        let record = native
            .record([67, 64], &moved, transform, |_| {})?
            .context("moved slot")?;
        Ok((native, record))
    })
    .join()
    .unwrap()?;
    unsafe {
        consumer
            .GetImmediateContext()?
            .ExecuteCommandList(&moved_record.commands, true);
    }
    let moved_pixels = gpui.render(
        &full_surface(moved_record.surface, [67, 64]),
        [67, 64],
        [1.; 4],
    )?;
    close(moved_pixels.get_pixel(5, 5).0, [255; 4]);
    close(moved_pixels.get_pixel(30, 5).0, [255, 127, 127, 255]);
    assert_eq!(gpui.render(&native_scene, [67, 64], [1.; 4])?, pixels);

    // Hold recorded frames without submitting them, as when UI delivery is
    // coalesced. Releasing those lists frees slots while the old scene stays pinned.
    let mut pending = Vec::new();
    for _ in 1..WindowsPresenter::MAX_SLOTS {
        pending.push(
            native
                .record([67, 64], &scene, transform, |_| {})?
                .context("native free slot")?,
        );
    }
    assert!(
        native
            .record([67, 64], &scene, transform, |_| {})?
            .is_none()
    );
    drop(pending);
    let mut replacement = scene.clone();
    replacement.textures = Arc::from([SceneTexture {
        cache_key: "blue".into(),
        rgba: Arc::from([0, 0, 255, 128]),
        ..scene.textures[0].clone()
    }]);
    let resized = [73, 65];
    let mut resized_transform = transform;
    resized_transform.metrics.width = resized[0] as f32;
    resized_transform.metrics.height = resized[1] as f32;
    let next = native
        .record(resized, &replacement, resized_transform, |_| {})?
        .context("native resized slot")?;
    unsafe {
        consumer
            .GetImmediateContext()?
            .ExecuteCommandList(&next.commands, true)
    };
    let new_pixels = gpui.render(&full_surface(next.surface, resized), resized, [1.; 4])?;
    close(new_pixels.get_pixel(5, 5).0, [127, 127, 255, 255]);
    drop(native);
    assert_eq!(gpui.render(&native_scene, [67, 64], [1.; 4])?, pixels);

    // Retained consumer frames outlive both the model renderer and producer pool.
    drop(renderer);
    drop(presenter);
    let pixels = gpui.render(&gpui_scene, [67, 64], [1.; 4])?;
    close(pixels.get_pixel(5, 5).0, [255, 127, 127, 255]);

    Ok(())
}

#[test]
#[ignore = "requires a local model3 or project fixture"]
fn d3d11_preview_matches_wgpu_on_local_model() -> Result<()> {
    use zipzi_editor_core::{
        Editor, EditorCommand, LoadRequest, ViewportId, ViewportKind, ViewportMetrics,
        ViewportTransform,
    };
    let model = std::env::var_os("ZIPZI_D3D11_PARITY_MODEL").context("ZIPZI_D3D11_PARITY_MODEL")?;
    let extent = [768, 768];
    let metrics = ViewportMetrics {
        width: extent[0] as f32,
        height: extent[1] as f32,
        scale_factor: 1.,
    };
    let mut editor = Editor::empty();
    editor.register_primary_viewport(ViewportId::new(), ViewportKind::Modeling)?;
    editor.set_canvas_metrics(metrics);
    let model = std::path::PathBuf::from(model).canonicalize()?;
    let request = if model.extension().is_some_and(|ext| ext == "zipzi") {
        LoadRequest::Open(model)
    } else {
        LoadRequest::Model3(model)
    };
    let task = editor.prepare_load(request)?.context("model load task")?;
    editor.finish_load(task.run())?;
    let document = editor.active_id().context("model document")?;
    let parameters = editor.document().context("model document")?.parameters();
    let parameter = parameters
        .iter()
        .find(|p| p.name == "Angle_X")
        .or(parameters.first());

    let consumer = device()?;
    let context = windows_context(&consumer)?;
    let mut presenter = WindowsPresenter::new(&context, consumer.clone())?;
    let target = WgpuTargetConfig {
        width: extent[0],
        height: extent[1],
        format: wgpu::TextureFormat::Rgba8Unorm,
    };
    let mut wgpu = WgpuSceneRenderer::with_context(context, target)?;
    let mut native = D3d11Preview::new(consumer.clone())?;
    let mut gpui = gpui_windows::SurfaceTestRenderer::new(consumer.clone())?;
    let poses = parameter.map_or_else(
        || vec![None],
        |p| vec![None, Some(p.minimum), Some(p.maximum)],
    );
    for pose in poses {
        if let Some(value) = pose {
            editor.execute_for(
                document,
                EditorCommand::SetPreviewParameters {
                    values: vec![(parameter.unwrap().id.clone(), value)],
                },
            )?;
        }
        let preview = editor.preview()?;
        let transform = ViewportTransform::new(metrics, preview.view).context("model transform")?;
        let slot = presenter.acquire(extent)?.context("wgpu slot")?;
        let wgpu_frame = wgpu.submit_transformed(target, &preview.scene, transform)?;
        let wgpu_surface = presenter.publish(wgpu.context(), slot, wgpu_frame.image)?;
        let native_frame = native
            .record(extent, &preview.scene, transform, |_| {})?
            .context("D3D11 slot")?;
        unsafe {
            consumer
                .GetImmediateContext()?
                .ExecuteCommandList(&native_frame.commands, true)
        };
        let reference = gpui.render(&full_surface(wgpu_surface, extent), extent, [0.; 4])?;
        let candidate =
            gpui.render(&full_surface(native_frame.surface, extent), extent, [0.; 4])?;
        let mut nonempty = 0;
        let mut different = 0;
        let mut sum = 0_u64;
        for (a, b) in reference.pixels().zip(candidate.pixels()) {
            nonempty += usize::from(a.0[3] > 0);
            let diff = std::array::from_fn::<_, 4, _>(|i| a.0[i].abs_diff(b.0[i]));
            different += usize::from(diff.iter().any(|&channel| channel > 8));
            sum += diff.into_iter().map(u64::from).sum::<u64>();
        }
        let mean = sum as f64 / reference.as_raw().len() as f64;
        println!(
            "D3D11 vs wgpu DX12 at {pose:?}: {nonempty} nontransparent pixels, {different} pixels differ by >8, mean channel difference {mean:.5}"
        );
        assert!(nonempty > 0 && mean < 0.5 && different * 1000 < reference.pixels().len());
    }
    Ok(())
}
#[test]
fn closing_a_preview_does_not_wait_for_its_in_flight_gpu_copy() -> Result<()> {
    use windows::Win32::Graphics::Direct3D12::{D3D12_FENCE_FLAG_NONE, ID3D12Fence};
    let consumer = device()?;
    let context = windows_context(&consumer)?;
    let mut presenter = WindowsPresenter::new(&context, consumer.clone())?;
    let hal = unsafe { context.device().as_hal::<wgpu::hal::api::Dx12>() }.unwrap();
    let gate: ID3D12Fence = unsafe { hal.raw_device().CreateFence(0, D3D12_FENCE_FLAG_NONE)? };
    unsafe {
        hal.raw_queue().Wait(&gate, 1)?;
    }
    drop(hal);
    let image = frame(&context, &mut presenter, [67, 64], [1., 0., 0., 1.])?;
    let started = std::time::Instant::now();
    drop(presenter);
    let elapsed = started.elapsed();
    // Always release the deliberate GPU stall before asserting.
    unsafe {
        gate.Signal(1)?;
    }
    assert!(
        elapsed < std::time::Duration::from_secs(1),
        "UI teardown waited for GPU: {elapsed:?}"
    );
    let mut gpui = gpui_windows::SurfaceTestRenderer::new(consumer)?;
    let pixels = gpui.render(&scene(image, 1.), [32, 32], [1.; 4])?;
    close(pixels.get_pixel(5, 5).0, [255, 0, 0, 255]);
    Ok(())
}

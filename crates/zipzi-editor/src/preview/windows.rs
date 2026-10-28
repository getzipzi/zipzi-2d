//! Immutable GPU previews: D3D12 copy -> shared texture -> GPUI D3D11.
use super::*;
use gpui_kit::{
    App, Bounds, ContentMask, IntoElement, Pixels, Styled, SurfaceSource, Window, canvas,
};
use zipzi_editor_render::{WgpuSceneRenderer, WgpuTargetConfig, WindowsPresenter, windows_context};

pub type Context = ::windows::Win32::Graphics::Direct3D11::ID3D11Device;
pub type Device = Context;
pub fn context(window: &Window) -> Option<Context> {
    window.directx_device()
}
pub fn device_changed(window: &Window, device: Option<&Device>) -> bool {
    context(window).as_ref() != device
}
pub fn autorelease<T>(f: impl FnOnce() -> T) -> T {
    f()
}
pub fn init(_: &mut App) {}

pub struct ImageData {
    surface: Option<Arc<gpui_kit::DirectXSurface>>,
    trace_id: u64,
}
pub type Image = Arc<ImageData>;
pub fn target(width: u32, height: u32) -> WgpuTargetConfig {
    WgpuTargetConfig {
        width,
        height,
        format: wgpu::TextureFormat::Rgba8Unorm,
    }
}

pub struct Renderer {
    // Drop the presenter (drain native copies) before dropping the wgpu context.
    pub presenter: WindowsPresenter,
    pub scene: WgpuSceneRenderer,
}
impl Renderer {
    pub fn new(device: Device) -> anyhow::Result<Self> {
        let context = windows_context(&device)?;
        let presenter = WindowsPresenter::new(&context, device)?;
        let scene = WgpuSceneRenderer::with_context(context, target(1, 1))?;
        Ok(Self { scene, presenter })
    }
    pub fn publish(
        &mut self,
        slot: usize,
        texture: wgpu::Texture,
        trace_id: u64,
    ) -> anyhow::Result<Image> {
        let surface = self
            .presenter
            .publish(self.scene.context(), slot, texture)?;
        let _ = self.scene.context().device().poll(wgpu::PollType::Poll);
        frame_trace::emit(
            trace_id,
            "shared_texture_submitted",
            &[(
                "bytes",
                u64::from(surface.extent[0]) * u64::from(surface.extent[1]) * 4,
            )],
        );
        Ok(Arc::new(ImageData {
            surface: Some(surface),
            trace_id,
        }))
    }
}
pub fn paint(image: &Image, bounds: Bounds<Pixels>, window: &mut Window) -> anyhow::Result<()> {
    paint_in_bounds(image, bounds, bounds, window)
}
pub fn paint_in_bounds(
    image: &Image,
    bounds: Bounds<Pixels>,
    content: Bounds<Pixels>,
    window: &mut Window,
) -> anyhow::Result<()> {
    if let Some(surface) = &image.surface {
        if window.directx_device().as_ref() != Some(&surface.device) {
            return Ok(());
        }
        frame_trace::emit(image.trace_id, "paint_shared_surface", &[]);
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.paint_surface(content, SurfaceSource::DirectX(surface.clone()));
        });
    }
    Ok(())
}
pub fn element(image: Image) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let _ = paint(&image, bounds, window);
        },
    )
    .size_full()
}

pub fn start(window: &Window) -> anyhow::Result<Connection> {
    let device = context(window).ok_or_else(|| anyhow::anyhow!("D3D11 device unavailable"))?;
    let worker_device = device.clone();
    let shared = Arc::new((Mutex::new(Inbox::default()), Condvar::new()));
    let requests = RequestSender {
        shared: shared.clone(),
    };
    let (frames, receiver) = mailbox();
    std::thread::Builder::new()
        .name("zipzi-wgpu-preview".into())
        .spawn(move || {
            let mut renderer = match Renderer::new(worker_device) {
                Ok(renderer) => renderer,
                Err(error) => {
                    let _ = frames.send(
                        None,
                        Frame {
                            binding: None,
                            trace_id: 0,
                            revision: 0,
                            canvas: None,
                            animation: None,
                            stats: (0, 0, 0),
                            image: Err(error.to_string()),
                        },
                    );
                    return;
                }
            };
            eprintln!(
                "Editor preview: {} / {} (D3D12 shared texture; no readback)",
                renderer.scene.context().adapter_name(),
                renderer.scene.context().adapter_backend()
            );
            loop {
                let request = {
                    let (lock, ready) = &*shared;
                    let mut inbox = lock.lock().unwrap();
                    while inbox.pending.is_empty() && !inbox.closed {
                        inbox = ready.wait(inbox).unwrap();
                    }
                    if inbox.closed {
                        break;
                    }
                    inbox.pending.pop().unwrap()
                };
                let trace_id = request.trace_id;
                // Backpressure before encoding; pending requests continue coalescing.
                let prepared = WindowsPresenter::fit_preview(
                    target(request.target.width, request.target.height),
                    request.transform,
                )
                .and_then(|(target, transform)| {
                    let slot = renderer.presenter.acquire([target.width, target.height])?;
                    Ok((slot, target, transform))
                });
                let slot = match prepared {
                    Ok((Some(slot), target, transform)) => Ok((slot, target, transform)),
                    Ok((None, _, _)) => {
                        let (lock, ready) = &*shared;
                        let mut inbox = lock.lock().unwrap();
                        if inbox.closed {
                            break;
                        }
                        let latest = inbox
                            .pending
                            .remove(request.binding.viewport)
                            .unwrap_or(request);
                        inbox.pending.insert(latest.binding.viewport, latest);
                        drop(
                            ready
                                .wait_timeout(inbox, std::time::Duration::from_millis(2))
                                .unwrap(),
                        );
                        let _ = renderer.scene.context().device().poll(wgpu::PollType::Poll);
                        continue;
                    }
                    Err(error) => Err(error),
                };
                frame_trace::emit(trace_id, "dequeued", &[]);
                frame_trace::emit(trace_id, "worker_begin", &[]);
                let image = slot
                    .and_then(|(slot, target, transform)| {
                        renderer
                            .scene
                            .submit_transformed_traced(target, &request.scene, transform, |stage| {
                                frame_trace::emit(trace_id, stage, &[])
                            })
                            .and_then(|frame| renderer.publish(slot, frame.image, trace_id))
                    })
                    .map_err(|e| e.to_string());
                frame_trace::emit(
                    trace_id,
                    if image.is_ok() {
                        "published"
                    } else {
                        "render_error"
                    },
                    &[],
                );
                if frames
                    .send_with_replaced(
                        Some(request.binding.viewport),
                        Frame {
                            binding: Some(request.binding),
                            trace_id,
                            revision: request.revision,
                            canvas: request.canvas,
                            animation: request.animation,
                            stats: renderer.scene.stats(),
                            image,
                        },
                        |old| {
                            frame_trace::emit(
                                old.trace_id,
                                "delivery_coalesced",
                                &[("replaced_by", trace_id)],
                            )
                        },
                    )
                    .is_err()
                {
                    break;
                }
                frame_trace::emit(trace_id, "delivery_sent", &[]);
            }
        })?;
    Ok((requests, receiver, Some(device)))
}

#[cfg(test)]
pub fn test_image() -> Image {
    Arc::new(ImageData {
        surface: None,
        trace_id: 0,
    })
}

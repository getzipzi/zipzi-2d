//! Immutable GPU previews consumed by GPUI's D3D11 renderer.
use super::*;
use ::windows::Win32::Graphics::Direct3D11::ID3D11CommandList;
use gpui_kit::{
    App, Bounds, ContentMask, IntoElement, Pixels, Styled, SurfaceSource, Window, canvas,
};
use zipzi_editor_render::{D3d11Preview, RecordedD3d11Preview, fit_windows_preview};
#[cfg(feature = "wgpu-backend")]
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
    commands: Option<Mutex<Option<ID3D11CommandList>>>,
    trace_id: u64,
}
pub type Image = Arc<ImageData>;
#[cfg(feature = "wgpu-backend")]
fn wgpu_target([width, height]: [u32; 2]) -> WgpuTargetConfig {
    WgpuTargetConfig {
        width,
        height,
        format: wgpu::TextureFormat::Rgba8Unorm,
    }
}

#[cfg(feature = "wgpu-backend")]
struct WgpuPreview {
    // Drop the presenter (drain native copies) before dropping the wgpu context.
    presenter: WindowsPresenter,
    scene: WgpuSceneRenderer,
}
#[cfg(feature = "wgpu-backend")]
impl WgpuPreview {
    fn new(device: Device) -> anyhow::Result<Self> {
        let context = windows_context(&device)?;
        let presenter = WindowsPresenter::new(&context, device)?;
        let scene = WgpuSceneRenderer::with_context(context, wgpu_target([1, 1]))?;
        Ok(Self { scene, presenter })
    }
    fn publish(
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
            commands: None,
            trace_id,
        }))
    }
}

enum Renderer {
    #[cfg(feature = "wgpu-backend")]
    Wgpu(WgpuPreview),
    D3d11(D3d11Preview),
}

impl Renderer {
    fn new(device: Device) -> anyhow::Result<Self> {
        #[cfg(feature = "wgpu-backend")]
        if std::env::var_os("ZIPZI_PREVIEW_BACKEND").as_deref() != Some("d3d11".as_ref()) {
            let preview = WgpuPreview::new(device)?;
            eprintln!(
                "Editor preview: {} / {} (D3D12 shared texture; no readback)",
                preview.scene.context().adapter_name(),
                preview.scene.context().adapter_backend()
            );
            return Ok(Self::Wgpu(preview));
        }
        let preview = D3d11Preview::new(device)?;
        eprintln!("Editor preview: native D3D11 (GPUI device, deferred recording, UI submission)");
        Ok(Self::D3d11(preview))
    }

    fn stats(&self) -> (u64, u64, u64) {
        match self {
            #[cfg(feature = "wgpu-backend")]
            Self::Wgpu(renderer) => renderer.scene.stats(),
            Self::D3d11(renderer) => renderer.stats(),
        }
    }

    fn poll(&self) {
        #[cfg(feature = "wgpu-backend")]
        if let Self::Wgpu(renderer) = self {
            let _ = renderer.scene.context().device().poll(wgpu::PollType::Poll);
        }
    }

    fn retain_cached_textures(&mut self, keys: &HashSet<String>) {
        match self {
            #[cfg(feature = "wgpu-backend")]
            Self::Wgpu(renderer) => {
                if let Err(error) = renderer.scene.retain_cached_textures(keys) {
                    eprintln!("Preview cache release failed: {error}");
                }
                renderer.presenter.release_cached_slots();
            }
            Self::D3d11(renderer) => renderer.retain_cached_textures(keys),
        }
        self.poll();
    }

    fn render(&mut self, request: &Request) -> anyhow::Result<Option<Image>> {
        let (extent, transform) = fit_windows_preview(
            [request.target.width, request.target.height],
            request.transform,
        )?;
        let trace_id = request.trace_id;
        let trace = |stage| frame_trace::emit(trace_id, stage, &[]);
        let image = match self {
            #[cfg(feature = "wgpu-backend")]
            Self::Wgpu(renderer) => {
                let Some(slot) = renderer.presenter.acquire(extent)? else {
                    return Ok(None);
                };
                trace("dequeued");
                trace("worker_begin");
                let frame = renderer.scene.submit_transformed_traced(
                    wgpu_target(extent),
                    &request.scene,
                    transform,
                    trace,
                )?;
                renderer.publish(slot, frame.image, trace_id)?
            }
            Self::D3d11(renderer) => {
                let Some(RecordedD3d11Preview { surface, commands }) =
                    renderer.record(extent, &request.scene, transform, trace)?
                else {
                    return Ok(None);
                };
                Arc::new(ImageData {
                    surface: Some(surface),
                    commands: Some(Mutex::new(Some(commands))),
                    trace_id,
                })
            }
        };
        frame_trace::emit(
            trace_id,
            "preview_extent",
            &[
                ("width", u64::from(extent[0])),
                ("height", u64::from(extent[1])),
            ],
        );
        Ok(Some(image))
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
        if let Some(commands) = &image.commands {
            let mut commands = commands.lock().unwrap();
            if let Some(list) = commands.as_ref() {
                // The surface is sampled later on this same immediate context.
                // Restore GPUI state after executing the worker's recorded list.
                let immediate = unsafe { surface.device.GetImmediateContext()? };
                frame_trace::emit(image.trace_id, "native_d3d11_submit_begin", &[]);
                unsafe { immediate.ExecuteCommandList(list, true) };
                frame_trace::emit(image.trace_id, "native_d3d11_submit_end", &[]);
                *commands = None;
            }
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
        .name("zipzi-windows-preview".into())
        .spawn(move || {
            let mut renderer = match Renderer::new(worker_device) {
                Ok(renderer) => renderer,
                Err(error) => {
                    eprintln!("Editor preview initialization failed: {error:#}");
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
            let mut cache_owners = PreviewCacheOwners::default();
            loop {
                let work = {
                    let (lock, ready) = &*shared;
                    let mut inbox = lock.lock().unwrap();
                    while !inbox.has_work() && !inbox.closed {
                        inbox = ready.wait(inbox).unwrap();
                    }
                    if inbox.closed {
                        break;
                    }
                    inbox.pop_work().unwrap()
                };
                let request = match work {
                    PreviewWork::Retain(live) => {
                        let keys = cache_owners.retain(&live);
                        renderer.retain_cached_textures(&keys);
                        continue;
                    }
                    PreviewWork::Render(request) => request,
                };
                let trace_id = request.trace_id;
                cache_owners.note(&request);
                // Backpressure before encoding; pending requests continue coalescing.
                let image = match renderer.render(&request) {
                    Ok(Some(image)) => Ok(image),
                    Ok(None) => {
                        let (lock, ready) = &*shared;
                        let mut inbox = lock.lock().unwrap();
                        if inbox.closed {
                            break;
                        }
                        inbox.requeue_after_backpressure(request);
                        drop(
                            ready
                                .wait_timeout(inbox, std::time::Duration::from_millis(2))
                                .unwrap(),
                        );
                        renderer.poll();
                        continue;
                    }
                    Err(error) => {
                        eprintln!("Editor preview render failed: {error:#}");
                        Err(error.to_string())
                    }
                };
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
                            stats: renderer.stats(),
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
        commands: None,
        trace_id: 0,
    })
}

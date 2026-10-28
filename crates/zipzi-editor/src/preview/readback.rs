//! WGPU worker → straight BGRA → GPUI's D3D11 image atlas.
//! No window GPU resources cross this boundary. Each published image is immutable.
use super::*;
use gpui_kit::{App, Bounds, IntoElement, Pixels, RenderImage, Styled, Window, canvas};
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};
use zipzi_editor_render::{WgpuSceneRenderer, WgpuTargetConfig};

/// Readback uses an independent WGPU device, not the window's D3D11 device.
#[derive(Clone)]
pub struct Context;
pub type Device = Context;
pub fn context(_window: &Window) -> Option<Context> {
    Some(Context)
}
pub fn device_changed(_window: &Window, _device: Option<&Device>) -> bool {
    // WGPU frames are CPU images and have no identity tied to a window device.
    false
}
pub fn autorelease<T>(f: impl FnOnce() -> T) -> T {
    f()
}

type Retirement = (
    async_channel::Sender<Arc<RenderImage>>,
    async_channel::Receiver<Arc<RenderImage>>,
);
fn retirement() -> &'static Retirement {
    static CHANNEL: OnceLock<Retirement> = OnceLock::new();
    CHANNEL.get_or_init(async_channel::unbounded)
}
pub fn init(cx: &mut App) {
    let receiver = retirement().1.clone();
    cx.spawn(async move |cx| {
        while let Ok(image) = receiver.recv().await {
            cx.update(|cx| cx.drop_image(image, None));
        }
    })
    .detach();
}

pub struct ImageData {
    pixels: Arc<RenderImage>,
    painted: AtomicBool,
}
impl Drop for ImageData {
    fn drop(&mut self) {
        // Only images actually uploaded need retirement. Coalesced worker frames
        // never enter the atlas. Frozen frames retain this owner until released.
        if self.painted.load(Ordering::Relaxed) {
            let _ = retirement().0.try_send(self.pixels.clone());
        }
    }
}
pub type Image = Arc<ImageData>;

pub fn target(width: u32, height: u32) -> WgpuTargetConfig {
    WgpuTargetConfig {
        width,
        height,
        format: wgpu::TextureFormat::Rgba8Unorm,
    }
}

pub fn read_image(
    renderer: &WgpuSceneRenderer,
    texture: &wgpu::Texture,
    trace_id: u64,
) -> anyhow::Result<Image> {
    let start = std::time::Instant::now();
    let mut rgba = renderer
        .context()
        .read_rgba8(texture)
        .map_err(|e| anyhow::anyhow!("{}: {}", e.code, e.message))?;
    let readback_ns = start.elapsed().as_nanos() as u64;
    straight_bgra(&mut rgba);
    frame_trace::emit(
        trace_id,
        "readback",
        &[
            ("readback_ns", readback_ns),
            (
                "conversion_ns",
                start.elapsed().as_nanos() as u64 - readback_ns,
            ),
            ("bytes", rgba.len() as u64),
        ],
    );
    let buffer = image::RgbaImage::from_raw(texture.width(), texture.height(), rgba)
        .ok_or_else(|| anyhow::anyhow!("Invalid readback dimensions"))?;
    Ok(Arc::new(ImageData {
        pixels: Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])),
        painted: AtomicBool::new(false),
    }))
}

fn straight_bgra(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        let [red, green, blue, alpha] = *pixel;
        *pixel = match alpha {
            0 => [0; 4],
            255 => [blue, green, red, alpha],
            _ => {
                let alpha32 = u32::from(alpha);
                let straight =
                    |value: u8| ((u32::from(value) * 255 + alpha32 / 2) / alpha32).min(255) as u8;
                [straight(blue), straight(green), straight(red), alpha]
            }
        };
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
    image.painted.store(true, Ordering::Relaxed);
    window.paint_image(
        bounds,
        content,
        Default::default(),
        image.pixels.clone(),
        0,
        false,
    )
}
pub fn element(image: Image) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            if let Err(error) = paint(&image, bounds, window) {
                eprintln!("Preview upload failed: {error}");
            }
        },
    )
    .size_full()
}

pub fn start(_window: &Window) -> anyhow::Result<Connection> {
    let shared = Arc::new((Mutex::new(Inbox::default()), Condvar::new()));
    let requests = RequestSender {
        shared: shared.clone(),
    };
    let (frames, receiver) = mailbox();
    std::thread::Builder::new()
        .name("zipzi-wgpu-preview".into())
        .spawn(move || {
            let mut renderer = match WgpuSceneRenderer::new(target(1, 1)) {
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
                "Editor preview: {} / {} (readback)",
                renderer.context().adapter_name(),
                renderer.context().adapter_backend()
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
                frame_trace::emit(trace_id, "dequeued", &[]);
                frame_trace::emit(trace_id, "worker_begin", &[]);
                // Readback waits here, so at most one GPU command is in flight and
                // requests continue coalescing while the UI remains responsive.
                let image = renderer
                    .submit_transformed_traced(
                        target(request.target.width, request.target.height),
                        &request.scene,
                        request.transform,
                        |stage| frame_trace::emit(trace_id, stage, &[]),
                    )
                    .and_then(|frame| read_image(&renderer, &frame.image, trace_id))
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
    Ok((requests, receiver, Some(Context)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn gpui_receives_straight_bgra_including_transparent_texels() {
        let mut bytes = [128, 64, 0, 128, 90, 80, 70, 0, 1, 2, 3, 255];
        super::straight_bgra(&mut bytes);
        assert_eq!(bytes, [0, 128, 255, 128, 0, 0, 0, 0, 3, 2, 1, 255]);
    }
}

#[cfg(test)]
pub fn test_image() -> Image {
    Arc::new(ImageData {
        pixels: Arc::new(RenderImage::new(vec![image::Frame::new(
            image::RgbaImage::new(1, 1),
        )])),
        painted: AtomicBool::new(false),
    })
}

//! Native Metal preview adapter. No document editing or project IO lives here.
pub use crate::overlay::MetalOverlayRenderer;
use anyhow::{Result, anyhow, ensure};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use zipzi_core::Vec2;
use zipzi_editor_core::{SceneSnapshot, ViewState, ViewportMetrics, ViewportTransform};
use zipzi_render::texture::{premultiply_rgba8, premultiply_rgba8_in_place, straight_rgba_mipmaps};
use zipzi_render::{Affine2, ViewportConfig};
use zipzi_render_metal::{
    MetalContext, MetalOutputMode, MetalRenderer, MetalTexture, MetalTextureCatalog, metal,
};

pub use zipzi_render_metal::MetalTargetConfig;

type Mipmaps = Vec<(u32, u32, Vec<u8>)>;
struct PreparedTexture {
    rgba: Vec<u8>,
    mipmaps: Mipmaps,
}

fn prepare_texture(source: &zipzi_editor_core::SceneTexture) -> PreparedTexture {
    let mut mipmaps = straight_rgba_mipmaps(source.width, source.height, &source.rgba);
    // Generate every straight level before premultiplying owned upload buffers.
    // The next level must always filter straight colors with alpha weighting.
    for (_, _, pixels) in &mut mipmaps {
        premultiply_rgba8_in_place(pixels);
    }
    PreparedTexture {
        rgba: premultiply_rgba8(&source.rgba),
        mipmaps,
    }
}
type PendingTexture<'a> = (String, &'a zipzi_editor_core::SceneTexture);
const PREPARE_BATCH_BYTES: usize = 64 << 20;

fn preparation_bytes(source: &zipzi_editor_core::SceneTexture) -> usize {
    // Bound the premultiplied base plus every mip (which together fit in twice
    // the base RGBA size, even for single-pixel axes), footprint tables and metadata.
    source
        .rgba
        .len()
        .saturating_mul(2)
        .saturating_add(
            (source.width as usize)
                .saturating_add(source.height as usize)
                .saturating_mul(32),
        )
        .saturating_add(1024)
}

fn batch_end(pending: &[PendingTexture<'_>], start: usize, limit: usize) -> usize {
    let mut bytes = 0usize;
    let mut end = start;
    while let Some((_, source)) = pending.get(end) {
        let next = preparation_bytes(source);
        if end != start && next > limit.saturating_sub(bytes) {
            break;
        }
        bytes = bytes.saturating_add(next);
        end += 1;
        if bytes >= limit {
            break;
        }
    }
    end
}

fn prepare_texture_batch(pending: &[PendingTexture<'_>]) -> Vec<(usize, PreparedTexture)> {
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .min(4)
        .min(pending.len());
    if workers <= 1 || pending.iter().map(|(_, s)| s.rgba.len()).sum::<usize>() < 1 << 20 {
        return pending
            .iter()
            .enumerate()
            .map(|(i, (_, s))| (i, prepare_texture(s)))
            .collect();
    }
    // Balance by pixels rather than asset count; model artwork has very
    // different dimensions. Only owned CPU buffers cross these thread boundaries.
    let mut jobs = vec![Vec::new(); workers];
    let mut loads = vec![0usize; workers];
    let mut order: Vec<_> = (0..pending.len()).collect();
    order.sort_unstable_by_key(|&i| std::cmp::Reverse(pending[i].1.rgba.len()));
    for i in order {
        let worker = (0..workers).min_by_key(|&w| loads[w]).unwrap();
        loads[worker] = loads[worker].saturating_add(pending[i].1.rgba.len());
        jobs[worker].push(i);
    }
    std::thread::scope(|scope| {
        let handles: Vec<_> = jobs
            .into_iter()
            .map(|indices| {
                scope.spawn(move || {
                    indices
                        .into_iter()
                        .map(|i| {
                            let source = pending[i].1;
                            (i, prepare_texture(source))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut prepared = Vec::with_capacity(pending.len());
        for handle in handles {
            prepared.extend(handle.join().expect("texture preparation worker"));
        }
        // Keep upload revisions/cache behavior independent of worker scheduling.
        prepared.sort_unstable_by_key(|(i, _)| *i);
        prepared
    })
}

/// The image is ordered on the consumer queue. Retain the command to bound
/// producer work by GPU completion without ever waiting on the UI thread.
pub struct SubmittedFrame {
    pub image: metal::Texture,
    pub command: metal::CommandBuffer,
}

struct UploadedTexture {
    texture: metal::Texture,
    width: u32,
    height: u32,
    revision: u64,
    used: u64,
    bytes: u64,
}

pub struct MetalSceneRenderer {
    context: MetalContext,
    renderer: MetalRenderer,
    textures: HashMap<String, UploadedTexture>,
    next_revision: u64,
    draws: u64,
    uploads: u64,
    cache_budget: u64,
    synced_scene: Option<SceneSnapshot>,
    last_stats: zipzi_render_metal::MetalRenderStats,
}

fn status(error: zipzi_core::Status) -> anyhow::Error {
    anyhow!("{}: {}", error.code, error.message)
}

impl MetalSceneRenderer {
    pub fn new(device: metal::Device, config: MetalTargetConfig) -> Result<Self> {
        let context = MetalContext::from_device(device);
        let renderer = MetalRenderer::new(&context, config).map_err(status)?;
        Ok(Self {
            context,
            renderer,
            textures: HashMap::new(),
            next_revision: 1,
            draws: 0,
            uploads: 0,
            cache_budget: 128 * 1024 * 1024,
            synced_scene: None,
            last_stats: Default::default(),
        })
    }

    /// Produce an immutable frame on the consumer's queue, without a CPU wait/readback.
    pub fn draw(
        &mut self,
        queue: &metal::CommandQueueRef,
        config: MetalTargetConfig,
        scene: &SceneSnapshot,
        view: ViewState,
    ) -> Result<metal::Texture> {
        let metrics = ViewportMetrics {
            width: config.width as f32,
            height: config.height as f32,
            scale_factor: 1.,
        };
        let transform =
            ViewportTransform::new(metrics, view).ok_or_else(|| anyhow!("Invalid viewport"))?;
        self.draw_transformed(queue, config, scene, transform)
    }

    pub fn stats(&self) -> (u64, u64, u64) {
        (
            self.draws,
            self.uploads,
            self.textures.values().map(|t| t.bytes).sum(),
        )
    }
    pub fn last_frame_stats(&self) -> zipzi_render_metal::MetalRenderStats {
        self.last_stats
    }
    pub fn draw_transformed(
        &mut self,
        queue: &metal::CommandQueueRef,
        config: MetalTargetConfig,
        scene: &SceneSnapshot,
        transform: ViewportTransform,
    ) -> Result<metal::Texture> {
        Ok(self
            .submit_transformed(queue, config, scene, transform)?
            .image)
    }

    pub fn submit_transformed(
        &mut self,
        queue: &metal::CommandQueueRef,
        config: MetalTargetConfig,
        scene: &SceneSnapshot,
        transform: ViewportTransform,
    ) -> Result<SubmittedFrame> {
        self.submit_transformed_traced(queue, config, scene, transform, |_| {})
    }

    /// Optional CPU boundary observer; the normal path monomorphizes to a no-op.
    pub fn submit_transformed_traced(
        &mut self,
        queue: &metal::CommandQueueRef,
        config: MetalTargetConfig,
        scene: &SceneSnapshot,
        transform: ViewportTransform,
        mut boundary: impl FnMut(&'static str),
    ) -> Result<SubmittedFrame> {
        ensure!(transform.metrics.visible(), "Hidden canvas");
        self.draws += 1;
        let key = |source: &zipzi_editor_core::SceneTexture| {
            format!("{}:{}:{}", source.cache_key, source.width, source.height)
        };
        let mut missing = Vec::new();
        let mut scheduled = HashSet::new();
        for source in scene.textures.iter() {
            let key = key(source);
            if let Some(cached) = self.textures.get_mut(&key) {
                cached.used = self.draws;
                continue;
            }
            ensure!(
                source.width > 0
                    && source.height > 0
                    && source.width <= self.context.max_texture_dimension()
                    && source.height <= self.context.max_texture_dimension()
                    && u64::from(source.width) * u64::from(source.height) * 4
                        == source.rgba.len() as u64,
                "Invalid texture dimensions or RGBA bytes"
            );
            if scheduled.insert(key.clone()) {
                missing.push((key, source));
            }
        }
        let mut start = 0;
        while start < missing.len() {
            let end = batch_end(&missing, start, PREPARE_BATCH_BYTES);
            let batch = &missing[start..end];
            for (i, prepared) in prepare_texture_batch(batch) {
                let (key, source) = &batch[i];
                let bytes = source.rgba.len() as u64
                    + prepared
                        .mipmaps
                        .iter()
                        .map(|m| m.2.len() as u64)
                        .sum::<u64>();
                let texture = self
                    .context
                    .upload_premultiplied_rgba8(
                        source.width,
                        source.height,
                        &prepared.rgba,
                        &prepared.mipmaps,
                    )
                    .map_err(status)?;
                self.textures.insert(
                    key.clone(),
                    UploadedTexture {
                        texture,
                        width: source.width,
                        height: source.height,
                        revision: self.next_revision,
                        used: self.draws,
                        bytes,
                    },
                );
                self.next_revision += 1;
                self.uploads += 1;
            }
            start = end;
        }
        let mut cached_bytes = self.textures.values().map(|t| t.bytes).sum::<u64>();
        if cached_bytes > self.cache_budget {
            // Sort idle entries once instead of scanning the whole cache for
            // every eviction when switching between texture-heavy documents.
            let mut idle: Vec<_> = self
                .textures
                .iter()
                .filter(|(_, t)| t.used != self.draws)
                .map(|(key, texture)| (key.clone(), texture.used))
                .collect();
            idle.sort_unstable_by_key(|(_, used)| *used);
            for (key, _) in idle {
                if cached_bytes <= self.cache_budget {
                    break;
                }
                if let Some(texture) = self.textures.remove(&key) {
                    cached_bytes -= texture.bytes;
                }
            }
        }
        // Asset ids are local aliases for this scene; cache identity is immutable content.
        let mut textures = MetalTextureCatalog::new(
            scene
                .textures
                .iter()
                .map(|source| {
                    let texture = &self.textures[&key(source)];
                    (
                        source.asset_id.clone(),
                        MetalTexture {
                            view: &texture.texture,
                            width: texture.width,
                            height: texture.height,
                        },
                    )
                })
                .collect(),
        );
        for source in scene.textures.iter() {
            let texture = &self.textures[&key(source)];
            textures.set_revision(source.asset_id.clone(), texture.revision);
            textures.set_repeat(source.asset_id.clone(), true);
        }
        boundary("textures_ready");
        let output = self.context.output_texture(config).map_err(status)?;
        self.renderer.resize(config).map_err(status)?;
        if !self.synced_scene.as_ref().is_some_and(|last| {
            Arc::ptr_eq(&last.viewport_frame, &scene.viewport_frame)
                && Arc::ptr_eq(&last.textures, &scene.textures)
                && last.model_opacity == scene.model_opacity
        }) {
            self.renderer
                .sync_model_shared(scene.render_frame()?, &textures)
                .map_err(status)?;
            self.synced_scene = Some(scene.clone());
        }
        boundary("model_synced");
        let scale = transform.scale * transform.metrics.scale_factor;
        self.renderer
            .update_view(ViewportConfig {
                transform: Affine2 {
                    a: Vec2::new(scale, 0.),
                    b: Vec2::new(0., -scale),
                    origin: Vec2::new(
                        transform.offset.x * transform.metrics.scale_factor,
                        transform.offset.y * transform.metrics.scale_factor,
                    ),
                },
                target_extent: Vec2::new(config.width as f32, config.height as f32),
                mask_scale: scale as f64,
            })
            .map_err(status)?;
        // Use GPUI's queue, not MetalContext's convenience queue, to preserve ordering.
        let command = queue.new_command_buffer();
        boundary("encode_begin");
        self.last_stats = self
            .renderer
            .encode(command, &output, MetalOutputMode::Replace, &textures)
            .map_err(status)?;
        boundary("preview_commit_begin");
        command.commit();
        boundary("preview_submitted");
        Ok(SubmittedFrame {
            image: output,
            command: command.to_owned(),
        })
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use zipzi_editor_core::{Document, SceneTexture};

    fn pixels(width: u32, height: u32, color: u8) -> SceneTexture {
        SceneTexture {
            asset_id: color.to_string(),
            cache_key: color.to_string(),
            width,
            height,
            rgba: vec![color; width as usize * height as usize * 4].into(),
        }
    }

    #[test]
    fn preparation_batches_bound_workspace_and_isolate_an_oversized_image() {
        let images = [
            pixels(128, 128, 1),
            pixels(257, 257, 2),
            pixels(32, 32, 3),
            pixels(32, 32, 4),
        ];
        let pending: Vec<_> = images.iter().map(|s| (s.cache_key.clone(), s)).collect();
        let limit = 256 << 10;
        assert!(preparation_bytes(&images[0]) < limit);
        assert!(preparation_bytes(&images[1]) > limit);
        assert_eq!(batch_end(&pending, 0, limit), 1);
        assert_eq!(batch_end(&pending, 1, limit), 2);
        assert_eq!(batch_end(&pending, 2, limit), 4);
    }

    #[test]
    fn parallel_preparation_keeps_each_images_upload_bytes_in_request_order() {
        let images: Vec<_> = (0..8)
            .map(|i| pixels(257 + i, 257 - i, i as u8 * 31))
            .collect();
        let pending: Vec<_> = images.iter().map(|s| (s.cache_key.clone(), s)).collect();
        let prepared = prepare_texture_batch(&pending);
        assert_eq!(prepared.len(), images.len());
        for ((index, prepared), (expected_index, source)) in
            prepared.into_iter().zip(images.iter().enumerate())
        {
            assert_eq!(index, expected_index);
            assert_eq!(prepared.rgba, premultiply_rgba8(&source.rgba));
            let mut expected = straight_rgba_mipmaps(source.width, source.height, &source.rgba);
            for (_, _, rgba) in &mut expected {
                *rgba = premultiply_rgba8(rgba);
            }
            assert_eq!(prepared.mipmaps, expected);
        }
    }

    #[test]
    fn boundless_offscreen_artwork_renders_after_pan_and_zoom() -> Result<()> {
        objc2::rc::autoreleasepool(|_| {
            use zipzi_core::{
                Drawable,
                evaluation::{OffscreenFrame, RenderCommand},
            };
            use zipzi_editor_core::ViewportPoint;
            let device = metal::Device::system_default().expect("Metal device");
            let queue = device.new_command_queue();
            let config = MetalTargetConfig {
                width: 64,
                height: 64,
                format: metal::MTLPixelFormat::RGBA8Unorm,
            };
            let mut renderer = MetalSceneRenderer::new(device, config)?;
            let mut scene = Document::new_blank()?.snapshot()?;
            let frame = Arc::make_mut(&mut scene.viewport_frame);
            frame.drawables = vec![Drawable {
                id: "art".into(),
                texture_asset_id: "red".into(),
                positions: vec![
                    Vec2::new(990., -990.),
                    Vec2::new(1010., -990.),
                    Vec2::new(1010., -1010.),
                    Vec2::new(990., -1010.),
                ],
                uvs: Arc::from([
                    Vec2::new(0., 0.),
                    Vec2::new(1., 0.),
                    Vec2::new(1., 1.),
                    Vec2::new(0., 1.),
                ]),
                indices: Arc::from([0, 1, 2, 0, 2, 3]),
                ..Default::default()
            }];
            frame.offscreens = vec![OffscreenFrame {
                id: "group".into(),
                enabled: true,
                opacity: 0.5,
                multiply_color: [1.; 4],
                ..Default::default()
            }];
            frame.render_plan = vec![
                RenderCommand::BeginOffscreen {
                    offscreen_id: "group".into(),
                },
                RenderCommand::DrawMesh {
                    mesh_id: "art".into(),
                },
                RenderCommand::EndOffscreen {
                    offscreen_id: "group".into(),
                },
            ];
            scene.textures = Arc::from([SceneTexture {
                asset_id: "red".into(),
                cache_key: "red".into(),
                width: 1,
                height: 1,
                rgba: Arc::from([255, 0, 0, 255]),
            }]);
            for zoom in [1., 2.] {
                let transform = ViewportTransform::new(
                    ViewportMetrics {
                        width: 64.,
                        height: 64.,
                        scale_factor: 1.,
                    },
                    ViewState {
                        zoom,
                        pan: ViewportPoint {
                            x: -1000. * zoom,
                            y: -1000. * zoom,
                        },
                    },
                )
                .unwrap();
                let submitted = renderer.submit_transformed(&queue, config, &scene, transform)?;
                submitted.command.wait_until_completed();
                assert_eq!(
                    submitted.command.status(),
                    metal::MTLCommandBufferStatus::Completed
                );
                let pixels = zipzi_render_metal::read_rgba8(&submitted.image).map_err(status)?;
                let center = &pixels[(32 * 64 + 32) * 4..][..4];
                assert!(
                    center[0] >= 120 && center[1] == 0 && center[2] == 0,
                    "{center:?}"
                );
                assert!((120..=136).contains(&center[3]), "{center:?}");
                assert_eq!(&pixels[..4], &[0, 0, 0, 0]);
            }
            Ok(())
        })
    }

    #[test]
    fn texture_eviction_keeps_recent_and_active_sources_without_reuploads() -> Result<()> {
        objc2::rc::autoreleasepool(|_| {
            let device = metal::Device::system_default().expect("Metal device");
            let queue = device.new_command_queue();
            let config = MetalTargetConfig {
                width: 1,
                height: 1,
                format: metal::MTLPixelFormat::RGBA8Unorm,
            };
            let mut renderer = MetalSceneRenderer::new(device, config)?;
            renderer.cache_budget = 12; // Three 1x1 RGBA textures.
            let mut scene = Document::new_blank()?.snapshot()?;
            let mut draw = |ids: &[&str]| -> Result<(u64, u64, u64)> {
                scene.textures = ids
                    .iter()
                    .enumerate()
                    .map(|(slot, id)| SceneTexture {
                        asset_id: format!("{id}-{slot}"),
                        cache_key: (*id).into(),
                        width: 1,
                        height: 1,
                        rgba: Arc::from([255; 4]),
                    })
                    .collect::<Vec<_>>()
                    .into();
                let transform = ViewportTransform::new(
                    ViewportMetrics {
                        width: 1.,
                        height: 1.,
                        scale_factor: 1.,
                    },
                    ViewState::default(),
                )
                .unwrap();
                let submitted = renderer.submit_transformed(&queue, config, &scene, transform)?;
                submitted.command.wait_until_completed();
                assert_eq!(
                    submitted.command.status(),
                    metal::MTLCommandBufferStatus::Completed
                );
                Ok(renderer.stats())
            };
            assert_eq!(draw(&["a", "a", "b", "c"])?, (1, 3, 12));
            assert_eq!(draw(&["a", "b"])?, (2, 3, 12));
            assert_eq!(draw(&["a", "d", "e"])?, (3, 5, 12));
            // The current scene survives eviction even when it exceeds budget.
            assert_eq!(draw(&["a", "d", "e", "f"])?, (4, 6, 16));
            assert_eq!(draw(&["a", "d", "e", "f"])?, (5, 6, 16));
            assert_eq!(draw(&["a", "d"])?, (6, 6, 12));
            // The oldest source was evicted and must upload again.
            assert_eq!(draw(&["c"])?, (7, 7, 12));
            Ok(())
        })
    }

    #[test]
    #[ignore = "requires models/local/mao_pro.zipzi"]
    fn mao_high_zoom_crops_masks_with_stable_pixels_after_panning() -> anyhow::Result<()> {
        objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
            use zipzi_editor_core::ViewportPoint;
            use zipzi_render_metal::read_rgba8;

            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../models/local/mao_pro.zipzi")
                .canonicalize()?;
            let mut document = Document::open(&path)?;
            let scene = document.snapshot()?;
            let frame = &scene.viewport_frame;
            let anchors: Vec<_> = frame
                .drawables
                .iter()
                .filter(|drawable| {
                    drawable.visible
                        && drawable.opacity > 0.
                        && !drawable.masks.is_empty()
                        && !drawable.positions.is_empty()
                })
                .take(2)
                .map(|drawable| {
                    let count = drawable.positions.len() as f32;
                    let x = drawable.positions.iter().map(|p| p.x).sum::<f32>() / count;
                    let y = drawable.positions.iter().map(|p| p.y).sum::<f32>() / count;
                    ViewportPoint { x: x, y: y }
                })
                .collect();
            assert_eq!(anchors.len(), 2);
            let device = metal::Device::system_default().expect("Metal device");
            let queue = device.new_command_queue();
            let mut renderer = MetalSceneRenderer::new(device, MetalTargetConfig::default())?;
            let mut content_frames = 0;
            for (width, height) in [(1280, 800), (2560, 1600)] {
                let config = MetalTargetConfig {
                    width,
                    height,
                    format: metal::MTLPixelFormat::RGBA8Unorm,
                };
                let metrics = ViewportMetrics {
                    width: width as f32 / 2.,
                    height: height as f32 / 2.,
                    scale_factor: 2.,
                };
                for zoom in [1., 16., 20., 24., 32.] {
                    let capture = |renderer: &mut MetalSceneRenderer,
                                   pan: ViewportPoint|
                     -> anyhow::Result<Vec<u8>> {
                        let transform =
                            ViewportTransform::new(metrics, ViewState { zoom, pan }).unwrap();
                        let submitted =
                            renderer.submit_transformed(&queue, config, &scene, transform)?;
                        submitted.command.wait_until_completed();
                        assert_eq!(
                            submitted.command.status(),
                            metal::MTLCommandBufferStatus::Completed
                        );
                        let stats = renderer.last_frame_stats();
                        assert!(
                            stats.attachment_budget_bytes
                                <= zipzi_render::OFFSCREEN_BUDGET_BYTES as u64
                        );
                        assert_eq!(
                            stats.mask_resolution_limit, 4096,
                            "Mao should retain full visible density"
                        );
                        read_rgba8(&submitted.image)
                            .map_err(|error| anyhow::anyhow!("{}: {}", error.code, error.message))
                    };
                    let first = capture(&mut renderer, ViewportPoint::default())?;
                    assert_eq!(first, capture(&mut renderer, ViewportPoint::default())?);
                    assert_eq!(
                        renderer.last_frame_stats().masks,
                        0,
                        "Warm frames reuse cropped masks"
                    );
                    let mut peak = renderer.last_frame_stats().attachment_budget_bytes;
                    let mut covered = first.chunks_exact(4).filter(|pixel| pixel[3] != 0).count();
                    for anchor in &anchors {
                        let pixels = capture(
                            &mut renderer,
                            ViewportPoint {
                                x: -anchor.x * zoom,
                                y: anchor.y * zoom,
                            },
                        )?;
                        covered += pixels.chunks_exact(4).filter(|pixel| pixel[3] != 0).count();
                        peak = peak.max(renderer.last_frame_stats().attachment_budget_bytes);
                    }
                    assert_eq!(
                        first,
                        capture(&mut renderer, ViewportPoint::default())?,
                        "Returning from a pan must restore identical pixels"
                    );
                    if covered > 0 {
                        content_frames += 1;
                    }
                    println!(
                        "Mao {width}x{height}, {:.0}%: peak reservation={:.2} MiB, covered pixels={covered}",
                        zoom * 100.,
                        peak as f64 / 1024. / 1024.
                    );
                }
            }
            assert_eq!(content_frames, 10, "Every zoom must render actual artwork");
            assert_eq!(
                renderer.stats().1,
                scene.textures.len() as u64,
                "Zoom and pan must not re-upload textures"
            );
            Ok(())
        })
    }
}

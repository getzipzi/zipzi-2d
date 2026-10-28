//! A reusable device and renderer for offscreen observation.
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use sha2::{Digest, Sha256};
use zipzi_core::Vec2;
use zipzi_render::{texture::straight_rgba_mipmaps, Affine2, ViewportConfig};
#[cfg(all(target_os = "macos", not(feature = "wgpu-backend")))]
use zipzi_render_metal::{
    metal, read_rgba8, MetalContext, MetalOutputMode as BackendOutputMode,
    MetalRenderer as BackendRenderer, MetalTargetConfig as BackendTargetConfig,
    MetalTexture as BackendTexture, MetalTextureCatalog as BackendTextureCatalog,
};
#[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
use zipzi_render_wgpu::{
    WgpuContext, WgpuEncodeTarget, WgpuOutputMode as BackendOutputMode,
    WgpuRenderer as BackendRenderer, WgpuTargetConfig as BackendTargetConfig,
    WgpuTexture as BackendTexture, WgpuTextureCatalog as BackendTextureCatalog,
};

use crate::{ObservationError, RenderRequest, ResolvedObservation, ResolvedTexture, ViewMapping};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObserverConfig {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedTexture {
    pub asset_id: String,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrawableBounds {
    pub id: String,
    pub visible: bool,
    /// Clipped pixel bounds with exclusive maximum coordinates.
    pub bounds: Option<(u32, u32, u32, u32)>,
}

#[derive(Clone, Debug)]
pub struct ObservedFrame {
    /// Hash of the evaluated frame, validated texture content, and output view.
    pub input_sha256: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub evaluation_revision: u64,
    pub document_id: String,
    pub parameters: zipzi_core::PreviewValues,
    pub view: ViewMapping,
    pub drawable_bounds: Vec<DrawableBounds>,
    pub textures: Vec<ObservedTexture>,
    pub adapter_name: String,
    pub adapter_backend: String,
}

impl ObservedFrame {
    pub fn png_bytes(&self) -> Result<Vec<u8>, ObservationError> {
        let mut output = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut output, self.width, self.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .and_then(|mut writer| writer.write_image_data(&self.rgba))
                .map_err(|failure| error("PNG_ENCODE", failure.to_string()))?;
        }
        Ok(output)
    }
}

struct OwnedTexture {
    #[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
    _texture: wgpu::Texture,
    #[cfg(all(target_os = "macos", not(feature = "wgpu-backend")))]
    view: metal::Texture,
    #[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    sha256: String,
    revision: u64,
}

struct DigestWriter<'a>(&'a mut Sha256);

impl std::fmt::Write for DigestWriter<'_> {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.0.update(value.as_bytes());
        Ok(())
    }
}

pub struct Observer {
    #[cfg(all(target_os = "macos", not(feature = "wgpu-backend")))]
    context: MetalContext,
    #[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
    context: WgpuContext,
    renderer: BackendRenderer,
    textures: HashMap<String, OwnedTexture>,
    next_texture_revision: u64,
    adapter_name: String,
    adapter_backend: String,
}

fn error(code: &str, message: impl Into<String>) -> ObservationError {
    ObservationError {
        code: code.into(),
        message: message.into(),
        asset_id: None,
    }
}

impl Observer {
    pub fn new(config: ObserverConfig) -> Result<Self, ObservationError> {
        if config.width == 0 || config.height == 0 {
            return Err(error("INVALID_VIEW", "Output dimensions must be positive"));
        }
        #[cfg(all(target_os = "macos", not(feature = "wgpu-backend")))]
        let (context, renderer, adapter_name) = {
            let context =
                MetalContext::new().map_err(|failure| error("GPU_ADAPTER", failure.to_string()))?;
            let limit = context.max_texture_dimension();
            if config.width > limit || config.height > limit {
                return Err(error(
                    "OUTPUT_SIZE_LIMIT",
                    format!("Output size exceeds the device limit {limit}x{limit}"),
                ));
            }
            let renderer = BackendRenderer::new(
                &context,
                BackendTargetConfig {
                    width: config.width,
                    height: config.height,
                    format: metal::MTLPixelFormat::RGBA8Unorm,
                },
            )
            .map_err(|status| error(&status.code, status.message))?;
            let adapter_name = context.name().to_owned();
            (context, renderer, adapter_name)
        };
        #[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
        let (context, renderer, adapter_name, adapter_backend) = {
            let context =
                WgpuContext::new().map_err(|status| error(&status.code, status.message))?;
            let limit = context.device().limits().max_texture_dimension_2d;
            if config.width > limit || config.height > limit {
                return Err(error(
                    "OUTPUT_SIZE_LIMIT",
                    format!("Output size exceeds the device limit {limit}x{limit}"),
                ));
            }
            let renderer = BackendRenderer::new(
                context.device(),
                BackendTargetConfig {
                    width: config.width,
                    height: config.height,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                },
            )
            .map_err(|status| error(&status.code, status.message))?;
            let adapter_name = context.adapter_name().to_owned();
            let adapter_backend = context.adapter_backend().to_owned();
            (context, renderer, adapter_name, adapter_backend)
        };
        Ok(Self {
            context,
            renderer,
            textures: HashMap::new(),
            next_texture_revision: 1,
            adapter_name,
            #[cfg(all(target_os = "macos", not(feature = "wgpu-backend")))]
            adapter_backend: "Metal".into(),
            #[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
            adapter_backend,
        })
    }

    fn upload_textures(&mut self, resolved: &[ResolvedTexture]) -> Result<(), ObservationError> {
        #[cfg(all(target_os = "macos", not(feature = "wgpu-backend")))]
        let limit = self.context.max_texture_dimension();
        #[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
        let limit = self.context.device().limits().max_texture_dimension_2d;
        for texture in resolved {
            let width = texture.data.width;
            let height = texture.data.height;
            if width == 0 || height == 0 || width > limit || height > limit {
                return Err(ObservationError {
                    code: "TEXTURE_SIZE_LIMIT".into(),
                    message: format!(
                        "Texture {width}x{height} exceeds the device limit {limit}x{limit}"
                    ),
                    asset_id: Some(texture.asset.id.clone()),
                });
            }
        }
        let required: HashSet<_> = resolved
            .iter()
            .map(|texture| texture.asset.id.as_str())
            .collect();
        self.textures.retain(|id, _| required.contains(id.as_str()));
        for texture in resolved {
            let id = texture.asset.id.clone();
            if self.textures.get(&id).is_some_and(|cached| {
                cached.sha256 == texture.data.sha256
                    && cached.width == texture.data.width
                    && cached.height == texture.data.height
            }) {
                continue;
            }
            let mipmaps =
                straight_rgba_mipmaps(texture.data.width, texture.data.height, &texture.data.rgba);
            #[cfg(all(target_os = "macos", not(feature = "wgpu-backend")))]
            let view = self
                .context
                .upload_rgba8(
                    texture.data.width,
                    texture.data.height,
                    &texture.data.rgba,
                    &mipmaps,
                )
                .map_err(|status| error(&status.code, status.message))?;
            #[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
            let (source, view) = {
                let source = self
                    .context
                    .upload_rgba8(
                        texture.data.width,
                        texture.data.height,
                        &texture.data.rgba,
                        &mipmaps,
                    )
                    .map_err(|status| error(&status.code, status.message))?;
                let view = source.create_view(&wgpu::TextureViewDescriptor::default());
                (source, view)
            };
            let revision = self.next_texture_revision;
            self.next_texture_revision += 1;
            self.textures.insert(
                id,
                OwnedTexture {
                    #[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
                    _texture: source,
                    view,
                    width: texture.data.width,
                    height: texture.data.height,
                    sha256: texture.data.sha256.clone(),
                    revision,
                },
            );
        }
        Ok(())
    }

    /// Render frozen texture content at an explicit model-space view.
    pub fn render(
        &mut self,
        captured: &ResolvedObservation,
        request: RenderRequest,
    ) -> Result<ObservedFrame, ObservationError> {
        let input = &captured.input;
        let resolved = &captured.textures;
        let view = request.mapping()?;
        let (width, height) = (request.width, request.height);
        #[cfg(all(target_os = "macos", not(feature = "wgpu-backend")))]
        let limit = self.context.max_texture_dimension();
        #[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
        let limit = self.context.device().limits().max_texture_dimension_2d;
        if width > limit || height > limit || width > 4096 || height > 4096 {
            return Err(error(
                "OUTPUT_SIZE_LIMIT",
                format!("Output size exceeds the inspection or device limit {limit}x{limit}"),
            ));
        }
        let mut digest = Sha256::new();
        {
            let mut digest_writer = DigestWriter(&mut digest);
            let texture_hashes = resolved
                .iter()
                .map(|texture| (&texture.asset.id, &texture.data.sha256))
                .collect::<Vec<_>>();
            write!(
                digest_writer,
                "zipzi-observation-explicit-v1:{:?}:{:?}",
                input.frame, texture_hashes,
            )
            .expect("digest writer is infallible");
        }
        digest.update(b"explicit-roi-v1");
        digest.update(request.width.to_le_bytes());
        digest.update(request.height.to_le_bytes());
        for value in [
            request.roi.x0,
            request.roi.y0,
            request.roi.x1,
            request.roi.y1,
            request.padding_model,
        ] {
            digest.update(value.to_bits().to_le_bytes());
        }
        digest.update(b"source-texture:linear-mipmap-linear-repeat-area-v2");
        let mut input_sha256 = String::with_capacity(64);
        for byte in digest.finalize() {
            use std::fmt::Write;
            write!(&mut input_sha256, "{byte:02x}").unwrap();
        }
        self.upload_textures(resolved)?;
        let target = BackendTargetConfig {
            width,
            height,
            #[cfg(all(target_os = "macos", not(feature = "wgpu-backend")))]
            format: metal::MTLPixelFormat::RGBA8Unorm,
            #[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
            format: wgpu::TextureFormat::Rgba8Unorm,
        };
        #[cfg(all(target_os = "macos", not(feature = "wgpu-backend")))]
        self.renderer
            .resize(target)
            .map_err(|status| error(&status.code, status.message))?;
        #[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
        self.renderer
            .resize(self.context.device(), target)
            .map_err(|status| error(&status.code, status.message))?;
        let mut catalog = BackendTextureCatalog::new(
            self.textures
                .iter()
                .map(|(id, texture)| {
                    (
                        id.clone(),
                        BackendTexture {
                            view: &texture.view,
                            width: texture.width,
                            height: texture.height,
                        },
                    )
                })
                .collect(),
        );
        for (id, texture) in &self.textures {
            catalog.set_revision(id.clone(), texture.revision);
            catalog.set_repeat(id.clone(), true);
        }
        let status = |s: zipzi_core::Status| error(&s.code, s.message);
        #[cfg(all(target_os = "macos", not(feature = "wgpu-backend")))]
        self.renderer
            .sync_model(&input.frame, &catalog)
            .map_err(status)?;
        #[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
        self.renderer
            .sync_model(self.context.device(), &input.frame, &catalog)
            .map_err(status)?;
        let (scale, offset_x, offset_y) = (view.scale, view.offset.0, view.offset.1);
        let drawable_bounds = input
            .frame
            .drawables
            .iter()
            .map(|drawable| {
                let visible = drawable.enabled && drawable.visible && drawable.opacity > 0.0;
                let bounds = if visible && !drawable.positions.is_empty() {
                    let mut min_x = f32::INFINITY;
                    let mut min_y = f32::INFINITY;
                    let mut max_x = f32::NEG_INFINITY;
                    let mut max_y = f32::NEG_INFINITY;
                    for position in &drawable.positions {
                        let x = position.x * scale + offset_x;
                        let y = -position.y * scale + offset_y;
                        min_x = min_x.min(x);
                        min_y = min_y.min(y);
                        max_x = max_x.max(x);
                        max_y = max_y.max(y);
                    }
                    let x0 = (min_x.floor() - 2.0).clamp(0.0, width as f32) as u32;
                    let y0 = (min_y.floor() - 2.0).clamp(0.0, height as f32) as u32;
                    let x1 = (max_x.ceil() + 2.0).clamp(0.0, width as f32) as u32;
                    let y1 = (max_y.ceil() + 2.0).clamp(0.0, height as f32) as u32;
                    (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
                } else {
                    None
                };
                DrawableBounds {
                    id: drawable.id.clone(),
                    visible,
                    bounds,
                }
            })
            .collect();
        let rgba = render_sample(
            &mut self.renderer,
            &self.context,
            &catalog,
            width,
            height,
            scale,
            offset_x,
            offset_y,
        )?;
        let mut textures: Vec<_> = self
            .textures
            .iter()
            .map(|(id, texture)| ObservedTexture {
                asset_id: id.clone(),
                sha256: texture.sha256.clone(),
            })
            .collect();
        textures.sort_by(|a, b| a.asset_id.cmp(&b.asset_id));
        Ok(ObservedFrame {
            input_sha256,
            width,
            height,
            rgba,
            evaluation_revision: input.evaluation_revision,
            document_id: input.document_id.clone(),
            parameters: input
                .frame
                .parameters
                .iter()
                .map(|parameter| (parameter.id.clone(), parameter.value))
                .collect(),
            view,
            drawable_bounds,
            textures,
            adapter_name: self.adapter_name.clone(),
            adapter_backend: self.adapter_backend.clone(),
        })
    }
}

#[cfg(all(target_os = "macos", not(feature = "wgpu-backend")))]
#[allow(clippy::too_many_arguments)]
fn render_sample(
    renderer: &mut BackendRenderer,
    context: &MetalContext,
    catalog: &BackendTextureCatalog<'_>,
    width: u32,
    height: u32,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> Result<Vec<u8>, ObservationError> {
    // Drain native command buffers even on Python/Rust worker threads.
    metal::objc::rc::autoreleasepool(|| {
        let view = ViewportConfig {
            transform: Affine2 {
                a: Vec2::new(scale, 0.0),
                b: Vec2::new(0.0, -scale),
                origin: Vec2::new(offset_x, offset_y),
            },
            target_extent: Vec2::new(width as f32, height as f32),
            mask_scale: scale as f64,
        };
        let status = |s: zipzi_core::Status| error(&s.code, s.message);
        renderer.update_view(view).map_err(status)?;
        let target = BackendTargetConfig {
            width,
            height,
            format: metal::MTLPixelFormat::RGBA8Unorm,
        };
        let output = context.output_texture(target).map_err(status)?;
        let command = context.queue().new_command_buffer();
        renderer
            .encode(command, &output, BackendOutputMode::Replace, catalog)
            .map_err(status)?;
        command.commit();
        command.wait_until_completed();
        if command.status() != metal::MTLCommandBufferStatus::Completed {
            return Err(error("GPU_SUBMIT", "Metal command buffer failed"));
        }
        read_rgba8(&output).map_err(status)
    })
}

#[cfg(any(not(target_os = "macos"), feature = "wgpu-backend"))]
#[allow(clippy::too_many_arguments)]
fn render_sample(
    renderer: &mut BackendRenderer,
    context: &WgpuContext,
    catalog: &BackendTextureCatalog<'_>,
    width: u32,
    height: u32,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
) -> Result<Vec<u8>, ObservationError> {
    let view = ViewportConfig {
        transform: Affine2 {
            a: Vec2::new(scale, 0.0),
            b: Vec2::new(0.0, -scale),
            origin: Vec2::new(offset_x, offset_y),
        },
        target_extent: Vec2::new(width as f32, height as f32),
        mask_scale: scale as f64,
    };
    let status = |s: zipzi_core::Status| error(&s.code, s.message);
    renderer
        .update_view(context.device(), view)
        .map_err(status)?;
    let output = context
        .output_texture(BackendTargetConfig {
            width,
            height,
            format: wgpu::TextureFormat::Rgba8Unorm,
        })
        .map_err(status)?;
    let output_view = output.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("observe.encode"),
        });
    renderer
        .encode(
            WgpuEncodeTarget {
                device: context.device(),
                queue: context.queue(),
                encoder: &mut encoder,
                output: &output_view,
                output_mode: BackendOutputMode::Replace,
            },
            catalog,
        )
        .map_err(status)?;
    context.queue().submit([encoder.finish()]);
    context.read_rgba8(&output).map_err(status)
}

#[cfg(test)]
mod tests {
    use super::straight_rgba_mipmaps;

    #[test]
    fn mipmaps_include_odd_edges_and_single_pixel_axes() {
        for (width, height) in [(3, 1), (1, 3), (3, 3), (5, 1)] {
            let mut rgba = vec![0; width * height * 4];
            rgba[(width * height - 1) * 4..].fill(255);
            let levels = straight_rgba_mipmaps(width as u32, height as u32, &rgba);
            let last = levels.last().unwrap();
            assert_eq!((last.0, last.1), (1, 1));
            assert_eq!(
                last.2,
                vec![
                    255,
                    255,
                    255,
                    (255.0 / (width * height) as f64).round() as u8
                ]
            );
        }
    }
}

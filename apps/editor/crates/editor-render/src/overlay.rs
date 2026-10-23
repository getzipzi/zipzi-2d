//! Immutable mesh-overlay textures on the consumer's Metal queue.
use anyhow::{Result, ensure};
use editor_core::{OverlaySnapshot, OverlayVertexKind};
use kasane_render_metal::metal::*;

/// Shared screen-space radius for filled and hollow editing markers.
pub const OVERLAY_MARKER_RADIUS: f32 = 3.;

/// Separate textures let the canvas insert control lines beneath all markers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlayPass {
    All,
    Edges,
    Vertices,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayStyle {
    /// Straight RGBA; shaders produce premultiplied output.
    pub edge: [f32; 4],
    pub vertex: [f32; 4],
    pub selected: [f32; 4],
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayViewport {
    pub extent: [u32; 2],
    pub scale: f32,
    /// Logical window origin; the texture starts at floor(origin * scale).
    pub origin: [f32; 2],
}
#[repr(C)]
struct Uniforms {
    viewport: [f32; 2],
    scale: f32,
    _padding: f32,
    origin: [f32; 2],
    texture_origin: [f32; 2],
    edge: [f32; 4],
    vertex: [f32; 4],
    selected: [f32; 4],
}
#[derive(PartialEq)]
struct Key {
    viewport: OverlayViewport,
    style: OverlayStyle,
    edges: Vec<[f32; 4]>,
    markers: Vec<[f32; 4]>,
}

pub struct MetalOverlayRenderer {
    device: Device,
    queue: Option<CommandQueue>,
    edges: RenderPipelineState,
    markers: RenderPipelineState,
    cached: [Option<(Key, Texture)>; 3],
}
impl MetalOverlayRenderer {
    pub fn new(device: Device) -> Result<Self> {
        let options = CompileOptions::new();
        options.set_fast_math_enabled(false);
        let library = device
            .new_library_with_source(include_str!("overlay.metal"), &options)
            .map_err(anyhow::Error::msg)?;
        let pipeline = |vertex: &str, fragment: &str, samples| -> Result<RenderPipelineState> {
            let desc = RenderPipelineDescriptor::new();
            let v = library
                .get_function(vertex, None)
                .map_err(anyhow::Error::msg)?;
            let f = library
                .get_function(fragment, None)
                .map_err(anyhow::Error::msg)?;
            desc.set_vertex_function(Some(&v));
            desc.set_fragment_function(Some(&f));
            desc.set_raster_sample_count(samples);
            let color = desc.color_attachments().object_at(0).unwrap();
            color.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
            color.set_blending_enabled(true);
            color.set_source_rgb_blend_factor(MTLBlendFactor::One);
            color.set_source_alpha_blend_factor(MTLBlendFactor::One);
            color.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
            color.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
            device
                .new_render_pipeline_state(&desc)
                .map_err(anyhow::Error::msg)
        };
        let edges = pipeline("edge_vertex", "solid_fragment", 4)?;
        let markers = pipeline("marker_vertex", "marker_fragment", 1)?;
        Ok(Self {
            device,
            queue: None,
            edges,
            markers,
            cached: [None, None, None],
        })
    }
    pub fn uses_device(&self, device: &DeviceRef) -> bool {
        std::ptr::eq(self.device.as_ref(), device)
    }
    /// Canvas-local logical coordinates. Never modifies a published texture or
    /// in-flight buffer and never waits on the UI thread. Commands retain inputs.
    /// The caller must use the same queue for subsequent GPUI composition.
    pub fn draw(
        &mut self,
        queue: &CommandQueueRef,
        viewport: OverlayViewport,
        overlay: &OverlaySnapshot,
        style: OverlayStyle,
    ) -> Result<Option<Texture>> {
        self.draw_pass(queue, viewport, overlay, style, OverlayPass::All)
    }
    pub fn draw_pass(
        &mut self,
        queue: &CommandQueueRef,
        viewport: OverlayViewport,
        overlay: &OverlaySnapshot,
        style: OverlayStyle,
        pass: OverlayPass,
    ) -> Result<Option<Texture>> {
        let OverlayViewport {
            extent,
            scale,
            origin,
        } = viewport;
        ensure!(
            self.uses_device(queue.device()),
            "Overlay queue/device mismatch"
        );
        ensure!(
            extent.iter().all(|v| *v > 0 && *v <= 16384)
                && scale.is_finite()
                && scale > 0.
                && origin.iter().all(|v| v.is_finite()),
            "Invalid overlay viewport"
        );
        // A new window can expose another queue on the same device. Re-render
        // there rather than reusing an image whose producer is on the old queue.
        if self
            .queue
            .as_ref()
            .is_none_or(|old| !std::ptr::eq(old.as_ref(), queue))
        {
            self.cached = [None, None, None];
            self.queue = Some(queue.to_owned());
        }
        let slot = match pass {
            OverlayPass::All => 0,
            OverlayPass::Edges => 1,
            OverlayPass::Vertices => 2,
        };
        let lines = if pass == OverlayPass::Vertices {
            &[][..]
        } else {
            &overlay.lines
        };
        let vertices = if pass == OverlayPass::Edges {
            &[][..]
        } else {
            &overlay.vertices
        };
        let key = Key {
            viewport,
            style,
            edges: lines
                .iter()
                .filter(|(a, b)| [a.x, a.y, b.x, b.y].iter().all(|v| v.is_finite()))
                .map(|(a, b)| [a.x, a.y, b.x, b.y])
                .collect(),
            markers: [false, true]
                .into_iter()
                .flat_map(|emphasized| {
                    vertices
                        .iter()
                        .filter(move |v| (v.selected || v.hovered) == emphasized)
                })
                .filter(|v| v.kind == OverlayVertexKind::Mesh)
                .filter(|v| v.position.x.is_finite() && v.position.y.is_finite())
                .map(|v| {
                    [
                        v.position.x,
                        v.position.y,
                        OVERLAY_MARKER_RADIUS,
                        (u8::from(v.selected) + 2 * u8::from(v.hovered)) as f32,
                    ]
                })
                .collect(),
        };
        if key.edges.is_empty() && key.markers.is_empty() {
            self.cached[slot] = None;
            return Ok(None);
        }
        if let Some((old, image)) = &self.cached[slot] {
            if old == &key {
                return Ok(Some(image.clone()));
            }
        }
        let uniforms = Uniforms {
            viewport: [extent[0] as f32, extent[1] as f32],
            scale,
            _padding: 0.,
            origin,
            texture_origin: origin.map(|v| (v * scale).floor()),
            edge: style.edge,
            vertex: style.vertex,
            selected: style.selected,
        };
        let td = TextureDescriptor::new();
        td.set_width(extent[0] as u64);
        td.set_height(extent[1] as u64);
        td.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        td.set_storage_mode(MTLStorageMode::Private);
        td.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        let image = self.device.new_texture(&td);
        td.set_texture_type(MTLTextureType::D2Multisample);
        td.set_sample_count(4);
        td.set_storage_mode(if self.device.supports_family(MTLGPUFamily::Apple1) {
            MTLStorageMode::Memoryless
        } else {
            MTLStorageMode::Private
        });
        td.set_usage(MTLTextureUsage::RenderTarget);
        let msaa = (!key.edges.is_empty()).then(|| self.device.new_texture(&td));
        let command = queue.new_command_buffer();
        let pass = RenderPassDescriptor::new();
        let color = pass.color_attachments().object_at(0).unwrap();
        color.set_texture(msaa.as_deref());
        color.set_resolve_texture(Some(&image));
        color.set_load_action(MTLLoadAction::Clear);
        color.set_clear_color(MTLClearColor::new(0., 0., 0., 0.));
        color.set_store_action(MTLStoreAction::MultisampleResolve);
        let encode = |pass: &RenderPassDescriptorRef,
                      pipeline: &RenderPipelineStateRef,
                      instances: &[[f32; 4]]| {
            let encoder = command.new_render_command_encoder(pass);
            encoder.set_render_pipeline_state(pipeline);
            encoder.set_vertex_bytes(
                1,
                std::mem::size_of::<Uniforms>() as u64,
                (&uniforms as *const Uniforms).cast(),
            );
            if !instances.is_empty() {
                let buffer = self.device.new_buffer_with_data(
                    instances.as_ptr().cast(),
                    std::mem::size_of_val(instances) as u64,
                    MTLResourceOptions::StorageModeShared,
                );
                encoder.set_vertex_buffer(0, Some(&buffer), 0);
                encoder.draw_primitives_instanced(
                    MTLPrimitiveType::Triangle,
                    0,
                    6,
                    instances.len() as u64,
                );
            }
            encoder.end_encoding();
        };
        if msaa.is_some() {
            encode(pass, &self.edges, &key.edges);
        }
        if !key.markers.is_empty() {
            let pass = RenderPassDescriptor::new();
            let color = pass.color_attachments().object_at(0).unwrap();
            color.set_texture(Some(&image));
            color.set_load_action(if msaa.is_some() {
                MTLLoadAction::Load
            } else {
                MTLLoadAction::Clear
            });
            color.set_clear_color(MTLClearColor::new(0., 0., 0., 0.));
            color.set_store_action(MTLStoreAction::Store);
            encode(pass, &self.markers, &key.markers);
        }
        command.commit();
        self.cached[slot] = Some((key, image.clone()));
        Ok(Some(image))
    }
}

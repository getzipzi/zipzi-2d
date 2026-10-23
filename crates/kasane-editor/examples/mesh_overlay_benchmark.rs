//! Compare production GPUI edge batching against an experimental Metal overlay.
//! Headless, one frame in flight, same device/queue, no timed readback/presentation.
use anyhow::{Context as _, ensure};
use editor_core::ViewportPoint;
use gpui_apple::metal_renderer::MetalRenderer;
use gpui_kit::{
    Bounds, ContentMask, DevicePixels, PaintSurface, Pixels, PrimitiveBatch, Quad, Scene,
    SurfaceSource, point, px, rgba, size,
};
use metal::*;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{BufWriter, Write},
    path::PathBuf,
    time::Instant,
};
#[path = "../src/mesh_edges.rs"]
mod mesh_edges;

const SCALE: f32 = 2.;
const COLOR: u32 = 0xa0b0c073;
const WARMUP: usize = 10;
const METHODS: [&str; 3] = ["gpui_paths", "metal_texture", "metal_full_overlay"];
fn method_order(sample: usize) -> [usize; 3] {
    [
        [0, 1, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
        [1, 0, 2],
        [0, 2, 1],
    ][sample % 6]
}

#[repr(C)]
struct Uniforms {
    viewport: [f32; 2],
    stretch: [f32; 2],
    translation: [f32; 2],
    half_width: f32,
    frame: u32,
    color: [f32; 4],
    clip: [f32; 4],
}
impl Uniforms {
    fn new(width: u32, height: u32, sample: usize, mode: &str) -> Self {
        let t = if mode == "view" { sample as f32 } else { 0. };
        let zoom = if mode == "view" {
            0.85 + 0.12 * (t * 0.09).sin()
        } else {
            0.9
        };
        Self {
            viewport: [width as f32, height as f32],
            stretch: [width as f32 * zoom, height as f32 * zoom],
            translation: [
                width as f32 * (1. - zoom) * 0.5 + (t * 0.13).sin() * 18. * SCALE,
                height as f32 * (1. - zoom) * 0.5 + (t * 0.07).cos() * 12. * SCALE,
            ],
            half_width: 0.5 * SCALE,
            frame: sample as u32,
            color: [160. / 255., 176. / 255., 192. / 255., 115. / 255.],
            clip: [
                16. * SCALE,
                16. * SCALE,
                width as f32 - 16. * SCALE,
                height as f32 - 16. * SCALE,
            ],
        }
    }
    fn transform(&self, p: [f32; 2]) -> ViewportPoint {
        ViewportPoint {
            x: (p[0] * self.stretch[0] + self.translation[0]) / SCALE,
            y: (p[1] * self.stretch[1] + self.translation[1]) / SCALE,
        }
    }
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        Bounds::new(
            point(px(0.), px(0.)),
            size(px(self.viewport[0] / SCALE), px(self.viewport[1] / SCALE)),
        )
    }
    fn mask(&self) -> ContentMask<Pixels> {
        ContentMask {
            bounds: Bounds::new(
                point(px(self.clip[0] / SCALE), px(self.clip[1] / SCALE)),
                size(
                    px((self.clip[2] - self.clip[0]) / SCALE),
                    px((self.clip[3] - self.clip[1]) / SCALE),
                ),
            ),
        }
    }
}

struct Mesh {
    base: Vec<[f32; 2]>,
    positions: Vec<[f32; 2]>,
    edges: Vec<[u32; 2]>,
    grid: usize,
}
impl Mesh {
    fn grid(n: usize) -> Self {
        let mut positions = Vec::new();
        for y in 0..=n {
            for x in 0..=n {
                positions.push([x as f32 / n as f32, y as f32 / n as f32]);
            }
        }
        let mut edges = Vec::new();
        for y in 0..=n {
            for x in 0..=n {
                let a = (y * (n + 1) + x) as u32;
                if x < n {
                    edges.push([a, a + 1]);
                }
                if y < n {
                    edges.push([a, a + n as u32 + 1]);
                }
                if x < n && y < n {
                    edges.push([a, a + n as u32 + 2]);
                }
            }
        }
        edges.sort_unstable();
        Self {
            base: positions.clone(),
            positions,
            edges,
            grid: n,
        }
    }
    fn set_frame(&mut self, sample: usize, mode: &str) {
        for (i, (dst, src)) in self.positions.iter_mut().zip(&self.base).enumerate() {
            let offset = if mode == "deform" {
                (sample as f32 * 0.11 + i as f32 * 0.37).sin() * 0.12 / self.grid as f32
            } else {
                0.
            };
            *dst = [src[0] + offset, src[1] - offset];
        }
    }
}

struct Overlay {
    pipeline: RenderPipelineState,
    handle_pipeline: RenderPipelineState,
    positions: Buffer,
    edges: Buffer,
    texture: Texture,
    msaa: Texture,
    edge_count: usize,
    vertex_count: usize,
}
impl Overlay {
    fn new(device: &DeviceRef, mesh: &Mesh, width: u32, height: u32) -> anyhow::Result<Self> {
        let options = CompileOptions::new();
        options.set_fast_math_enabled(false);
        let library = device
            .new_library_with_source(include_str!("mesh_overlay/edges.metal"), &options)
            .map_err(anyhow::Error::msg)?;
        let desc = RenderPipelineDescriptor::new();
        let vertex = library
            .get_function("edge_vertex", None)
            .map_err(anyhow::Error::msg)?;
        let fragment = library
            .get_function("edge_fragment", None)
            .map_err(anyhow::Error::msg)?;
        desc.set_vertex_function(Some(vertex.as_ref()));
        desc.set_fragment_function(Some(fragment.as_ref()));
        desc.set_raster_sample_count(4);
        let color = desc.color_attachments().object_at(0).unwrap();
        color.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        color.set_blending_enabled(true);
        color.set_source_rgb_blend_factor(MTLBlendFactor::One);
        color.set_source_alpha_blend_factor(MTLBlendFactor::One);
        color.set_destination_rgb_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
        color.set_destination_alpha_blend_factor(MTLBlendFactor::OneMinusSourceAlpha);
        let pipeline = device
            .new_render_pipeline_state(&desc)
            .map_err(anyhow::Error::msg)?;
        let handle_vertex = library
            .get_function("handle_vertex", None)
            .map_err(anyhow::Error::msg)?;
        let handle_fragment = library
            .get_function("handle_fragment", None)
            .map_err(anyhow::Error::msg)?;
        desc.set_vertex_function(Some(handle_vertex.as_ref()));
        desc.set_fragment_function(Some(handle_fragment.as_ref()));
        // GPUI's square, borderless markers use single-sample rasterization.
        desc.set_raster_sample_count(1);
        let handle_pipeline = device
            .new_render_pipeline_state(&desc)
            .map_err(anyhow::Error::msg)?;
        let positions = device.new_buffer_with_data(
            mesh.positions.as_ptr().cast(),
            std::mem::size_of_val(mesh.positions.as_slice()) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        let edges = device.new_buffer_with_data(
            mesh.edges.as_ptr().cast(),
            std::mem::size_of_val(mesh.edges.as_slice()) as u64,
            MTLResourceOptions::StorageModeShared,
        );
        let td = TextureDescriptor::new();
        td.set_width(width as u64);
        td.set_height(height as u64);
        td.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        td.set_storage_mode(MTLStorageMode::Private);
        td.set_usage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
        let texture = device.new_texture(&td);
        td.set_texture_type(MTLTextureType::D2Multisample);
        td.set_sample_count(4);
        td.set_storage_mode(if device.supports_family(MTLGPUFamily::Apple1) {
            MTLStorageMode::Memoryless
        } else {
            MTLStorageMode::Private
        });
        let msaa = device.new_texture(&td);
        Ok(Self {
            pipeline,
            handle_pipeline,
            positions,
            edges,
            texture,
            msaa,
            edge_count: mesh.edges.len(),
            vertex_count: mesh.positions.len(),
        })
    }
    fn upload(&self, mesh: &Mesh) {
        // The runner waits for the previous composition before reusing this buffer.
        unsafe {
            std::ptr::copy_nonoverlapping(
                mesh.positions.as_ptr().cast::<u8>(),
                self.positions.contents().cast::<u8>(),
                std::mem::size_of_val(mesh.positions.as_slice()),
            );
        }
    }
    fn submit(&self, queue: &CommandQueueRef, u: &Uniforms, handles: bool) -> CommandBuffer {
        let command = queue.new_command_buffer();
        let pass = RenderPassDescriptor::new();
        let color = pass.color_attachments().object_at(0).unwrap();
        color.set_texture(Some(&self.msaa));
        color.set_resolve_texture(Some(&self.texture));
        color.set_load_action(MTLLoadAction::Clear);
        color.set_store_action(MTLStoreAction::MultisampleResolve);
        color.set_clear_color(MTLClearColor::new(0., 0., 0., 0.));
        let encoder = command.new_render_command_encoder(pass);
        encoder.set_render_pipeline_state(&self.pipeline);
        encoder.set_vertex_buffer(0, Some(&self.positions), 0);
        encoder.set_vertex_buffer(1, Some(&self.edges), 0);
        encoder.set_vertex_bytes(
            2,
            std::mem::size_of::<Uniforms>() as u64,
            (u as *const Uniforms).cast(),
        );
        encoder.set_fragment_bytes(
            0,
            std::mem::size_of::<Uniforms>() as u64,
            (u as *const Uniforms).cast(),
        );
        encoder.draw_primitives_instanced(MTLPrimitiveType::Triangle, 0, 6, self.edge_count as u64);
        encoder.end_encoding();
        if handles {
            let pass = RenderPassDescriptor::new();
            let color = pass.color_attachments().object_at(0).unwrap();
            color.set_texture(Some(&self.texture));
            color.set_load_action(MTLLoadAction::Load);
            color.set_store_action(MTLStoreAction::Store);
            let encoder = command.new_render_command_encoder(pass);
            encoder.set_render_pipeline_state(&self.handle_pipeline);
            encoder.set_vertex_buffer(0, Some(&self.positions), 0);
            encoder.set_vertex_bytes(
                2,
                std::mem::size_of::<Uniforms>() as u64,
                (u as *const Uniforms).cast(),
            );
            let count = self.vertex_count as u32;
            encoder.set_vertex_bytes(3, 4, (&count as *const u32).cast());
            encoder.draw_primitives_instanced(
                MTLPrimitiveType::Triangle,
                0,
                6,
                self.vertex_count as u64,
            );
            encoder.end_encoding();
        }
        command.commit();
        command.to_owned()
    }
}

fn scene(mesh: &Mesh, u: &Uniforms, overlay: Option<&Overlay>, handles: bool) -> Scene {
    let mut scene = Scene::default();
    let quad = |bounds: Bounds<Pixels>, color: u32, mask: ContentMask<Pixels>| Quad {
        order: 0,
        border_style: Default::default(),
        bounds: bounds.scale(SCALE),
        content_mask: mask.scale(SCALE),
        background: rgba(color).into(),
        border_color: Default::default(),
        corner_radii: Default::default(),
        border_widths: Default::default(),
    };
    scene.insert_primitive(quad(
        u.viewport_bounds(),
        0x202830ff,
        ContentMask {
            bounds: u.viewport_bounds(),
        },
    ));
    let positions: Vec<_> = if overlay.is_none() || handles {
        mesh.positions.iter().map(|p| u.transform(*p)).collect()
    } else {
        Vec::new()
    };
    if let Some(overlay) = overlay {
        scene.insert_primitive(PaintSurface {
            order: 0,
            bounds: u.viewport_bounds().scale(SCALE),
            content_mask: u.mask().scale(SCALE),
            source: SurfaceSource::MetalTexture(overlay.texture.clone()),
            opacity: 1.,
        });
    } else {
        let edges: Vec<_> = mesh
            .edges
            .iter()
            .map(|e| (positions[e[0] as usize], positions[e[1] as usize]))
            .collect();
        mesh_edges::paint_edges(&edges, point(px(0.), px(0.)), |mut path| {
            path.color = rgba(COLOR).into();
            path.content_mask = u.mask();
            scene.insert_primitive(path.scale(SCALE));
        });
    }
    if handles {
        for (i, p) in positions.iter().enumerate() {
            let radius = if i == (u.frame as usize * 17) % positions.len() {
                4.
            } else {
                3.
            };
            scene.insert_primitive(quad(
                Bounds::new(
                    point(px(p.x - radius), px(p.y - radius)),
                    size(px(radius * 2.), px(radius * 2.)),
                ),
                if i % 37 == 0 { 0xff4060ff } else { 0xf0f0f0ff },
                u.mask(),
            ));
        }
    }
    scene.finish();
    scene
}

fn gpu_times(command: &CommandBufferRef) -> anyhow::Result<(f64, f64)> {
    ensure!(
        command.status() == MTLCommandBufferStatus::Completed,
        "GPU command failed: {:?}",
        command.status()
    );
    let object =
        unsafe { &*(command as *const CommandBufferRef as *const objc2::runtime::AnyObject) };
    let start: f64 = unsafe { objc2::msg_send![object, GPUStartTime] };
    let end: f64 = unsafe { objc2::msg_send![object, GPUEndTime] };
    ensure!(start > 0. && end >= start, "invalid GPU timestamps");
    Ok((start, end))
}

fn gpu_union_ms(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.1 - a.0) + (b.1 - b.0) - (a.1.min(b.1) - a.0.max(b.0)).max(0.)) * 1e3
}

#[derive(Serialize)]
struct Sample {
    width: u32,
    height: u32,
    grid: usize,
    vertices: usize,
    edges: usize,
    mode: String,
    handles: bool,
    method: String,
    sample: usize,
    warmup: bool,
    pair_order: usize,
    paths: usize,
    path_batches: usize,
    prepare_ms: f64,
    overlay_submit_ms: f64,
    gpui_submit_ms: f64,
    cpu_submit_ms: f64,
    complete_ms: f64,
    overlay_gpu_ms: f64,
    compose_gpu_ms: f64,
    gpu_union_ms: f64,
    gpu_sum_ms: f64,
    gpu_span_ms: f64,
}
fn measure(
    gpui: &mut MetalRenderer,
    queue: &CommandQueueRef,
    overlay: &Overlay,
    mesh: &Mesh,
    u: &Uniforms,
    width: u32,
    height: u32,
    mode: &str,
    handles: bool,
    sample: usize,
    order: usize,
    method: usize,
) -> anyhow::Result<Sample> {
    objc2::rc::autoreleasepool(|_| {
        let dedicated = method != 0;
        let start = Instant::now();
        if dedicated && mode == "deform" {
            overlay.upload(mesh);
        }
        let scene = scene(
            mesh,
            u,
            dedicated.then_some(overlay),
            handles && method != 2,
        );
        let prepare_ms = start.elapsed().as_secs_f64() * 1e3;
        let overlay_command = dedicated.then(|| overlay.submit(queue, u, handles && method == 2));
        let after_overlay = start.elapsed().as_secs_f64() * 1e3;
        let command = gpui.submit_scene_for_benchmark(
            &scene,
            size(DevicePixels(width as i32), DevicePixels(height as i32)),
        )?;
        let cpu_submit_ms = start.elapsed().as_secs_f64() * 1e3;
        command.wait_until_completed();
        let complete_ms = start.elapsed().as_secs_f64() * 1e3;
        let (compose_start, compose_end) = gpu_times(&command)?;
        let (overlay_start, overlay_end) = if let Some(command) = overlay_command.as_ref() {
            gpu_times(command)?
        } else {
            (compose_start, compose_start)
        };
        let overlay_gpu_ms = (overlay_end - overlay_start) * 1e3;
        let compose_gpu_ms = (compose_end - compose_start) * 1e3;
        Ok(Sample {
            width,
            height,
            grid: mesh.grid,
            vertices: mesh.positions.len(),
            edges: mesh.edges.len(),
            mode: mode.into(),
            handles,
            method: METHODS[method].into(),
            sample,
            warmup: sample < WARMUP,
            pair_order: order,
            paths: scene.paths.len(),
            path_batches: scene
                .batches()
                .filter(|b| matches!(b, PrimitiveBatch::Paths(_)))
                .count(),
            prepare_ms,
            overlay_submit_ms: after_overlay - prepare_ms,
            gpui_submit_ms: cpu_submit_ms - after_overlay,
            cpu_submit_ms,
            complete_ms,
            overlay_gpu_ms,
            compose_gpu_ms,
            gpu_union_ms: gpu_union_ms((overlay_start, overlay_end), (compose_start, compose_end)),
            gpu_sum_ms: overlay_gpu_ms + compose_gpu_ms,
            gpu_span_ms: (compose_end.max(overlay_end) - compose_start.min(overlay_start)) * 1e3,
        })
    })
}

fn quantiles(values: impl Iterator<Item = f64>) -> serde_json::Value {
    let mut values: Vec<_> = values.collect();
    values.sort_by(f64::total_cmp);
    let q = |p: f64| values[(values.len() as f64 * p).ceil() as usize - 1];
    serde_json::json!({"count":values.len(),"p50":q(0.5),"p95":q(0.95),"p99":q(0.99),"max":values.last(),
        "over_8_33_ms":values.iter().filter(|v|**v>1000./120.).count()})
}

fn main() -> anyhow::Result<()> {
    ensure!(!cfg!(debug_assertions), "benchmark must run with --release");
    let mut args = std::env::args().skip(1);
    let output=PathBuf::from(args.next().context("usage: mesh_overlay_benchmark OUTDIR [SAMPLES=100] [GRIDS=8,32,64,128,256] [SIZES=1280x800,2560x1600]")?);
    let samples: usize = args.next().unwrap_or_else(|| "100".into()).parse()?;
    let grids: Vec<usize> = args
        .next()
        .unwrap_or_else(|| "8,32,64,128,256".into())
        .split(',')
        .map(str::parse)
        .collect::<Result<_, _>>()?;
    let sizes: Vec<(u32, u32)> = args
        .next()
        .unwrap_or_else(|| "1280x800,2560x1600".into())
        .split(',')
        .map(|s| {
            let (w, h) = s.split_once('x').context("size must be WIDTHxHEIGHT")?;
            Ok((w.parse()?, h.parse()?))
        })
        .collect::<anyhow::Result<_>>()?;
    ensure!(
        args.next().is_none() && (1..=10_000).contains(&samples),
        "invalid arguments"
    );
    ensure!(
        !grids.is_empty() && grids.iter().all(|n| (1..=512).contains(n)),
        "grid must be 1..512"
    );
    ensure!(
        !sizes.is_empty()
            && sizes
                .iter()
                .all(|(w, h)| (128..=8192).contains(w) && (128..=8192).contains(h)),
        "invalid image size"
    );
    fs::create_dir_all(&output)?;
    let mut raw = BufWriter::new(
        File::options()
            .write(true)
            .create_new(true)
            .open(output.join("samples.jsonl"))?,
    );
    let mut all = Vec::new();
    let mut quality = Vec::new();
    let mut setup = Vec::new();
    objc2::rc::autoreleasepool(|_| -> anyhow::Result<()> {
        let mut gpui = MetalRenderer::new_headless(Default::default());
        let gpu = gpui.shared_metal_context();
        fs::write(
            output.join("metadata.json"),
            serde_json::to_vec_pretty(
                &serde_json::json!({"device":gpu.device.name(),"samples":samples,"warmup":WARMUP,"grids":grids,"sizes":sizes,"scale_factor":SCALE,"msaa":4,"line_width_logical":1,"color_rgba":COLOR,"frames_in_flight":1,"headless":true,"timed_readback":false,"schema_version":2,"methods":METHODS,"order":"six permutations of ABC, repeating; pair_order is position 0..2","markers":"selected every 37; hovered index=(frame*17)%vertex_count; radius 3 logical, hovered 4; single sample","geometry":"triangulated square grid; cached unique edge indices for both methods","workloads":["view: uniform transform changes, persistent GPU positions","deform: all positions change; GPU upload included, SDK evaluation excluded"]}),
            )?,
        )?;
        for (width, height) in sizes {
            for grid in &grids {
                let mut mesh = Mesh::grid(*grid);
                let start = Instant::now();
                let overlay = Overlay::new(&gpu.device, &mesh, width, height)?;
                setup.push(serde_json::json!({"width":width,"height":height,"grid":grid,"candidate_setup_ms":start.elapsed().as_secs_f64()*1e3,
                    "candidate_position_bytes":overlay.positions.length(),"candidate_index_bytes":overlay.edges.length(),"candidate_resolve_bytes":width as u64*height as u64*4}));
                for mode in ["view", "deform"] {
                    for handles in [false, true] {
                        // Frame 0 hovers a selected vertex; frame 17 hovers another vertex.
                        for qa_sample in [0, 17] {
                            mesh.set_frame(qa_sample, mode);
                            overlay.upload(&mesh);
                            let u = Uniforms::new(width, height, qa_sample, mode);
                            for method in [1, 2] {
                                let qa = objc2::rc::autoreleasepool(|_| -> anyhow::Result<_> {
                                    let original = gpui.render_scene_to_image(
                                        &scene(&mesh, &u, None, handles),
                                        size(
                                            DevicePixels(width as i32),
                                            DevicePixels(height as i32),
                                        ),
                                    )?;
                                    let producer =
                                        overlay.submit(&gpu.queue, &u, handles && method == 2);
                                    let candidate = gpui.render_scene_to_image(
                                        &scene(&mesh, &u, Some(&overlay), handles && method != 2),
                                        size(
                                            DevicePixels(width as i32),
                                            DevicePixels(height as i32),
                                        ),
                                    )?;
                                    gpu_times(&producer)?;
                                    let mut sum = 0u64;
                                    let mut changed = 0usize;
                                    let mut max = 0u8;
                                    for (a, b) in original.pixels().zip(candidate.pixels()) {
                                        let delta =
                                            a.0.iter()
                                                .zip(b.0)
                                                .map(|(a, b)| a.abs_diff(b))
                                                .max()
                                                .unwrap();
                                        if delta > 4 {
                                            changed += 1;
                                        }
                                        max = max.max(delta);
                                        sum += a.0[..3]
                                            .iter()
                                            .zip(&b.0[..3])
                                            .map(|(a, b)| u64::from(a.abs_diff(*b)))
                                            .sum::<u64>();
                                    }
                                    let mae = sum as f64 / (width as f64 * height as f64 * 3.);
                                    let changed_fraction =
                                        changed as f64 / (width as f64 * height as f64);
                                    let stem = format!(
                                        "qa-{width}x{height}-{grid}-{mode}-{handles}-frame{qa_sample}-{}",
                                        METHODS[method]
                                    );
                                    original.save(output.join(format!("{stem}-gpui.png")))?;
                                    candidate.save(output.join(format!("{stem}-texture.png")))?;
                                    let record = serde_json::json!({"width":width,"height":height,"grid":grid,"mode":mode,"handles":handles,"frame":qa_sample,"method":METHODS[method],"mae_rgb":mae,"fraction_pixels_delta_gt_4":changed_fraction,"max_channel_delta":max});
                                    eprintln!("QA {record}");
                                    ensure!(
                                        mae <= 0.5 && changed_fraction <= 0.01,
                                        "pixel equivalence failed: {record}"
                                    );
                                    Ok(record)
                                })?;
                                quality.push(qa);
                            }
                        }
                        mesh.set_frame(0, mode);
                        overlay.upload(&mesh);
                        for sample in 0..samples + WARMUP {
                            // Common, already evaluated geometry is outside the renderer timing.
                            mesh.set_frame(sample, mode);
                            let u = Uniforms::new(width, height, sample, mode);
                            for (order, method) in method_order(sample).into_iter().enumerate() {
                                let row = measure(
                                    &mut gpui, &gpu.queue, &overlay, &mesh, &u, width, height,
                                    mode, handles, sample, order, method,
                                )?;
                                serde_json::to_writer(&mut raw, &row)?;
                                writeln!(raw)?;
                                all.push(row);
                            }
                        }
                        raw.flush()?;
                        eprintln!(
                            "DONE {width}x{height} grid={grid} edges={} mode={mode} handles={handles}",
                            mesh.edges.len()
                        );
                    }
                }
            }
        }
        Ok(())
    })?;
    raw.flush()?;
    fs::write(
        output.join("quality.json"),
        serde_json::to_vec_pretty(&quality)?,
    )?;
    fs::write(
        output.join("setup.json"),
        serde_json::to_vec_pretty(&setup)?,
    )?;
    let mut groups: BTreeMap<String, Vec<&Sample>> = BTreeMap::new();
    for row in &all {
        if !row.warmup {
            groups
                .entry(format!(
                    "{}x{}/{}/{}/{}/{}",
                    row.width, row.height, row.grid, row.mode, row.handles, row.method
                ))
                .or_default()
                .push(row);
        }
    }
    let mut summary = BTreeMap::new();
    for (key, rows) in groups {
        summary.insert(key,serde_json::json!({"edges":rows[0].edges,"vertices":rows[0].vertices,"paths":rows[0].paths,"path_batches":rows[0].path_batches,
            "prepare_ms":quantiles(rows.iter().map(|r|r.prepare_ms)),"cpu_submit_ms":quantiles(rows.iter().map(|r|r.cpu_submit_ms)),
            "overlay_gpu_ms":quantiles(rows.iter().map(|r|r.overlay_gpu_ms)),"compose_gpu_ms":quantiles(rows.iter().map(|r|r.compose_gpu_ms)),
            "gpu_union_ms":quantiles(rows.iter().map(|r|r.gpu_union_ms)),"gpu_sum_ms":quantiles(rows.iter().map(|r|r.gpu_sum_ms)),"gpu_span_ms":quantiles(rows.iter().map(|r|r.gpu_span_ms)),
            "complete_ms":quantiles(rows.iter().map(|r|r.complete_ms))}));
    }
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comparison_order_balances_each_method_and_position() {
        let mut counts = [[0; 3]; 3];
        for sample in WARMUP..WARMUP + 30 {
            for (position, method) in method_order(sample).into_iter().enumerate() {
                counts[method][position] += 1;
            }
        }
        assert_eq!(counts, [[10; 3]; 3]);
    }
    #[test]
    fn gpu_intervals_do_not_double_count_overlapping_commands() {
        assert_eq!(gpu_union_ms((1., 3.), (2., 4.)), 3000.);
        assert_eq!(gpu_union_ms((1., 2.), (3., 4.)), 2000.);
        assert_eq!(gpu_union_ms((1., 4.), (2., 3.)), 3000.);
        assert_eq!(gpu_union_ms((3., 3.), (3., 4.)), 1000.);
    }
    #[test]
    fn grid_edges_are_unique_and_cover_every_triangle() {
        for n in [1, 8, 32, 256] {
            let mesh = Mesh::grid(n);
            assert_eq!(mesh.edges.len(), 3 * n * n + 2 * n);
            assert_eq!(mesh.positions.len(), (n + 1) * (n + 1));
            assert!(mesh.edges.windows(2).all(|p| p[0] < p[1]));
            assert!(
                mesh.edges
                    .iter()
                    .all(|e| e[0] < e[1] && (e[1] as usize) < mesh.positions.len())
            );
            for y in 0..n {
                for x in 0..n {
                    let a = (y * (n + 1) + x) as u32;
                    let b = a + 1;
                    let c = a + n as u32 + 1;
                    let d = c + 1;
                    for edge in [[a, b], [b, d], [a, d], [a, c], [c, d]] {
                        assert!(mesh.edges.binary_search(&edge).is_ok());
                    }
                }
            }
        }
    }
}

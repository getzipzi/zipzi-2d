use anyhow::Result;
use std::collections::{hash_map::Entry, HashMap};
use windows::Win32::Graphics::{
    Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, Direct3D11::*,
    Dxgi::Common::DXGI_FORMAT_R32_UINT,
};
use zipzi_core::{BlendMode, Drawable, DrawableFrame, Status, Vec2};
use zipzi_render::{
    gpu::{compose_affine, inverse_affine, MaskLayout, MaskViewport, EMPTY_MASK_LAYOUT},
    surface_layout_bounds, Affine2, Bounds2, MaskId, MeshId, ScenePlan, TargetId, TargetItem,
    ViewportConfig,
};

use crate::{
    renderer::DrawRange,
    resources::{bytes, dynamic_buffer, valid_extent, write_discard},
    D3d11Renderer, D3d11Target, D3d11TextureCatalog,
};

#[derive(Clone, Copy)]
struct Layout {
    extent: [u32; 2],
    transform: Affine2,
    screen_origin: Vec2,
}

impl Layout {
    fn cropped(bounds: Bounds2, view: ViewportConfig) -> Result<Self, Status> {
        let (size, transform) = surface_layout_bounds(bounds, view.transform, view.target_extent)?;
        Ok(Self {
            extent: [size.width as u32, size.height as u32],
            transform,
            screen_origin: Vec2::new(
                view.transform.origin.x - transform.origin.x,
                view.transform.origin.y - transform.origin.y,
            ),
        })
    }
}

#[derive(Clone, Copy)]
struct Attachment {
    target: TargetId,
    slot: Option<usize>,
}

enum ColorOp {
    Clear(Attachment),
    Draw(Attachment, MeshId, Option<usize>),
    Composite {
        parent: Attachment,
        child: Attachment,
        snapshot: Option<usize>,
    },
}

pub(crate) struct ColorPlan {
    pub(crate) sizes: Vec<[u32; 2]>,
    layouts: Vec<Layout>,
    ops: Vec<ColorOp>,
}

impl ColorPlan {
    pub(crate) fn new(
        plan: &ScenePlan,
        frame: &DrawableFrame,
        view: ViewportConfig,
        extent: [u32; 2],
    ) -> Result<Self, Status> {
        let main = Layout {
            extent,
            transform: view.transform,
            screen_origin: Vec2::default(),
        };
        let mut layouts = vec![main; plan.targets().len()];
        for (layout, target) in layouts.iter_mut().zip(plan.targets()).skip(1) {
            if target.active {
                *layout = Layout::cropped(target.bounds, view)?;
            }
        }
        let cropped = Self::lower(plan, frame, layouts);
        let selected = if plan.active_target_count() == 0 {
            cropped
        } else {
            let common = Layout::cropped(plan.effective_bounds(), view)?;
            let mut layouts = vec![common; plan.targets().len()];
            layouts[0] = main;
            let global = Self::lower(plan, frame, layouts);
            if global.sizes.iter().copied().map(bytes).sum::<u64>()
                < cropped.sizes.iter().copied().map(bytes).sum::<u64>()
            {
                global
            } else {
                cropped
            }
        };
        if !selected.sizes.iter().copied().all(valid_extent) {
            return Err(Status::error(
                "OFFSCREEN_BUDGET_EXCEEDED",
                "D3D11 offscreen exceeds device limits",
            ));
        }
        Ok(selected)
    }

    fn lower(plan: &ScenePlan, frame: &DrawableFrame, layouts: Vec<Layout>) -> Self {
        let mut sizes = Vec::new();
        let mut free = Vec::new();
        let main = Attachment {
            target: TargetId::MAIN,
            slot: None,
        };
        let mut ops = vec![ColorOp::Clear(main)];
        let mut stack = vec![(main, 0)];
        while let Some((target, next)) = stack.last_mut() {
            let attachment = *target;
            let target = &plan.targets()[target.target.0];
            let Some(&item) = target.items.get(*next) else {
                stack.pop();
                if let Some(&(parent, _)) = stack.last() {
                    let snapshot = target
                        .reads_destination
                        .then(|| acquire(&mut sizes, &mut free, layouts[parent.target.0].extent));
                    ops.push(ColorOp::Composite {
                        parent,
                        child: attachment,
                        snapshot,
                    });
                    free.extend(snapshot);
                    free.extend(attachment.slot);
                }
                continue;
            };
            *next += 1;
            match item {
                TargetItem::Draw(id) if visible(&frame.drawables[id.0]) => {
                    let snapshot = plan.meshes()[id.0].reads_destination.then(|| {
                        acquire(&mut sizes, &mut free, layouts[attachment.target.0].extent)
                    });
                    ops.push(ColorOp::Draw(attachment, id, snapshot));
                    free.extend(snapshot);
                }
                TargetItem::Composite(child) if plan.targets()[child.0].active => {
                    let slot = acquire(&mut sizes, &mut free, layouts[child.0].extent);
                    let child = Attachment {
                        target: child,
                        slot: Some(slot),
                    };
                    ops.push(ColorOp::Clear(child));
                    stack.push((child, 0));
                }
                _ => {}
            }
        }
        Self {
            sizes,
            layouts,
            ops,
        }
    }
}

fn acquire(sizes: &mut Vec<[u32; 2]>, free: &mut Vec<usize>, extent: [u32; 2]) -> usize {
    if let Some(i) = free.iter().position(|&slot| sizes[slot] == extent) {
        return free.swap_remove(i);
    }
    sizes.push(extent);
    sizes.len() - 1
}

fn visible(mesh: &Drawable) -> bool {
    mesh.renderable && mesh.visible && mesh.opacity > 0. && !mesh.indices.is_empty()
}

#[derive(Default)]
pub(crate) struct MaskPlan {
    jobs: Vec<(MaskId, Option<MaskLayout>)>,
    meshes: Vec<Option<usize>>,
    groups: Vec<Option<usize>>,
}

impl MaskPlan {
    pub(crate) fn new(
        plan: &ScenePlan,
        frame: &DrawableFrame,
        scale: f64,
        view: MaskViewport,
    ) -> Result<Self, Status> {
        let mut jobs = Vec::new();
        let mut slots = HashMap::new();
        let mut add = |mask: MaskId, scale: f64| {
            let slot = match slots.entry((mask, scale.to_bits())) {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    let layout = view.layout(plan.masks()[mask.0].bounds, scale)?;
                    jobs.push((mask, layout));
                    *entry.insert(jobs.len() - 1)
                }
            };
            Ok(slot)
        };
        let meshes = plan
            .meshes()
            .iter()
            .zip(&frame.drawables)
            .map(|(mesh, drawable)| {
                mesh.mask
                    .filter(|_| visible(drawable) && plan.targets()[mesh.target.0].active)
                    .map(|mask| add(mask, scale))
                    .transpose()
            })
            .collect::<Result<_, Status>>()?;
        let groups = plan
            .targets()
            .iter()
            .skip(1)
            .map(|target| {
                target
                    .mask
                    .filter(|_| target.active)
                    .map(|mask| add(mask, scale.max(1.)))
                    .transpose()
            })
            .collect::<Result<_, Status>>()?;
        Ok(Self {
            jobs,
            meshes,
            groups,
        })
    }

    pub(crate) fn extents(&self) -> impl Iterator<Item = [u32; 2]> + '_ {
        self.jobs
            .iter()
            .map(|(_, layout)| layout.map_or([1, 1], |l| [l.width, l.height]))
    }

    fn apply<'a>(
        &self,
        draw: &mut Draw<'a>,
        targets: &'a [D3d11Target],
        slot: Option<usize>,
        space: Affine2,
        inverted: bool,
    ) {
        if let Some(slot) = slot {
            draw.masked(
                &targets[slot],
                self.jobs[slot].1.unwrap_or(EMPTY_MASK_LAYOUT),
                space,
                inverted,
            );
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Program {
    Mask,
    Legacy(BlendMode),
    Raw(u32),
}

type Constants = [[f32; 4]; 8];
type ConstantKey = [[u32; 4]; 8];
#[derive(Default)]
pub(crate) struct ConstantBuffers {
    buffers: Vec<ID3D11Buffer>,
    used: HashMap<ConstantKey, usize>,
}

impl ConstantBuffers {
    pub(crate) fn clear(&mut self) {
        self.buffers.clear();
        self.used.clear();
    }

    fn begin(&mut self) {
        // Retain only the last recording's working set, never a pose history.
        self.buffers.truncate(self.used.len());
        self.used.clear();
    }

    fn get(
        &mut self,
        renderer: &D3d11Renderer,
        key: ConstantKey,
        data: &Constants,
    ) -> Result<&ID3D11Buffer> {
        let next = self.used.len();
        let slot = match self.used.entry(key) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                if next == self.buffers.len() {
                    self.buffers.push(dynamic_buffer(
                        &renderer.device,
                        std::mem::size_of::<Constants>(),
                        D3D11_BIND_CONSTANT_BUFFER,
                    )?);
                }
                write_discard(&renderer.deferred, &self.buffers[next], data)?;
                *entry.insert(next)
            }
        };
        Ok(&self.buffers[slot])
    }
}

struct Draw<'a> {
    range: DrawRange,
    source: &'a ID3D11ShaderResourceView,
    mask: Option<&'a ID3D11ShaderResourceView>,
    destination: Option<&'a ID3D11ShaderResourceView>,
    data: Constants,
    program: Program,
    double_sided: bool,
    repeat: bool,
}

impl<'a> Draw<'a> {
    fn new(
        range: DrawRange,
        source: &'a ID3D11ShaderResourceView,
        view: Affine2,
        extent: [u32; 2],
        program: Program,
    ) -> Self {
        let [w, h] = extent.map(|n| n as f32);
        let (legacy, raw_color, raw_alpha) = match program {
            Program::Mask | Program::Legacy(BlendMode::Normal) => (0., 0., 0.),
            Program::Legacy(BlendMode::Additive) => (1., 0., 0.),
            Program::Legacy(BlendMode::Multiplicative) => (2., 0., 0.),
            Program::Raw(mode) => (0., (mode & 255) as f32, ((mode >> 8) & 255) as f32),
        };
        Self {
            range,
            source,
            mask: None,
            destination: None,
            program,
            double_sided: true,
            repeat: true,
            data: [
                [
                    2. * view.a.x / w,
                    2. * view.b.x / w,
                    0.,
                    2. * view.origin.x / w - 1.,
                ],
                [
                    -2. * view.a.y / h,
                    -2. * view.b.y / h,
                    0.,
                    1. - 2. * view.origin.y / h,
                ],
                [1.; 4],
                [0.; 4],
                [0.; 4],
                [0.; 4],
                [1., 0., 0., legacy],
                [0., raw_color, raw_alpha, 0.],
            ],
        }
    }

    fn masked(
        &mut self,
        target: &'a D3d11Target,
        layout: MaskLayout,
        space: Affine2,
        inverted: bool,
    ) {
        let size = layout.logical_size;
        self.mask = Some(&target.view);
        self.data[4] = [
            space.a.x / size.x,
            space.b.x / size.x,
            (space.origin.x - layout.origin.x) / size.x,
            0.,
        ];
        self.data[5] = [
            space.a.y / size.y,
            space.b.y / size.y,
            (space.origin.y - layout.origin.y) / size.y,
            0.,
        ];
        self.data[6][1] = 1.;
        self.data[6][2] = u8::from(inverted) as f32;
    }
}

pub(crate) fn encode(
    renderer: &D3d11Renderer,
    output: &D3d11Target,
    textures: &D3d11TextureCatalog<'_>,
    color: &ColorPlan,
    masks: &MaskPlan,
    constants: &mut ConstantBuffers,
) -> Result<()> {
    let geometry = renderer.geometry.as_ref().unwrap();
    let frame = renderer.frame.as_ref().unwrap();
    let c = &renderer.deferred;
    // Upload on every list, including view-only redraws of the same frame: the
    // preceding recording may never have been submitted by the host.
    write_discard(c, &geometry.vertex, &geometry.vertices)?;
    constants.begin();
    unsafe {
        c.IASetInputLayout(&renderer.input);
        c.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        c.IASetVertexBuffers(
            0,
            1,
            Some(&Some(geometry.vertex.clone())),
            Some(&16),
            Some(&0),
        );
        c.IASetIndexBuffer(&geometry.index, DXGI_FORMAT_R32_UINT, 0);
        c.VSSetShader(&renderer.vertex, None);
    }
    let mut encoder = Encoder {
        renderer,
        bound: None,
        repeat: None,
        program: None,
        face: None,
        constant_key: None,
        constants,
    };
    // An Editor frame may be discarded before submission. Each list establishes
    // the contents of every scratch attachment it reads, including pooled masks.
    for (&(mask, layout), target) in masks.jobs.iter().zip(&renderer.masks) {
        encoder.clear(target);
        if let Some(layout) = layout {
            let view = Affine2 {
                a: Vec2::new(layout.scale, 0.),
                b: Vec2::new(0., layout.scale),
                origin: Vec2::new(
                    -layout.origin.x * layout.scale,
                    -layout.origin.y * layout.scale,
                ),
            };
            for id in &renderer.plan.masks()[mask.0].sources {
                let mesh = &frame.drawables[id.0];
                let range = geometry.ranges[id.0];
                if range.count > 0 {
                    let mut draw = Draw::new(
                        range,
                        textures.view(&mesh.texture_asset_id)?,
                        view,
                        target.extent,
                        Program::Mask,
                    );
                    draw.double_sided = mesh.double_sided;
                    encoder.draw(&draw)?;
                }
            }
        }
    }
    let target = |attachment: Attachment| {
        attachment
            .slot
            .map_or(output, |slot| &renderer.colors[slot])
    };
    for op in &color.ops {
        match *op {
            ColorOp::Clear(binding) => encoder.clear(target(binding)),
            ColorOp::Draw(binding, id, snapshot) => {
                let mesh = &frame.drawables[id.0];
                let target = target(binding);
                let program = mesh
                    .raw_blend_mode
                    .map_or(Program::Legacy(mesh.blend_mode), Program::Raw);
                let mut draw = Draw::new(
                    geometry.ranges[id.0],
                    textures.view(&mesh.texture_asset_id)?,
                    color.layouts[binding.target.0].transform,
                    target.extent,
                    program,
                );
                draw.data[2] = mesh.multiply_color;
                draw.data[3] = mesh.screen_color;
                draw.data[6][0] = mesh.opacity;
                draw.double_sided = mesh.double_sided;
                masks.apply(
                    &mut draw,
                    &renderer.masks,
                    masks.meshes[id.0],
                    Affine2::IDENTITY,
                    mesh.inverted_mask,
                );
                draw.destination = encoder.snapshot(target, snapshot);
                encoder.bind(target);
                encoder.draw(&draw)?;
            }
            ColorOp::Composite {
                parent,
                child,
                snapshot,
            } => {
                let group = &frame.offscreens[child.target.0 - 1];
                let layout = color.layouts[child.target.0];
                let parent_layout = color.layouts[parent.target.0];
                let pixels = Affine2 {
                    a: Vec2::new(layout.extent[0] as f32, 0.),
                    b: Vec2::new(0., layout.extent[1] as f32),
                    origin: Vec2::default(),
                };
                let view = Affine2 {
                    origin: Vec2::new(
                        layout.screen_origin.x - parent_layout.screen_origin.x,
                        layout.screen_origin.y - parent_layout.screen_origin.y,
                    ),
                    ..pixels
                };
                let program = if group.blend_mode == 0 {
                    Program::Legacy(BlendMode::Normal)
                } else {
                    Program::Raw(group.blend_mode)
                };
                let mut draw = Draw::new(
                    DrawRange {
                        count: 6,
                        first_index: 0,
                        base_vertex: 0,
                    },
                    &target(child).view,
                    view,
                    parent_layout.extent,
                    program,
                );
                draw.repeat = false;
                draw.data[2] = group.multiply_color;
                draw.data[3] = group.screen_color;
                draw.data[6][0] = group.opacity;
                draw.data[7][0] = 1.;
                if let Some(slot) = masks.groups[child.target.0 - 1] {
                    let inverse = inverse_affine(layout.transform)
                        .map_err(|s| anyhow::anyhow!("{}", s.message))?;
                    masks.apply(
                        &mut draw,
                        &renderer.masks,
                        Some(slot),
                        compose_affine(inverse, pixels),
                        group.flags & 8 != 0,
                    );
                }
                let parent = target(parent);
                draw.destination = encoder.snapshot(parent, snapshot);
                encoder.bind(parent);
                encoder.draw(&draw)?;
            }
        }
    }
    encoder.unbind();
    Ok(())
}

struct Encoder<'a, 'cache> {
    renderer: &'a D3d11Renderer,
    bound: Option<&'a D3d11Target>,
    repeat: Option<bool>,
    program: Option<Program>,
    face: Option<usize>,
    constant_key: Option<ConstantKey>,
    constants: &'cache mut ConstantBuffers,
}

impl<'a> Encoder<'a, '_> {
    fn unbind(&mut self) {
        unsafe {
            self.renderer
                .deferred
                .PSSetShaderResources(0, Some(&[None, None, None]));
            self.renderer.deferred.OMSetRenderTargets(None, None);
        }
        self.bound = None;
    }

    fn bind(&mut self, target: &'a D3d11Target) {
        if self.bound.is_some_and(|prev| std::ptr::eq(prev, target)) {
            return;
        }
        self.unbind();
        unsafe {
            let c = &self.renderer.deferred;
            c.OMSetRenderTargets(Some(&[Some(target.rtv.clone())]), None);
            c.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: target.extent[0] as f32,
                Height: target.extent[1] as f32,
                MaxDepth: 1.,
                ..Default::default()
            }]));
        }
        self.bound = Some(target);
    }

    fn clear(&mut self, target: &'a D3d11Target) {
        self.bind(target);
        unsafe {
            self.renderer
                .deferred
                .ClearRenderTargetView(&target.rtv, &[0.; 4])
        };
    }

    fn snapshot(
        &mut self,
        source: &D3d11Target,
        slot: Option<usize>,
    ) -> Option<&'a ID3D11ShaderResourceView> {
        let target = &self.renderer.colors[slot?];
        self.unbind();
        unsafe {
            self.renderer
                .deferred
                .CopyResource(&target.texture, &source.texture)
        };
        Some(&target.view)
    }

    fn draw(&mut self, draw: &Draw<'_>) -> Result<()> {
        let renderer = self.renderer;
        let c = &renderer.deferred;
        let key = draw.data.map(|row| row.map(f32::to_bits));
        if self.constant_key != Some(key) {
            let constant = self.constants.get(renderer, key, &draw.data)?;
            let constants = [Some(constant.clone())];
            unsafe {
                c.VSSetConstantBuffers(0, Some(&constants));
                c.PSSetConstantBuffers(0, Some(&constants));
            }
            self.constant_key = Some(key);
        }
        let determinant = draw.data[0][0] * draw.data[1][1] - draw.data[0][1] * draw.data[1][0];
        let face = if draw.double_sided {
            0
        } else if determinant < 0. {
            2
        } else {
            1
        };
        let program = match draw.program {
            Program::Raw(_) => Program::Raw(0),
            program => program,
        };
        unsafe {
            if self.repeat != Some(draw.repeat) {
                c.PSSetSamplers(
                    0,
                    Some(&[
                        renderer.samplers[usize::from(!draw.repeat)].clone(),
                        renderer.samplers[1].clone(),
                    ]),
                );
                self.repeat = Some(draw.repeat);
            }
            if self.program != Some(program) {
                let (pixel, blend) = match program {
                    Program::Mask => (&renderer.mask_pixel, Some(&renderer.blend[0])),
                    Program::Legacy(BlendMode::Normal) => {
                        (&renderer.pixel, Some(&renderer.blend[0]))
                    }
                    Program::Legacy(BlendMode::Additive) => {
                        (&renderer.pixel, Some(&renderer.blend[1]))
                    }
                    Program::Legacy(BlendMode::Multiplicative) => {
                        (&renderer.pixel, Some(&renderer.blend[2]))
                    }
                    Program::Raw(_) => (&renderer.raw_pixel, None),
                };
                c.PSSetShader(pixel, None);
                c.OMSetBlendState(blend, None, u32::MAX);
                self.program = Some(program);
            }
            if self.face != Some(face) {
                c.RSSetState(&renderer.raster[face]);
                self.face = Some(face);
            }
            c.PSSetShaderResources(
                0,
                Some(&[
                    Some(draw.source.clone()),
                    draw.mask.cloned(),
                    draw.destination.cloned(),
                ]),
            );
            c.DrawIndexed(
                draw.range.count,
                draw.range.first_index,
                draw.range.base_vertex,
            );
        }
        Ok(())
    }
}

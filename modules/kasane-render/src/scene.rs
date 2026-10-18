//! Persistent logical scene description. No frame positions or host handles are retained.
use std::collections::HashMap;

use kasane_core::evaluation::{DrawableFrame, RenderCommand};
use kasane_core::types::{Status, Vec2};

use crate::{Affine2, Size2, TextureCatalog};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MeshId(pub usize);
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TargetId(pub usize);
impl TargetId {
    pub const MAIN: Self = Self(0);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaskId(pub usize);

/// Ordering is local to a target. A composite consumes the completed child
/// target; a destination-reading item reads its own target immediately before
/// the item executes. Mask inputs are raw mesh geometry/texture, not color draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetItem {
    Draw(MeshId),
    Composite(TargetId),
}

#[derive(Clone, Debug)]
pub struct MeshPlan {
    pub id: String,
    pub target: TargetId,
    pub mask: Option<MaskId>,
    pub reads_destination: bool,
    mask_sources: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct TargetPlan {
    /// Empty only for the host's main target.
    pub id: String,
    pub parent: TargetId,
    pub active: bool,
    pub mask: Option<MaskId>,
    pub reads_destination: bool,
    pub items: Vec<TargetItem>,
    /// Visible, nonempty color draws and active children in render-plane pixels.
    /// Raw mask inputs retain separate bounds, including hidden source meshes.
    pub bounds: Bounds2,
    mask_sources: Vec<String>,
}

impl TargetPlan {
    pub fn effective_bounds(&self) -> Bounds2 {
        self.bounds
    }
}

#[derive(Clone, Debug)]
pub struct MaskPlan {
    /// Unique raw mesh inputs, in first-occurrence order.
    pub sources: Vec<MeshId>,
    /// Canvas-pixel bounds, before the backend's padding/sampling policy.
    pub bounds: Bounds2,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds2 {
    pub position: Vec2,
    pub size: Vec2,
}
impl Bounds2 {
    pub fn new(position: Vec2, size: Vec2) -> Self {
        Self { position, size }
    }
    pub fn from_point(point: Vec2) -> Self {
        Self {
            position: point,
            size: Vec2::default(),
        }
    }
    pub fn end(self) -> Vec2 {
        Vec2::new(self.position.x + self.size.x, self.position.y + self.size.y)
    }
    pub fn include(self, point: Vec2) -> Self {
        let end = self.end();
        let position = Vec2::new(self.position.x.min(point.x), self.position.y.min(point.y));
        Self {
            position,
            size: Vec2::new(
                end.x.max(point.x) - position.x,
                end.y.max(point.y) - position.y,
            ),
        }
    }
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            other
        } else if other.is_empty() {
            self
        } else {
            self.include(other.position).include(other.end())
        }
    }
    pub fn grow(self, amount: f32) -> Self {
        Self {
            position: Vec2::new(self.position.x - amount, self.position.y - amount),
            size: Vec2::new(self.size.x + 2.0 * amount, self.size.y + 2.0 * amount),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.size.x <= 0.0 || self.size.y <= 0.0
    }
}

/// Owns IDs, dependencies and small layout data, and weakly tracks topology,
/// but never a published frame or dynamic positions.
/// Indices are scoped to this scene's current successful update. External
/// callers cannot mutate its validated records through this API.
#[derive(Clone, Debug, Default)]
pub struct ScenePlan {
    validator: crate::validation::FrameValidator,
    meshes: Vec<MeshPlan>,
    targets: Vec<TargetPlan>,
    masks: Vec<MaskPlan>,
    mesh_ids: HashMap<String, MeshId>,
    target_ids: HashMap<String, TargetId>,
    stack: Vec<TargetId>,
    ordered: Vec<usize>,
    bounds: Vec<Option<Bounds2>>,
    content_bounds: Bounds2,
}

impl ScenePlan {
    pub fn meshes(&self) -> &[MeshPlan] {
        &self.meshes
    }
    pub fn targets(&self) -> &[TargetPlan] {
        &self.targets
    }
    pub fn masks(&self) -> &[MaskPlan] {
        &self.masks
    }
    pub fn content_bounds(&self) -> Bounds2 {
        self.content_bounds
    }
    pub fn effective_bounds(&self) -> Bounds2 {
        self.content_bounds
    }
    pub fn mesh_id(&self, id: &str) -> Option<MeshId> {
        self.mesh_ids.get(id).copied()
    }
    pub fn target_id(&self, id: &str) -> Option<TargetId> {
        self.target_ids.get(id).copied()
    }
    pub fn active_target_count(&self) -> usize {
        self.targets.iter().skip(1).filter(|t| t.active).count()
    }

    /// Validate before changing the published logical state. The unversioned
    /// compatibility input is checked on each model submission; immutable topology
    /// is reused only while its allocation identity and vertex count match.
    /// View-only changes do not call this method.
    pub fn update<T: TextureCatalog>(
        &mut self,
        frame: &DrawableFrame,
        textures: &T,
    ) -> Result<(), Status> {
        let status = self.validator.validate(frame);
        if !status.is_ok() {
            return Err(status);
        }
        for mesh in &frame.drawables {
            let texture = textures
                .texture_info(&mesh.texture_asset_id)
                .ok_or_else(|| Status::error("MISSING_TEXTURE", &mesh.texture_asset_id))?;
            if texture.width == 0 || texture.height == 0 {
                return Err(Status::error("INVALID_TEXTURE", &mesh.texture_asset_id));
            }
        }
        let same_structure = self.meshes.len() == frame.drawables.len()
            && self.targets.len() == frame.offscreens.len() + 1
            && self
                .meshes
                .iter()
                .zip(&frame.drawables)
                .all(|(a, b)| a.id == b.id && a.mask_sources == b.masks)
            && self
                .targets
                .iter()
                .skip(1)
                .zip(&frame.offscreens)
                .all(|(a, b)| a.id == b.id && a.mask_sources == b.masks);
        if !same_structure {
            self.rebuild_structure(frame);
        }
        for target in &mut self.targets {
            target.items.clear();
            target.active = false;
        }
        self.targets[0].active = true;
        self.stack.clear();
        self.stack.push(TargetId::MAIN);
        for (record, mesh) in self.meshes.iter_mut().zip(&frame.drawables) {
            record.reads_destination = mesh.raw_blend_mode.is_some();
        }
        for (record, group) in self.targets.iter_mut().skip(1).zip(&frame.offscreens) {
            record.reads_destination = group.blend_mode != 0;
        }
        if frame.render_plan.is_empty() {
            self.ordered.clear();
            self.ordered.extend(0..frame.drawables.len());
            self.ordered
                .sort_unstable_by_key(|&i| (frame.drawables[i].render_order, i));
            for &i in &self.ordered {
                self.meshes[i].target = TargetId::MAIN;
                self.targets[0].items.push(TargetItem::Draw(MeshId(i)));
            }
        } else {
            for command in &frame.render_plan {
                match command {
                    RenderCommand::BeginOffscreen { offscreen_id } => {
                        let id = self.target_ids[offscreen_id];
                        let parent = *self.stack.last().unwrap();
                        self.targets[id.0].parent = parent;
                        self.targets[parent.0].items.push(TargetItem::Composite(id));
                        self.stack.push(id);
                    }
                    RenderCommand::EndOffscreen { .. } => {
                        self.stack.pop();
                    }
                    RenderCommand::DrawMesh { mesh_id } => {
                        let id = self.mesh_ids[mesh_id];
                        let target = *self.stack.last().unwrap();
                        self.meshes[id.0].target = target;
                        self.targets[target.0].items.push(TargetItem::Draw(id));
                        let mesh = &frame.drawables[id.0];
                        if mesh.visible
                            && mesh.opacity > 0.0
                            && !mesh.indices.is_empty()
                            && self.stack.iter().skip(1).all(|id| {
                                let group = &frame.offscreens[id.0 - 1];
                                group.enabled && group.opacity > 0.0
                            })
                        {
                            for id in &self.stack {
                                self.targets[id.0].active = true;
                            }
                        }
                    }
                }
            }
        }
        self.bounds.clear();
        self.bounds.reserve(frame.drawables.len());
        let mut scene_bounds: Option<Bounds2> = None;
        for d in &frame.drawables {
            if d.positions.is_empty() {
                self.bounds.push(None);
                continue;
            }
            let mut b: Option<Bounds2> = None;
            for p in &d.positions {
                let point = *p;
                b = Some(b.map_or(Bounds2::from_point(point), |b| b.include(point)));
            }
            if let Some(bounds) = b {
                scene_bounds = Some(scene_bounds.map_or(bounds, |sb| sb.union(bounds)));
                self.bounds.push(Some(bounds));
            } else {
                self.bounds.push(None);
            }
        }
        self.content_bounds = scene_bounds.unwrap_or_default();
        for mask in &mut self.masks {
            let mut union: Option<Bounds2> = None;
            for id in &mask.sources {
                if let Some(Some(bounds)) = self.bounds.get(id.0) {
                    union = Some(union.map_or(*bounds, |b| b.union(*bounds)));
                }
            }
            mask.bounds = union.unwrap_or_default();
        }
        for target in &mut self.targets {
            target.bounds = Bounds2::default();
        }
        // Offscreen storage order is independent of nesting. Visit the render
        // tree in postorder so every child's bounds exist before its parent.
        let mut steps = vec![(TargetId::MAIN, false)];
        while let Some((id, visited)) = steps.pop() {
            if !visited {
                steps.push((id, true));
                for item in &self.targets[id.0].items {
                    if let TargetItem::Composite(child) = item {
                        if self.targets[child.0].active {
                            steps.push((*child, false));
                        }
                    }
                }
                continue;
            }
            let mut target_union: Option<Bounds2> = None;
            for item in &self.targets[id.0].items {
                let bounds = match item {
                    TargetItem::Draw(mesh_id) => {
                        let d = &frame.drawables[mesh_id.0];
                        if d.visible && d.opacity > 0.0 && !d.indices.is_empty() {
                            self.bounds[mesh_id.0]
                        } else {
                            None
                        }
                    }
                    TargetItem::Composite(child_id) => {
                        let child = &self.targets[child_id.0];
                        (child.active && !child.bounds.is_empty()).then_some(child.bounds)
                    }
                };
                if let Some(b) = bounds {
                    target_union = Some(target_union.map_or(b, |u| u.union(b)));
                }
            }
            self.targets[id.0].bounds = target_union.unwrap_or_default();
        }
        Ok(())
    }

    fn rebuild_structure(&mut self, frame: &DrawableFrame) {
        self.mesh_ids.clear();
        self.target_ids.clear();
        self.meshes.clear();
        self.targets.clear();
        self.masks.clear();
        for (i, mesh) in frame.drawables.iter().enumerate() {
            self.mesh_ids.insert(mesh.id.clone(), MeshId(i));
        }
        let mut masks: HashMap<Vec<MeshId>, MaskId> = HashMap::new();
        let mut intern = |sources: &[String]| {
            if sources.is_empty() {
                return None;
            }
            let mut inputs = Vec::with_capacity(sources.len());
            for id in sources {
                let id = self.mesh_ids[id];
                if !inputs.contains(&id) {
                    inputs.push(id);
                }
            }
            Some(*masks.entry(inputs.clone()).or_insert_with(|| {
                let id = MaskId(self.masks.len());
                self.masks.push(MaskPlan {
                    sources: inputs,
                    bounds: Bounds2::default(),
                });
                id
            }))
        };
        for mesh in &frame.drawables {
            self.meshes.push(MeshPlan {
                id: mesh.id.clone(),
                target: TargetId::MAIN,
                mask: intern(&mesh.masks),
                reads_destination: false,
                mask_sources: mesh.masks.clone(),
            });
        }
        self.targets.push(TargetPlan {
            id: String::new(),
            parent: TargetId::MAIN,
            active: true,
            mask: None,
            reads_destination: false,
            items: Vec::new(),
            bounds: Bounds2::default(),
            mask_sources: Vec::new(),
        });
        for (i, group) in frame.offscreens.iter().enumerate() {
            self.target_ids.insert(group.id.clone(), TargetId(i + 1));
            self.targets.push(TargetPlan {
                id: group.id.clone(),
                parent: TargetId::MAIN,
                active: false,
                mask: intern(&group.masks),
                reads_destination: false,
                items: Vec::new(),
                bounds: Bounds2::default(),
                mask_sources: group.masks.clone(),
            });
        }
    }
}

/// Screen-clamped crop for arbitrary bounds. Resource limits and attachment
/// formats belong to backend lowering, not to this coordinate calculation.
pub fn surface_layout_bounds(
    bounds: Bounds2,
    transform: Affine2,
    target: Vec2,
) -> Result<(Size2, Affine2), Status> {
    if !transform.is_finite() || transform.determinant().abs() < 1e-12 {
        return Err(Status::error(
            "INVALID_TRANSFORM",
            "Preview transform must be finite and invertible.",
        ));
    }
    if !target.x.is_finite() || !target.y.is_finite() || target.x <= 0.0 || target.y <= 0.0 {
        return Err(Status::error(
            "INVALID_VIEWPORT",
            "Target extent must be positive and finite.",
        ));
    }
    let p0 = transform.transform_point(bounds.position);
    let p1 = transform.transform_point(Vec2::new(
        bounds.position.x + bounds.size.x,
        bounds.position.y,
    ));
    let p2 = transform.transform_point(Vec2::new(
        bounds.position.x,
        bounds.position.y + bounds.size.y,
    ));
    let p3 = transform.transform_point(bounds.end());
    for point in [p0, p1, p2, p3] {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(Status::error(
                "INVALID_TRANSFORM",
                "Transformed bounds overflowed.",
            ));
        }
    }
    let screen_bounds = Bounds2::from_point(p0).include(p1).include(p2).include(p3);
    let min = Vec2::new(
        screen_bounds.position.x.max(0.0),
        screen_bounds.position.y.max(0.0),
    );
    let max = Vec2::new(
        screen_bounds.end().x.min(target.x),
        screen_bounds.end().y.min(target.y),
    );
    let (origin, end) = if max.x <= min.x || max.y <= min.y {
        (Vec2::default(), Vec2::default())
    } else {
        (
            Vec2::new(min.x.floor(), min.y.floor()),
            Vec2::new(max.x.ceil(), max.y.ceil()),
        )
    };
    let mut root = transform;
    root.origin.x -= origin.x;
    root.origin.y -= origin.y;
    Ok((
        Size2 {
            width: ((end.x - origin.x) as i32).max(2),
            height: ((end.y - origin.y) as i32).max(2),
        },
        root,
    ))
}

/// Geometry-only screen crop. Resource limits and attachment formats belong to
/// backend lowering, not to this coordinate calculation.
pub fn surface_layout(
    canvas: Vec2,
    transform: Affine2,
    target: Vec2,
) -> Result<(Size2, Affine2), Status> {
    surface_layout_bounds(
        Bounds2 {
            position: Vec2::default(),
            size: canvas,
        },
        transform,
        target,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextureInfo;
    use kasane_core::evaluation::{Drawable, OffscreenFrame, RenderCommand};

    #[test]
    fn bounds_union_and_empty() {
        let empty = Bounds2::default();
        assert!(empty.is_empty());
        let b1 = Bounds2::new(Vec2::new(10.0, 20.0), Vec2::new(30.0, 40.0));
        assert!(!b1.is_empty());
        assert_eq!(empty.union(b1), b1);
        assert_eq!(b1.union(empty), b1);
        let b2 = Bounds2::new(Vec2::new(5.0, 15.0), Vec2::new(10.0, 10.0));
        let union = b1.union(b2);
        assert_eq!(union.position, Vec2::new(5.0, 15.0));
        assert_eq!(union.end(), Vec2::new(40.0, 60.0));
    }

    #[test]
    fn surface_layout_bounds_matches_surface_layout_for_canvas() {
        let canvas = Vec2::new(1920.0, 1080.0);
        let transform = Affine2 {
            a: Vec2::new(1.5, 0.0),
            b: Vec2::new(0.0, 1.5),
            origin: Vec2::new(50.0, 75.0),
        };
        let target = Vec2::new(1024.0, 768.0);
        let res1 = surface_layout(canvas, transform, target).unwrap();
        let res2 = surface_layout_bounds(Bounds2::new(Vec2::default(), canvas), transform, target)
            .unwrap();
        assert_eq!(res1, res2);
    }

    #[test]
    fn surface_layout_bounds_crops_arbitrary_position_and_offsets_transform() {
        let bounds = Bounds2::new(Vec2::new(100.0, 200.0), Vec2::new(300.0, 400.0));
        let transform = Affine2 {
            a: Vec2::new(2.0, 0.0),
            b: Vec2::new(0.0, 2.0),
            origin: Vec2::new(0.0, 0.0),
        };
        let target = Vec2::new(1000.0, 1000.0);
        let (size, root) = surface_layout_bounds(bounds, transform, target).unwrap();
        // bounds on screen: [200, 400] to [800, 1200], clamped to target: [200, 400] to [800, 1000]
        assert_eq!(
            size,
            Size2 {
                width: 600,
                height: 600
            }
        );
        assert_eq!(root.origin, Vec2::new(-200.0, -400.0));
    }

    #[test]
    fn scene_plan_computes_content_bounds_and_target_bounds() {
        let frame = DrawableFrame {
            drawables: vec![
                Drawable {
                    id: "d1".to_owned(),
                    texture_asset_id: "tex".to_owned(),
                    positions: vec![
                        Vec2::new(0.0, 0.0),
                        Vec2::new(5.0, 0.0),
                        Vec2::new(5.0, -5.0),
                    ],
                    uvs: std::sync::Arc::from([Vec2::default(), Vec2::default(), Vec2::default()]),
                    indices: std::sync::Arc::from([0, 1, 2]),
                    ..Default::default()
                },
                Drawable {
                    id: "d2".to_owned(),
                    texture_asset_id: "tex".to_owned(),
                    positions: vec![
                        Vec2::new(-2.0, 3.0),
                        Vec2::new(1.0, 1.0),
                        Vec2::new(0.0, 0.0),
                    ],
                    uvs: std::sync::Arc::from([Vec2::default(), Vec2::default(), Vec2::default()]),
                    indices: std::sync::Arc::from([0, 1, 2]),
                    ..Default::default()
                },
            ],
            offscreens: vec![OffscreenFrame {
                id: "grp".to_owned(),
                enabled: true,
                opacity: 1.0,
                ..Default::default()
            }],
            render_plan: vec![
                RenderCommand::BeginOffscreen {
                    offscreen_id: "grp".to_owned(),
                },
                RenderCommand::DrawMesh {
                    mesh_id: "d1".to_owned(),
                },
                RenderCommand::EndOffscreen {
                    offscreen_id: "grp".to_owned(),
                },
                RenderCommand::DrawMesh {
                    mesh_id: "d2".to_owned(),
                },
            ],
            ..Default::default()
        };
        let textures = std::collections::HashMap::from([(
            "tex".to_owned(),
            TextureInfo {
                width: 16,
                height: 16,
            },
        )]);
        let mut scene = ScenePlan::default();
        scene.update(&frame, &textures).unwrap();

        // Planning bounds are unprojected model-space geometry.
        assert_eq!(scene.targets()[1].bounds.position, Vec2::new(0., -5.));
        assert_eq!(scene.targets()[1].bounds.size, Vec2::new(5., 5.));
        assert_eq!(scene.content_bounds().position, Vec2::new(-2., -5.));
        assert_eq!(scene.content_bounds().size, Vec2::new(7., 8.));
        assert_eq!(scene.effective_bounds(), scene.content_bounds());
    }
}

use super::*;
mod bindings;
mod dispatch;
mod expressions;
mod mesh;
mod motions;
mod organization;
mod parameters;
mod physics;
mod pose;
mod resources;
use std::collections::{HashMap, HashSet};
use zipzi_core::{
    state::{AssetContent, AssetRecord, DocumentBuilder},
    Mesh, Parameter, Part, Vec2,
};
use zipzi_project::{ImageResource, ResourceView};

#[derive(Debug, Clone, Copy)]
pub enum NameTarget<'a> {
    Asset(&'a str),
    Parameter(&'a str),
    Part(&'a str),
    Transform(&'a str),
    Mesh(&'a str),
}

/// Typed authoring commands for isolated Draft batches. The underlying builder is
/// private; a caller cannot bypass command validation or failure tracking.
pub struct Commands<'a> {
    builder: &'a mut DocumentBuilder,
    resources: &'a mut ResourceView,
    failure: Option<Status>,
}
impl<'a> Commands<'a> {
    pub(super) fn new(builder: &'a mut DocumentBuilder, resources: &'a mut ResourceView) -> Self {
        Self {
            builder,
            resources,
            failure: None,
        }
    }
    pub(super) fn finish(self) -> Result<(), Status> {
        self.failure.map_or(Ok(()), Err)
    }
    fn run<T>(&mut self, run: impl FnOnce(&mut Self) -> Result<T, Status>) -> Result<T, Status> {
        if let Some(failure) = &self.failure {
            return Err(failure.clone());
        }
        let result = run(self);
        if let Err(failure) = &result {
            self.failure = Some(failure.clone());
        }
        result
    }
    fn unlocked(&self, id: &str) -> Result<(), Status> {
        self.builder.authoring_unlocked(id)
    }
    pub fn rename(&mut self, target: NameTarget<'_>, name: String) -> Result<(), Status> {
        self.run(|this| {
            if name.contains('\0') {
                return Err(error("INVALID_NAME", "Name contains NUL"));
            }
            macro_rules! rename {
                ($id:expr, $get:ident, $replace:ident) => {{
                    let mut value = this
                        .builder
                        .$get($id)
                        .cloned()
                        .ok_or_else(|| Status::error("MISSING_OBJECT", $id))?;
                    this.unlocked($id)?;
                    value.name = name;
                    this.builder.$replace(value)
                }};
            }
            match target {
                NameTarget::Asset(id) => rename!(id, asset, replace_asset),
                NameTarget::Parameter(id) => rename!(id, parameter, replace_parameter),
                NameTarget::Part(id) => rename!(id, part, replace_part),
                NameTarget::Transform(id) => rename!(id, transform, replace_transform),
                NameTarget::Mesh(id) => rename!(id, mesh, replace_mesh),
            }
        })
    }
    /// Absolute base positions in the mesh's local space, addressed by stable IDs.
    pub fn set_vertex_positions(
        &mut self,
        mesh_id: &str,
        updates: &[(u32, Vec2)],
    ) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(mesh_id)?;
            let mut mesh = this
                .builder
                .mesh(mesh_id)
                .cloned()
                .ok_or_else(|| Status::error("MISSING_MESH", mesh_id))?;
            if mesh.vertex_ids.len() != mesh.base_positions.len() {
                return Err(error(
                    "INVALID_LENGTH",
                    "Vertex IDs and positions must match",
                ));
            }
            let slots: HashMap<_, _> = mesh
                .vertex_ids
                .iter()
                .enumerate()
                .map(|(i, id)| (*id, i))
                .collect();
            let mut seen = HashSet::new();
            for (id, position) in updates {
                if !seen.insert(*id) {
                    return Err(error("DUPLICATE_VERTEX", "Duplicate vertex update"));
                }
                if !position.x.is_finite() || !position.y.is_finite() {
                    return Err(error("INVALID_POSITION", "Vertex position must be finite"));
                }
                let slot = slots
                    .get(id)
                    .ok_or_else(|| error("MISSING_VERTEX", "Unknown vertex ID"))?;
                mesh.base_positions[*slot] = *position;
            }
            this.builder.replace_mesh(mesh)
        })
    }
    pub fn create_part(&mut self, part: Part) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(&part.parent_id)?;
            this.builder.create_part(part)
        })
    }
    pub fn create_mesh(&mut self, mesh: Mesh) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(&mesh.part_id)?;
            this.builder.create_mesh(mesh)
        })
    }
    pub fn create_empty_asset(&mut self, id: String, name: String) -> Result<(), Status> {
        self.run(|this| {
            this.builder.create_asset(AssetRecord {
                id,
                name,
                content: AssetContent::Empty,
            })
        })
    }
    /// The image was captured before the command; only its immutable key enters content.
    pub fn replace_image(&mut self, asset_id: &str, image: ImageResource) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(asset_id)?;
            let mut asset = this
                .builder
                .asset(asset_id)
                .cloned()
                .ok_or_else(|| Status::error("MISSING_ASSET", asset_id))?;
            asset.content = AssetContent::Image {
                resource: image.key(),
                width: image.width(),
                height: image.height(),
            };
            this.builder.replace_asset(asset)?;
            *this.resources = this.resources.with_images([image]);
            Ok(())
        })
    }
}

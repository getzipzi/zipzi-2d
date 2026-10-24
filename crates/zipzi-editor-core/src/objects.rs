//! One immutable, typed projection shared by document panels. Preview changes do not rebuild it.
mod bindings;
mod build;
mod context;
mod expressions;
mod influences;
mod motions;
mod organization;
mod pose;
mod properties;
mod scene;
mod table;
mod types;

use properties::deformer_properties;
use std::{collections::HashMap, sync::Arc};
use zipzi_core::Document as Model;
use zipzi_core::SharedVec;

pub use bindings::BindingInfo;
pub use context::{NavigationContext, PanelContext};
pub use influences::{InfluencePath, ParameterBindings};
pub use properties::editable_property;
pub use table::{SnapshotIter, SnapshotTable};
pub use types::{ObjectInfo, ObjectKind, ObjectRef, Relation, RelationKind};

#[derive(Clone, Debug, Default)]
pub struct ObjectIndex {
    pub objects: SnapshotTable<ObjectInfo>,
    pub relations: SnapshotTable<Relation>,
    pub bindings: SharedVec<BindingInfo>,
    pub draw_groups: SharedVec<zipzi_core::draw_order::DrawOrderGroup>,
    structure: Arc<ObjectStructure>,
}

/// Slot maps and dependency paths change only when rebuilding the structure.
#[derive(Clone, Debug, Default)]
struct ObjectStructure {
    slots: HashMap<ObjectRef, usize>,
    outgoing: HashMap<ObjectRef, Vec<usize>>,
    incoming: HashMap<ObjectRef, Vec<usize>>,
    binding_slots: HashMap<ObjectRef, usize>,
    parameters: HashMap<String, ParameterBindings>,
}
impl ObjectIndex {
    pub fn get(&self, r: &ObjectRef) -> Option<&ObjectInfo> {
        self.structure.slots.get(r).map(|&i| &self.objects[i])
    }
    pub fn binding(&self, r: &ObjectRef) -> Option<&BindingInfo> {
        self.structure
            .binding_slots
            .get(r)
            .map(|&i| &self.bindings[i])
    }
    pub fn parameter(&self, id: &str) -> &ParameterBindings {
        static EMPTY: ParameterBindings = ParameterBindings {
            regular: Vec::new(),
            blend: Vec::new(),
            indirect: Vec::new(),
        };
        self.structure.parameters.get(id).unwrap_or(&EMPTY)
    }
    pub fn outgoing(&self, r: &ObjectRef) -> impl Iterator<Item = &Relation> {
        self.structure
            .outgoing
            .get(r)
            .into_iter()
            .flatten()
            .map(|&i| &self.relations[i])
    }
    pub fn incoming(&self, r: &ObjectRef) -> impl Iterator<Item = &Relation> {
        self.structure
            .incoming
            .get(r)
            .into_iter()
            .flatten()
            .map(|&i| &self.relations[i])
    }
    pub fn name<'a>(&'a self, r: &'a ObjectRef) -> &'a str {
        self.get(r).map_or(&r.id, |i| &i.name)
    }
    /// Control drags only change pose data, never bindings or relationships.
    /// Refresh the inspector payload without rebuilding the influence graph.
    pub(crate) fn refresh_deformer(&mut self, model: &Model, object: &ObjectRef) {
        if let Some(transform) = model.get_transform(&object.id) {
            if let Some(&slot) = self.structure.slots.get(object) {
                self.objects[slot].properties = deformer_properties(transform);
            }
        }
    }

    pub fn draw_group(&self, mesh: &str) -> Option<ObjectRef> {
        self.draw_groups
            .iter()
            .find(|g| g.items.iter().any(|id| id == mesh))
            .map(|g| {
                if g.owner.is_empty() {
                    self.objects[0].reference.clone()
                } else {
                    ObjectRef::new(ObjectKind::Part, &g.owner)
                }
            })
    }
    pub fn influences_for(&self, target: &ObjectRef) -> Vec<&InfluencePath> {
        self.structure
            .parameters
            .values()
            .flat_map(|p| &p.indirect)
            .filter(|p| &p.target == target)
            .collect()
    }
}

mod references;

#[path = "objects/resources.rs"]
mod resources;

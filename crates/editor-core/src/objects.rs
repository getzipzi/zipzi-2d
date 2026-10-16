//! One immutable, typed projection shared by document panels. Preview changes do not rebuild it.
mod bindings;
mod build;
mod context;
mod influences;
mod properties;
mod types;

use kasane_core::Document as Model;
use properties::deformer_properties;
use std::collections::HashMap;

pub use bindings::BindingInfo;
pub use context::{NavigationContext, PanelContext};
pub use influences::{InfluencePath, ParameterBindings};
pub use properties::editable_property;
pub use types::{ObjectInfo, ObjectKind, ObjectRef, Relation, RelationKind};

#[derive(Clone, Debug, Default)]
pub struct ObjectIndex {
    pub objects: Vec<ObjectInfo>,
    pub relations: Vec<Relation>,
    pub bindings: Vec<BindingInfo>,
    pub draw_groups: Vec<kasane_core::draw_order::DrawOrderGroup>,
    slots: HashMap<ObjectRef, usize>,
    outgoing: HashMap<ObjectRef, Vec<usize>>,
    incoming: HashMap<ObjectRef, Vec<usize>>,
    binding_slots: HashMap<ObjectRef, usize>,
    parameters: HashMap<String, ParameterBindings>,
}
impl ObjectIndex {
    pub fn get(&self, r: &ObjectRef) -> Option<&ObjectInfo> {
        self.slots.get(r).map(|&i| &self.objects[i])
    }
    pub fn binding(&self, r: &ObjectRef) -> Option<&BindingInfo> {
        self.binding_slots.get(r).map(|&i| &self.bindings[i])
    }
    pub fn parameter(&self, id: &str) -> &ParameterBindings {
        static EMPTY: ParameterBindings = ParameterBindings {
            regular: Vec::new(),
            blend: Vec::new(),
            indirect: Vec::new(),
        };
        self.parameters.get(id).unwrap_or(&EMPTY)
    }
    pub fn outgoing(&self, r: &ObjectRef) -> impl Iterator<Item = &Relation> {
        self.outgoing
            .get(r)
            .into_iter()
            .flatten()
            .map(|&i| &self.relations[i])
    }
    pub fn incoming(&self, r: &ObjectRef) -> impl Iterator<Item = &Relation> {
        self.incoming
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
            if let Some(&slot) = self.slots.get(object) {
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
        self.parameters
            .values()
            .flat_map(|p| &p.indirect)
            .filter(|p| &p.target == target)
            .collect()
    }
}

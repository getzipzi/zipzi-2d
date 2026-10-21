//! Inspector field eligibility and deformer display payloads.
use super::ObjectKind;
use serde_json::{Value, json};

impl super::ObjectIndex {
    /// These mesh fields cannot change identities, relation edges or draw groups.
    pub(crate) fn refresh_mesh_presentation(
        &mut self,
        model: &kasane_core::Document,
        object: &super::ObjectRef,
    ) -> bool {
        let Some(mesh) = model.get_mesh(&object.id) else {
            return false;
        };
        let Some(&slot) = self.structure.slots.get(object) else {
            return false;
        };
        let name = if mesh.name.is_empty() {
            object.id.clone()
        } else {
            mesh.name.clone()
        };
        let appearance = json!(mesh.appearance);
        let previous = &self.objects[slot];
        if previous.name == name
            && previous.enabled == Some(mesh.enabled)
            && previous.properties["name"] == mesh.name
            && previous.properties["enabled"] == mesh.enabled
            && previous.properties["appearance"] == appearance
        {
            return true;
        }
        let info = &mut self.objects[slot];
        info.name = name;
        info.enabled = Some(mesh.enabled);
        info.properties["name"] = json!(mesh.name);
        info.properties["enabled"] = json!(mesh.enabled);
        info.properties["appearance"] = appearance;
        true
    }

    fn refresh_physics_preview(
        &mut self,
        model: &kasane_core::Document,
        object: &super::ObjectRef,
    ) -> bool {
        let Some(asset) = model.physics() else {
            return false;
        };
        let Ok(rig) = crate::property::physics::group(asset, object) else {
            return false;
        };
        let Some(&slot) = self.structure.slots.get(object) else {
            return false;
        };
        let fps = json!(asset.data.meta.fps.unwrap_or(30.));
        let fps_changed = self.objects[slot].properties["fps"] != fps;
        let info = &mut self.objects[slot];
        info.properties = json!(rig);
        info.name = asset
            .data
            .meta
            .dictionary
            .iter()
            .find(|entry| entry.id == rig.id)
            .map_or_else(|| rig.id.clone(), |entry| entry.name.clone());
        info.properties["name"] = json!(info.name);
        info.properties["fps"] = fps.clone();
        if fps_changed {
            let groups: Vec<_> = self
                .objects
                .iter()
                .enumerate()
                .filter(|(_, info)| {
                    info.reference.kind == ObjectKind::PhysicsGroup && info.properties["fps"] != fps
                })
                .map(|(slot, _)| slot)
                .collect();
            for slot in groups {
                self.objects[slot].properties["fps"] = fps.clone();
            }
        }
        let root = super::ObjectRef::new(ObjectKind::Physics, &asset.id);
        if let Some(&slot) = self.structure.slots.get(&root) {
            let fps = asset.data.meta.fps.map(|fps| json!(fps));
            if self.objects[slot].properties["meta"].get("Fps") != fps.as_ref() {
                let meta = &mut self.objects[slot].properties["meta"];
                if let Some(fps) = fps {
                    meta["Fps"] = fps;
                } else if let Some(meta) = meta.as_object_mut() {
                    meta.remove("Fps");
                }
            }
        }
        let mut inputs = rig.inputs.iter();
        let mut outputs = rig.outputs.iter();
        for &edge in self.structure.outgoing.get(object).into_iter().flatten() {
            let detail = match self.relations[edge].kind {
                super::RelationKind::PhysicsInput => {
                    let Some(input) = inputs.next() else {
                        return false;
                    };
                    format!(
                        "{:?} · weight {} · reflect {}; saved configuration",
                        input.kind, input.weight, input.reflect
                    )
                }
                super::RelationKind::PhysicsOutput => {
                    let Some(output) = outputs.next() else {
                        return false;
                    };
                    format!(
                        "{:?} · vertex {} · weight {} · scale {}; saved configuration",
                        output.kind, output.vertex_index, output.weight, output.scale
                    )
                }
                _ => continue,
            };
            if self.relations[edge].detail != detail {
                self.relations[edge].detail = detail;
            }
        }
        true
    }

    /// Appearance candidates preserve identities and relation edges. Copy their
    /// payloads without rebuilding bindings or influence paths.
    pub(crate) fn refresh_property_preview(
        &mut self,
        model: &kasane_core::Document,
        object: &super::ObjectRef,
        keyform_order_changed: bool,
    ) -> bool {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = kasane_core::benchmark::span("editor.object_index_refresh");
        if object.kind == ObjectKind::PhysicsGroup {
            return self.refresh_physics_preview(model, object);
        }
        if object.kind == ObjectKind::Mesh && !self.refresh_mesh_presentation(model, object) {
            return false;
        }
        let Ok(values) =
            crate::Document::property_values_in(model, object, &crate::PropertyScope::Object)
        else {
            return false;
        };
        let Some(&slot) = self.structure.slots.get(object) else {
            return false;
        };
        let mut draw_order_changed = keyform_order_changed;
        let mut updates = Vec::new();
        for spec in crate::property_specs(object.kind)
            .iter()
            .filter(|s| s.effect != crate::property::PropertyEffect::Structure)
        {
            let Some(value) = values.get(&spec.key) else {
                return false;
            };
            let mut field = &self.objects[slot].properties;
            for key in spec.path.split('.') {
                let Some(next) = field.get(key) else {
                    return false;
                };
                field = next;
            }
            let value = value.json();
            if *field != value {
                draw_order_changed |= spec.effect == crate::property::PropertyEffect::DrawOrder;
                updates.push((spec.path, value));
            }
        }
        if !updates.is_empty() {
            let properties = &mut self.objects[slot].properties;
            for (path, value) in updates {
                let mut field = &mut *properties;
                for key in path.split('.') {
                    field = field.get_mut(key).expect("property path checked");
                }
                *field = value;
            }
        }
        if draw_order_changed {
            // Draw-order candidates also affect the resolved group bounds.
            self.draw_groups = kasane_core::draw_order::resolved_groups(model).into();
        }
        true
    }
}

/// Writable authoring fields are guarded by the same descriptor policy as the inspector.
pub fn editable_property(kind: ObjectKind, field: &str) -> bool {
    crate::property_specs(kind)
        .iter()
        .any(|spec| spec.path == field && !spec.readonly)
}

pub(super) fn deformer_properties(x: &kasane_core::Transform) -> Value {
    json!({"name":x.name,"runtime_id":x.runtime_id,"kind":x.kind(),"part_id":x.part(),"parent_id":x.parent(),"appearance":x.appearance,"enabled":x.enabled,"warp":x.warp(),"rotation":x.rotation()})
}

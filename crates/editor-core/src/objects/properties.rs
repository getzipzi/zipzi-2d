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

    pub(crate) fn refresh_physics_properties(
        &mut self,
        model: &kasane_core::Document,
        owners: &[String],
    ) -> bool {
        let Some(asset) = model.physics() else {
            return false;
        };
        let root = super::ObjectRef::new(ObjectKind::Physics, &asset.id);
        let Some(&slot) = self.structure.slots.get(&root) else {
            return false;
        };
        let payload = json!({"meta":asset.data.meta,"extensions":asset.data.extensions});
        let fps_changed =
            self.objects[slot].properties["meta"].get("Fps") != payload["meta"].get("Fps");
        if self.objects[slot].properties != payload {
            self.objects[slot].properties = payload;
        }
        if fps_changed {
            self.refresh_physics_fps(asset);
        }
        owners.iter().all(|id| {
            self.refresh_physics_preview(
                model,
                &super::ObjectRef::new(ObjectKind::PhysicsGroup, id),
            )
        })
    }

    // FPS belongs to the asset even though each group exposes it in Properties.
    // Asset-only receipts have no modified group, so synchronize it independently.
    fn refresh_physics_fps(&mut self, asset: &kasane_core::document::PhysicsAsset) {
        let fps = json!(asset.data.meta.fps.unwrap_or(30.));
        let changed: Vec<_> = asset
            .data
            .settings
            .iter()
            .filter_map(|group| {
                self.structure
                    .slots
                    .get(&super::ObjectRef::new(ObjectKind::PhysicsGroup, &group.id))
                    .copied()
            })
            .filter(|&slot| self.objects[slot].properties["fps"] != fps)
            .collect();
        for slot in changed {
            self.objects[slot].properties["fps"] = fps.clone();
        }
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
        info.properties["fps"] = fps;
        if fps_changed {
            self.refresh_physics_fps(asset);
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
        let children = rig
            .inputs
            .iter()
            .map(|v| (ObjectKind::PhysicsInput, &v.id, json!(v)))
            .chain(
                rig.outputs
                    .iter()
                    .map(|v| (ObjectKind::PhysicsOutput, &v.id, json!(v))),
            )
            .chain(
                rig.vertices
                    .iter()
                    .map(|v| (ObjectKind::PhysicsNode, &v.id, json!(v))),
            );
        for (kind, id, payload) in children {
            let child = super::ObjectRef::new(kind, id);
            let Some(&slot) = self.structure.slots.get(&child) else {
                return false;
            };
            if self.objects[slot].properties != payload {
                self.objects[slot].properties = payload;
            }
        }
        for input in &rig.inputs {
            let child = super::ObjectRef::new(ObjectKind::PhysicsInput, &input.id);
            let slot = self.structure.slots[&child];
            let name = format!("{:?} input", input.kind);
            if self.objects[slot].name != name {
                self.objects[slot].name = name;
            }
            let detail = format!(
                "{:?} · weight {} · reflect {}; saved configuration",
                input.kind, input.weight, input.reflect
            );
            for &edge in self.structure.outgoing.get(&child).into_iter().flatten() {
                if self.relations[edge].kind == super::RelationKind::PhysicsInput
                    && self.relations[edge].detail != detail
                {
                    self.relations[edge].detail = detail.clone();
                }
            }
        }
        for output in &rig.outputs {
            let child = super::ObjectRef::new(ObjectKind::PhysicsOutput, &output.id);
            let slot = self.structure.slots[&child];
            let name = format!("{:?} output", output.kind);
            if self.objects[slot].name != name {
                self.objects[slot].name = name;
            }
            let detail = format!(
                "{:?} · node {} · weight {} · scale {}; saved configuration",
                output.kind, output.node_id, output.weight, output.scale
            );
            for &edge in self.structure.outgoing.get(&child).into_iter().flatten() {
                if self.relations[edge].kind == super::RelationKind::PhysicsOutput
                    && self.relations[edge].detail != detail
                {
                    self.relations[edge].detail = detail.clone();
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
                        "{:?} · node {} · weight {} · scale {}; saved configuration",
                        output.kind, output.node_id, output.weight, output.scale
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

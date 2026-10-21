//! Compact executable channels carry explicit native slot maps.
use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum PhysicsDisabledReason {
    NeedsConfiguration,
    UnresolvedParameter { runtime_id: String },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DisabledPhysicsChannel {
    pub id: String,
    pub group_id: String,
    pub reason: PhysicsDisabledReason,
}
#[derive(Debug, Clone, Default)]
pub struct PhysicsSlotMap {
    pub native_to_runtime: BTreeMap<String, Option<usize>>,
    pub runtime_to_native: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct PhysicsGroupSlots {
    pub group_id: String,
    pub inputs: PhysicsSlotMap,
    pub outputs: PhysicsSlotMap,
    pub nodes: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct ExecutablePhysics {
    pub data: crate::physics::PhysicsDefinition,
    pub parameter_bindings: BTreeMap<String, String>,
}
#[derive(Debug, Clone)]
pub struct PhysicsProjection {
    pub revision: u64,
    pub executable: ExecutablePhysics,
    pub groups: Vec<PhysicsGroupSlots>,
    pub disabled: Vec<DisabledPhysicsChannel>,
}
impl PhysicsTarget {
    pub fn parameter_id(&self) -> Option<&str> {
        if let Self::Resolved { parameter_id } = self {
            Some(parameter_id)
        } else {
            None
        }
    }
    pub fn disabled_reason(&self) -> Option<PhysicsDisabledReason> {
        match self {
            Self::Unspecified => Some(PhysicsDisabledReason::NeedsConfiguration),
            Self::Unresolved { runtime_id } => Some(PhysicsDisabledReason::UnresolvedParameter {
                runtime_id: runtime_id.clone(),
            }),
            Self::Resolved { .. } => None,
        }
    }
}
impl Document {
    pub fn physics_projection(&self) -> Result<Option<PhysicsProjection>, Status> {
        let Some(asset) = self.physics() else {
            return Ok(None);
        };
        let mut data = asset.data.to_wire(|t| t.validation_key());
        let mut groups = vec![];
        let mut disabled = vec![];
        let mut bindings = BTreeMap::new();
        for (native, wire) in asset.data.settings.iter().zip(&mut data.settings) {
            self.validate_native_physics_rig(native)?;
            let wire = std::sync::Arc::make_mut(wire);
            let mut inputs = PhysicsSlotMap::default();
            let mut outputs = PhysicsSlotMap::default();
            let mut input_index = 0;
            wire.inputs.retain(|_| {
                let v = &native.inputs[input_index];
                input_index += 1;
                map_slot(
                    self,
                    &v.id,
                    &native.id,
                    &v.source,
                    &mut inputs,
                    &mut disabled,
                    &mut bindings,
                )
            });
            let mut output_index = 0;
            wire.outputs.retain(|_| {
                let v = &native.outputs[output_index];
                output_index += 1;
                map_slot(
                    self,
                    &v.id,
                    &native.id,
                    &v.destination,
                    &mut outputs,
                    &mut disabled,
                    &mut bindings,
                )
            });
            groups.push(PhysicsGroupSlots {
                group_id: native.id.clone(),
                inputs,
                outputs,
                nodes: native.vertices.iter().map(|v| v.id.clone()).collect(),
            });
        }
        let (g, i, o, n) = data.actual_counts();
        data.meta.setting_count = g;
        data.meta.input_count = i;
        data.meta.output_count = o;
        data.meta.vertex_count = n;
        Ok(Some(PhysicsProjection {
            revision: self.revision(),
            executable: ExecutablePhysics {
                data,
                parameter_bindings: bindings,
            },
            groups,
            disabled,
        }))
    }
}
fn map_slot(
    doc: &Document,
    id: &str,
    group: &str,
    target: &PhysicsTarget,
    map: &mut PhysicsSlotMap,
    disabled: &mut Vec<DisabledPhysicsChannel>,
    bindings: &mut BTreeMap<String, String>,
) -> bool {
    if let Some(reason) = target.disabled_reason() {
        map.native_to_runtime.insert(id.into(), None);
        disabled.push(DisabledPhysicsChannel {
            id: id.into(),
            group_id: group.into(),
            reason,
        });
        false
    } else {
        let parameter = target.parameter_id().expect("resolved target");
        debug_assert!(doc.get_parameter(parameter).is_some());
        bindings.insert(parameter.into(), parameter.into());
        map.native_to_runtime
            .insert(id.into(), Some(map.runtime_to_native.len()));
        map.runtime_to_native.push(id.into());
        true
    }
}
impl PhysicsProjection {
    pub fn estimated_bytes(&self) -> usize {
        // Charge shared projection storage conservatively in each seek checkpoint.
        fn strings(v: &Vec<String>) -> usize {
            v.capacity() * std::mem::size_of::<String>()
                + v.iter().map(String::capacity).sum::<usize>()
        }
        fn map(v: &PhysicsSlotMap) -> usize {
            strings(&v.runtime_to_native)
                + v.native_to_runtime
                    .keys()
                    .map(|id| id.capacity() + 128)
                    .sum::<usize>()
        }
        std::mem::size_of::<Self>()
            + self.groups.capacity() * std::mem::size_of::<PhysicsGroupSlots>()
            + self
                .groups
                .iter()
                .map(|g| {
                    g.group_id.capacity() + map(&g.inputs) + map(&g.outputs) + strings(&g.nodes)
                })
                .sum::<usize>()
            + self
                .executable
                .data
                .settings
                .iter()
                .map(|g| {
                    std::mem::size_of::<crate::physics::PhysicsSetting>()
                        + g.id.capacity()
                        + g.inputs.capacity() * std::mem::size_of::<crate::physics::PhysicsInput>()
                        + g.outputs.capacity()
                            * std::mem::size_of::<crate::physics::PhysicsOutput>()
                        + g.vertices.capacity()
                            * std::mem::size_of::<crate::physics::PhysicsVertex>()
                        + g.inputs
                            .iter()
                            .map(|v| v.source.id.capacity() + v.source.target.capacity())
                            .sum::<usize>()
                        + g.outputs
                            .iter()
                            .map(|v| v.destination.id.capacity() + v.destination.target.capacity())
                            .sum::<usize>()
                })
                .sum::<usize>()
            + self
                .executable
                .parameter_bindings
                .iter()
                .map(|(k, v)| k.capacity() + v.capacity() + 128)
                .sum::<usize>()
            + self
                .disabled
                .iter()
                .map(|d| {
                    std::mem::size_of::<DisabledPhysicsChannel>()
                        + d.id.capacity()
                        + d.group_id.capacity()
                        + match &d.reason {
                            PhysicsDisabledReason::UnresolvedParameter { runtime_id } => {
                                runtime_id.capacity()
                            }
                            _ => 0,
                        }
                })
                .sum::<usize>()
    }
}

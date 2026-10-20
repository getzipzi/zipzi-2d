//! Typed local writes. Unchanged rigs remain shared with snapshots and history.
use super::*;
use crate::physics::{validate_physics_setting, PhysicsAxis, PhysicsRange, PhysicsSetting};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsPatch {
    pub asset_id: String,
    pub group_id: String,
    pub changes: Vec<PhysicsProperty>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PhysicsProperty {
    Fps(Option<f32>),
    Name(String),
    InputWeight { index: usize, value: f32 },
    InputAxis { index: usize, value: PhysicsAxis },
    InputReflect { index: usize, value: bool },
    InputParameter { index: usize, parameter_id: String },
    OutputWeight { index: usize, value: f32 },
    OutputScale { index: usize, value: f32 },
    OutputAxis { index: usize, value: PhysicsAxis },
    OutputReflect { index: usize, value: bool },
    OutputParameter { index: usize, parameter_id: String },
    OutputVertex { index: usize, value: usize },
    VertexRadius { index: usize, value: f32 },
    VertexMobility { index: usize, value: f32 },
    VertexDelay { index: usize, value: f32 },
    VertexAcceleration { index: usize, value: f32 },
    PositionNormalization(PhysicsRange),
    AngleNormalization(PhysicsRange),
}
struct Prepared {
    index: usize,
    rig: Option<Arc<PhysicsSetting>>,
    fps: Option<Option<f32>>,
    name: Option<String>,
    bindings: Option<BTreeMap<String, String>>,
}
impl Prepared {
    fn changed(&self) -> bool {
        self.rig.is_some() || self.fps.is_some() || self.name.is_some() || self.bindings.is_some()
    }
}
impl Document {
    fn prepare_physics_patch(&self, patch: &PhysicsPatch) -> Result<Prepared, Status> {
        if self.mutation_blocked() {
            return Err(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        let asset = self
            .physics()
            .filter(|asset| asset.id == patch.asset_id)
            .ok_or_else(|| Status::error("MISSING_PHYSICS", &patch.asset_id))?;
        let index = asset
            .data
            .settings
            .iter()
            .position(|rig| rig.id == patch.group_id)
            .ok_or_else(|| Status::error("MISSING_PHYSICS_GROUP", &patch.group_id))?;
        let original = &asset.data.settings[index];
        let edits_rig = patch
            .changes
            .iter()
            .any(|change| !matches!(change, PhysicsProperty::Fps(_) | PhysicsProperty::Name(_)));
        let mut rig = edits_rig.then(|| original.as_ref().clone());
        let mut fps = None;
        let mut name = None;
        let mut bindings = None;
        let mut binding_changed = false;
        macro_rules! item {
            ($collection:ident,$i:expr) => {
                rig.as_mut()
                    .unwrap()
                    .$collection
                    .get_mut(*$i)
                    .ok_or_else(|| {
                        Status::error(
                            "INVALID_PHYSICS_INDEX",
                            format!("{}[{}]", stringify!($collection), $i),
                        )
                    })?
            };
        }
        for change in &patch.changes {
            use PhysicsProperty::*;
            match change {
                Fps(value) => {
                    if value.is_some_and(|v| !v.is_finite() || v < 0.) {
                        return Err(Status::error(
                            "INVALID_FPS",
                            "FPS must be nonnegative and finite",
                        ));
                    }
                    fps = Some(*value);
                }
                Name(value) => name = Some(value.clone()),
                InputWeight { index, value } => item!(inputs, index).weight = *value,
                InputAxis { index, value } => item!(inputs, index).kind = *value,
                InputReflect { index, value } => item!(inputs, index).reflect = *value,
                OutputWeight { index, value } => item!(outputs, index).weight = *value,
                OutputScale { index, value } => item!(outputs, index).scale = *value,
                OutputAxis { index, value } => item!(outputs, index).kind = *value,
                OutputReflect { index, value } => item!(outputs, index).reflect = *value,
                OutputVertex { index, value } => item!(outputs, index).vertex_index = *value,
                VertexRadius { index, value } => item!(vertices, index).radius = *value,
                VertexMobility { index, value } => item!(vertices, index).mobility = *value,
                VertexDelay { index, value } => item!(vertices, index).delay = *value,
                VertexAcceleration { index, value } => item!(vertices, index).acceleration = *value,
                PositionNormalization(value) => {
                    rig.as_mut().unwrap().normalization.position = *value
                }
                AngleNormalization(value) => rig.as_mut().unwrap().normalization.angle = *value,
                InputParameter {
                    index,
                    parameter_id,
                }
                | OutputParameter {
                    index,
                    parameter_id,
                } => {
                    let parameter = self
                        .get_parameter(parameter_id)
                        .ok_or_else(|| Status::error("MISSING_PARAMETER", parameter_id))?;
                    let current_runtime = parameter.runtime_id.as_str();
                    if current_runtime.is_empty() {
                        return Err(Status::error(
                            "INVALID_PHYSICS_TARGET",
                            "Parameter needs a runtime ID",
                        ));
                    }
                    let map = bindings.get_or_insert_with(|| asset.parameter_bindings.clone());
                    let runtime =
                        asset.parameter_binding_key_in(map, parameter_id, current_runtime);
                    let reference = if matches!(change, InputParameter { .. }) {
                        &mut item!(inputs, index).source
                    } else {
                        &mut item!(outputs, index).destination
                    };
                    reference.target = "Parameter".into();
                    reference.id = runtime.clone();
                    map.insert(runtime, parameter_id.clone());
                    binding_changed = true;
                }
            }
        }
        if let Some(rig) = &rig {
            validate_physics_setting(rig).map_err(|error| {
                Status::error(error.code, format!("{}: {}", error.path, error.message))
            })?;
        }
        if binding_changed {
            let referenced: HashSet<&str> = asset
                .data
                .settings
                .iter()
                .enumerate()
                .flat_map(|(i, old)| {
                    let setting = if i == index {
                        rig.as_ref().unwrap()
                    } else {
                        old.as_ref()
                    };
                    setting
                        .inputs
                        .iter()
                        .map(|i| i.source.id.as_str())
                        .chain(setting.outputs.iter().map(|o| o.destination.id.as_str()))
                })
                .collect();
            bindings
                .as_mut()
                .unwrap()
                .retain(|runtime, _| referenced.contains(runtime.as_str()));
        }
        let current_name = asset
            .data
            .meta
            .dictionary
            .iter()
            .find(|entry| entry.id == patch.group_id)
            .map(|entry| entry.name.as_str());
        Ok(Prepared {
            index,
            rig: rig.filter(|rig| rig != original.as_ref()).map(Arc::new),
            fps: fps.filter(|fps| *fps != asset.data.meta.fps),
            name: name.filter(|name| Some(name.as_str()) != current_name),
            bindings: bindings.filter(|bindings| bindings != &asset.parameter_bindings),
        })
    }
    /// Validate without mutating a candidate or aborting its SDK edit.
    pub fn validate_physics_patch(&self, patch: &PhysicsPatch) -> Status {
        self.prepare_physics_patch(patch)
            .map_or_else(|error| error, |_| Status::ok())
    }
    /// Atomically update fields in one rig. A rejected patch leaves all fields intact.
    pub fn patch_physics(&mut self, patch: &PhysicsPatch) -> EditResult {
        let prepared = match self.prepare_physics_patch(patch) {
            Ok(prepared) => prepared,
            Err(status) => return self.failed(status),
        };
        if !prepared.changed() {
            return self.failed(Status::ok());
        }
        let asset = Arc::make_mut(self.content.physics.as_mut().unwrap());
        if let Some(rig) = prepared.rig {
            asset.data.settings[prepared.index] = rig;
        }
        if let Some(fps) = prepared.fps {
            asset.data.meta.fps = fps;
        }
        if let Some(name) = prepared.name {
            asset
                .data
                .meta
                .dictionary
                .iter_mut()
                .find(|entry| entry.id == patch.group_id)
                .expect("validated physics dictionary")
                .name = name;
        }
        if let Some(bindings) = prepared.bindings {
            asset.parameter_bindings = bindings;
        }
        self.changed(ChangeKind::Metadata, vec![], vec![patch.asset_id.clone()])
    }
}

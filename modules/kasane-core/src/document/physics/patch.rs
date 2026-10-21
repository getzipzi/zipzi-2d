//! Typed local writes. Unchanged rigs remain shared with snapshots and history.
use super::*;
use crate::physics::{PhysicsAxis, PhysicsRange};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Atomic asset edits plus optional rig edits; asset-only patches support empty assets.
pub struct PhysicsPatch {
    pub asset_id: String,
    pub asset_changes: Vec<PhysicsAssetProperty>,
    pub rig: Option<PhysicsRigPatch>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PhysicsAssetProperty {
    /// None or zero follows the evaluation delta; positive values select a fixed step.
    Fps(Option<f32>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsRigPatch {
    pub group_id: String,
    pub changes: Vec<PhysicsProperty>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PhysicsProperty {
    Name(String),
    InputWeight {
        id: String,
        value: f32,
    },
    InputAxis {
        id: String,
        value: PhysicsAxis,
    },
    InputReflect {
        id: String,
        value: bool,
    },
    InputParameter {
        id: String,
        target: PhysicsTarget,
    },
    OutputWeight {
        id: String,
        value: f32,
    },
    OutputScale {
        id: String,
        value: f32,
    },
    OutputAxis {
        id: String,
        value: PhysicsAxis,
    },
    OutputReflect {
        id: String,
        value: bool,
    },
    OutputParameter {
        id: String,
        target: PhysicsTarget,
    },
    OutputNode {
        id: String,
        node_id: String,
    },
    VertexPosition {
        id: String,
        value: crate::physics::PhysicsVector,
    },
    VertexRadius {
        id: String,
        value: f32,
    },
    VertexMobility {
        id: String,
        value: f32,
    },
    VertexDelay {
        id: String,
        value: f32,
    },
    VertexAcceleration {
        id: String,
        value: f32,
    },
    PositionNormalization(PhysicsRange),
    AngleNormalization(PhysicsRange),
}
struct Prepared {
    index: usize,
    rig: Option<Arc<PhysicsSetting>>,
    fps: Option<Option<f32>>,
    name: Option<String>,
}
impl Prepared {
    fn changed(&self) -> bool {
        self.rig.is_some() || self.fps.is_some() || self.name.is_some()
    }
}
impl Document {
    fn prepare_physics_patch(&self, patch: &PhysicsPatch) -> Result<Prepared, Status> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = crate::benchmark::span("core.prepare_physics_patch");
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
        self.physics_unlocked(&asset.id)?;
        let mut fps = None;
        for change in &patch.asset_changes {
            match change {
                PhysicsAssetProperty::Fps(value) => {
                    if value.is_some_and(|v| !v.is_finite() || v < 0.) {
                        return Err(Status::error(
                            "INVALID_FPS",
                            "FPS must be nonnegative and finite",
                        ));
                    }
                    fps = Some(*value);
                }
            }
        }
        let fps = fps.filter(|fps| *fps != asset.data.meta.fps);
        let Some(patch) = &patch.rig else {
            return Ok(Prepared {
                index: 0,
                rig: None,
                fps,
                name: None,
            });
        };
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
            .any(|change| !matches!(change, PhysicsProperty::Name(_)));
        let mut rig = edits_rig.then(|| original.as_ref().clone());
        let mut name = None;
        macro_rules! item {
            ($collection:ident,$i:expr) => {
                rig.as_mut()
                    .unwrap()
                    .$collection
                    .iter_mut()
                    .find(|v| v.id == *$i)
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
                Name(value) => name = Some(value.clone()),
                InputWeight { id, value } => item!(inputs, id).weight = *value,
                InputAxis { id, value } => item!(inputs, id).kind = *value,
                InputReflect { id, value } => item!(inputs, id).reflect = *value,
                OutputWeight { id, value } => item!(outputs, id).weight = *value,
                OutputScale { id, value } => item!(outputs, id).scale = *value,
                OutputAxis { id, value } => item!(outputs, id).kind = *value,
                OutputReflect { id, value } => item!(outputs, id).reflect = *value,
                OutputNode { id, node_id } => item!(outputs, id).node_id = node_id.clone(),
                VertexPosition { id, value } => item!(vertices, id).position = *value,
                VertexRadius { id, value } => item!(vertices, id).radius = *value,
                VertexMobility { id, value } => item!(vertices, id).mobility = *value,
                VertexDelay { id, value } => item!(vertices, id).delay = *value,
                VertexAcceleration { id, value } => item!(vertices, id).acceleration = *value,
                PositionNormalization(value) => {
                    rig.as_mut().unwrap().normalization.position = *value
                }
                AngleNormalization(value) => rig.as_mut().unwrap().normalization.angle = *value,
                InputParameter { id, target } => item!(inputs, id).source = target.clone(),
                OutputParameter { id, target } => item!(outputs, id).destination = target.clone(),
            }
        }
        if let Some(rig) = &rig {
            self.validate_native_physics_rig(rig)?;
        }
        self.physics_unlocked(&patch.group_id)?;
        for change in &patch.changes {
            if let Some(id) = change.entity_id() {
                // The typed item lookup above already proved membership in
                // this unlocked group. Do not scan other rigs for its owner.
                if self.editor_state().get(id).locked {
                    return Err(Status::error("OBJECT_LOCKED", id));
                }
            }
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
            fps,
            name: name.filter(|name| Some(name.as_str()) != current_name),
        })
    }
    /// Validate without mutating a candidate or aborting its SDK edit.
    pub fn validate_physics_patch(&self, patch: &PhysicsPatch) -> Status {
        self.prepare_physics_patch(patch)
            .map_or_else(|error| error, |_| Status::ok())
    }
    /// Atomically update asset fields and optionally one rig. A rejected patch leaves all fields intact.
    pub fn patch_physics(&mut self, patch: &PhysicsPatch) -> EditResult {
        let prepared = match self.prepare_physics_patch(patch) {
            Ok(prepared) => prepared,
            Err(status) => return self.failed(status),
        };
        if !prepared.changed() {
            return self.failed(Status::ok());
        }
        let evaluation_changed = prepared.rig.is_some() || prepared.fps.is_some();
        let asset = Arc::make_mut(self.content.physics.as_mut().unwrap());
        prepared.apply(asset, patch);
        let result = self.changed(ChangeKind::Metadata, vec![], vec![patch.asset_id.clone()]);
        if evaluation_changed {
            self.evaluation_revision = self.revision;
        }
        result
    }
}
impl PhysicsProperty {
    pub fn entity_id(&self) -> Option<&str> {
        use PhysicsProperty::*;
        match self {
            InputWeight { id, .. }
            | InputAxis { id, .. }
            | InputReflect { id, .. }
            | InputParameter { id, .. }
            | OutputWeight { id, .. }
            | OutputScale { id, .. }
            | OutputAxis { id, .. }
            | OutputReflect { id, .. }
            | OutputParameter { id, .. }
            | OutputNode { id, .. }
            | VertexPosition { id, .. }
            | VertexRadius { id, .. }
            | VertexMobility { id, .. }
            | VertexDelay { id, .. }
            | VertexAcceleration { id, .. } => Some(id),
            _ => None,
        }
    }
}

impl Prepared {
    fn apply(self, asset: &mut PhysicsAsset, patch: &PhysicsPatch) {
        if let Some(rig) = self.rig {
            asset.data.settings[self.index] = rig;
        }
        if let Some(fps) = self.fps {
            asset.data.meta.fps = fps;
        }
        if let Some(name) = self.name {
            asset
                .data
                .meta
                .dictionary
                .iter_mut()
                .find(|entry| entry.id == patch.rig.as_ref().unwrap().group_id)
                .expect("validated physics dictionary")
                .name = name;
        }
    }
}

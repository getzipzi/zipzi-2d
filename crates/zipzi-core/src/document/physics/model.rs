//! Native identities and references are deliberately separate from physics3 DTOs.
use super::*;
use crate::physics as wire;
use std::sync::Arc;

pub use zipzi_contract::physics_document::PhysicsTarget;
pub(super) trait PhysicsTargetValidation {
    fn validation_key(&self) -> String;
}
impl PhysicsTargetValidation for PhysicsTarget {
    fn validation_key(&self) -> String {
        match self {
            Self::Unspecified => "__unconfigured__".into(),
            Self::Resolved { parameter_id } => parameter_id.clone(),
            Self::Unresolved { runtime_id } => runtime_id.clone(),
        }
    }
}
pub use zipzi_contract::physics_document::{
    PhysicsDefinition, PhysicsInput, PhysicsNode, PhysicsOutput, PhysicsSetting,
};

pub trait PhysicsSettingWire {
    fn to_wire(&self, map: &impl Fn(&PhysicsTarget) -> String) -> wire::PhysicsSetting;
}
impl PhysicsSettingWire for PhysicsSetting {
    fn to_wire(&self, map: &impl Fn(&PhysicsTarget) -> String) -> wire::PhysicsSetting {
        wire::PhysicsSetting {
            id: self.external_id.clone(),
            inputs: self
                .inputs
                .iter()
                .map(|v| wire::PhysicsInput {
                    source: wire::PhysicsParameterRef {
                        target: "Parameter".into(),
                        id: map(&v.source),
                        extensions: v.reference_extensions.clone(),
                    },
                    weight: v.weight,
                    kind: v.kind,
                    reflect: v.reflect,
                    extensions: v.extensions.clone(),
                })
                .collect(),
            outputs: self
                .outputs
                .iter()
                .map(|v| wire::PhysicsOutput {
                    destination: wire::PhysicsParameterRef {
                        target: "Parameter".into(),
                        id: map(&v.destination),
                        extensions: v.reference_extensions.clone(),
                    },
                    vertex_index: self.node_index(&v.node_id).unwrap_or(usize::MAX),
                    scale: v.scale,
                    weight: v.weight,
                    kind: v.kind,
                    reflect: v.reflect,
                    extensions: v.extensions.clone(),
                })
                .collect(),
            vertices: self
                .vertices
                .iter()
                .map(|v| wire::PhysicsVertex {
                    position: v.position,
                    mobility: v.mobility,
                    delay: v.delay,
                    acceleration: v.acceleration,
                    radius: v.radius,
                    extensions: v.extensions.clone(),
                })
                .collect(),
            normalization: self.normalization.clone(),
            extensions: self.extensions.clone(),
        }
    }
}

pub trait PhysicsDefinitionWire {
    fn to_wire(&self, map: impl Fn(&PhysicsTarget) -> String) -> wire::PhysicsDefinition;
}
impl PhysicsDefinitionWire for PhysicsDefinition {
    fn to_wire(&self, map: impl Fn(&PhysicsTarget) -> String) -> wire::PhysicsDefinition {
        let mut meta = self.meta.clone();
        for d in &mut meta.dictionary {
            if let Some(g) = self.settings.iter().find(|g| g.id == d.id) {
                d.id = g.external_id.clone();
            }
        }
        let mut result = wire::PhysicsDefinition {
            version: self.version,
            meta,
            settings: self
                .settings
                .iter()
                .map(|g| Arc::new(g.to_wire(&map)))
                .collect(),
            extensions: self.extensions.clone(),
        };
        let (g, i, o, n) = result.actual_counts();
        result.meta.setting_count = g;
        result.meta.input_count = i;
        result.meta.output_count = o;
        result.meta.vertex_count = n;
        result
    }
}

pub(super) trait PhysicsAssetIdentity {
    fn identity_roles(&self) -> std::collections::HashMap<&str, u8>;
}
impl PhysicsAssetIdentity for PhysicsAsset {
    fn identity_roles(&self) -> std::collections::HashMap<&str, u8> {
        let mut roles = std::collections::HashMap::from([(self.id.as_str(), 0)]);
        for g in &self.data.settings {
            roles.insert(g.id.as_str(), 1);
            roles.extend(g.inputs.iter().map(|v| (v.id.as_str(), 2)));
            roles.extend(g.outputs.iter().map(|v| (v.id.as_str(), 3)));
            roles.extend(g.vertices.iter().map(|v| (v.id.as_str(), 4)));
        }
        roles
    }
}
pub trait PhysicsAssetImport: Sized {
    fn from_wire(
        id: String,
        data: wire::PhysicsDefinition,
        resolve: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, Status>;
}
impl PhysicsAssetImport for PhysicsAsset {
    fn from_wire(
        id: String,
        data: wire::PhysicsDefinition,
        resolve: impl Fn(&str) -> Option<String>,
    ) -> Result<Self, Status> {
        wire::validate_physics_definition(&data)
            .map_err(|e| Status::error(e.code, format!("{}: {}", e.path, e.message)))?;
        let mut meta = data.meta;
        let settings = data
            .settings
            .iter()
            .map(|g| {
                let id = uuid::Uuid::new_v4().to_string();
                if let Some(d) = meta.dictionary.iter_mut().find(|d| d.id == g.id) {
                    d.id = id.clone();
                }
                let nodes: Vec<_> = g
                    .vertices
                    .iter()
                    .map(|v| PhysicsNode {
                        id: uuid::Uuid::new_v4().to_string(),
                        position: v.position,
                        mobility: v.mobility,
                        delay: v.delay,
                        acceleration: v.acceleration,
                        radius: v.radius,
                        extensions: v.extensions.clone(),
                    })
                    .collect();
                let target = |id: &str| {
                    resolve(id).map_or_else(
                        || PhysicsTarget::Unresolved {
                            runtime_id: id.into(),
                        },
                        |parameter_id| PhysicsTarget::Resolved { parameter_id },
                    )
                };
                Arc::new(PhysicsSetting {
                    id,
                    external_id: g.id.clone(),
                    inputs: g
                        .inputs
                        .iter()
                        .map(|v| PhysicsInput {
                            id: uuid::Uuid::new_v4().to_string(),
                            source: target(&v.source.id),
                            reference_extensions: v.source.extensions.clone(),
                            weight: v.weight,
                            kind: v.kind,
                            reflect: v.reflect,
                            extensions: v.extensions.clone(),
                        })
                        .collect(),
                    outputs: g
                        .outputs
                        .iter()
                        .map(|v| PhysicsOutput {
                            id: uuid::Uuid::new_v4().to_string(),
                            destination: target(&v.destination.id),
                            reference_extensions: v.destination.extensions.clone(),
                            node_id: nodes[v.vertex_index].id.clone(),
                            scale: v.scale,
                            weight: v.weight,
                            kind: v.kind,
                            reflect: v.reflect,
                            extensions: v.extensions.clone(),
                        })
                        .collect(),
                    vertices: nodes,
                    normalization: g.normalization.clone(),
                    extensions: g.extensions.clone(),
                })
            })
            .collect();
        let mut asset = Self {
            id,
            data: PhysicsDefinition {
                version: data.version,
                meta,
                settings,
                extensions: data.extensions,
            },
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        };
        asset.reconcile_counts();
        Ok(asset)
    }
}

impl Document {
    pub fn get_physics_group(&self, id: &str) -> Option<&PhysicsSetting> {
        self.physics()?
            .data
            .settings
            .iter()
            .find(|g| g.id == id)
            .map(AsRef::as_ref)
    }
    pub fn get_physics_input(&self, id: &str) -> Option<(&PhysicsSetting, &PhysicsInput)> {
        self.physics()?.data.settings.iter().find_map(|g| {
            g.inputs
                .iter()
                .find(|v| v.id == id)
                .map(|v| (g.as_ref(), v))
        })
    }
    pub fn get_physics_output(&self, id: &str) -> Option<(&PhysicsSetting, &PhysicsOutput)> {
        self.physics()?.data.settings.iter().find_map(|g| {
            g.outputs
                .iter()
                .find(|v| v.id == id)
                .map(|v| (g.as_ref(), v))
        })
    }
    pub fn get_physics_node(&self, id: &str) -> Option<(&PhysicsSetting, &PhysicsNode)> {
        self.physics()?.data.settings.iter().find_map(|g| {
            g.vertices
                .iter()
                .find(|v| v.id == id)
                .map(|v| (g.as_ref(), v))
        })
    }
    pub fn physics_node_users(&self, id: &str) -> Vec<String> {
        self.physics()
            .into_iter()
            .flat_map(|a| &a.data.settings)
            .flat_map(|g| &g.outputs)
            .filter(|v| v.node_id == id)
            .map(|v| v.id.clone())
            .collect()
    }
    pub fn physics_owner(&self, id: &str) -> Option<&str> {
        let a = self.physics()?;
        if a.id == id {
            return None;
        }
        if self.get_physics_group(id).is_some() {
            return Some(&a.id);
        }
        self.get_physics_input(id)
            .map(|(g, _)| g.id.as_str())
            .or_else(|| self.get_physics_output(id).map(|(g, _)| g.id.as_str()))
            .or_else(|| self.get_physics_node(id).map(|(g, _)| g.id.as_str()))
    }
    pub(in crate::document) fn validate_native_physics_rig(
        &self,
        g: &PhysicsSetting,
    ) -> Result<(), Status> {
        for output in &g.outputs {
            if g.node_index(&output.node_id).is_none() {
                return Err(Status::error("PHYSICS_NODE_UNAVAILABLE", &output.node_id));
            }
        }
        for target in g
            .inputs
            .iter()
            .map(|v| &v.source)
            .chain(g.outputs.iter().map(|v| &v.destination))
        {
            match target {
                PhysicsTarget::Resolved { parameter_id }
                    if self.get_parameter(parameter_id).is_none() =>
                {
                    return Err(Status::error("MISSING_PARAMETER", parameter_id));
                }
                PhysicsTarget::Unresolved { runtime_id }
                    if runtime_id.is_empty() || runtime_id.contains('\0') =>
                {
                    return Err(Status::error("INVALID_PHYSICS_TARGET", runtime_id));
                }
                _ => {}
            }
        }
        crate::physics::validate_physics_setting(&g.to_wire(&PhysicsTarget::validation_key))
            .map_err(|e| Status::error(e.code, format!("{}: {}", e.path, e.message)))
    }
}

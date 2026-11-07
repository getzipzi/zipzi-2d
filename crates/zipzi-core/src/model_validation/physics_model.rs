use super::*;
use crate::document::physics::model::PhysicsTargetValidation;
impl Model<'_> {
    pub(crate) fn validate_native_physics_rig(&self, g: &PhysicsSetting) -> Result<(), Status> {
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

//! Shared presentation of native capability states and typed planning failures.
use super::*;
use editor_core::{AuthoringAction, ObjectAuthoringCapabilities};
pub(crate) fn reason_message(code: &str) -> Option<String> {
    let key = match code {
        "OBJECT_LOCKED" => "authoring.locked",
        "OBJECT_REFERENCED" => "authoring.referenced",
        "AUTHORING_CANDIDATE_ACTIVE" => "authoring.candidate_active",
        "MISSING_OBJECT" => "authoring.missing_object",
        "MISSING_POSE_OWNER" => "authoring.pose_owner_required",
        "CHOOSE_PHYSICS_NODE" => "authoring.choose_output_node",
        "DUPLICATE_POSE_PART" => "authoring.pose_duplicate_part",
        "DUPLICATE_POSE_LINK" => "authoring.pose_duplicate_link",
        "MISSING_EXPRESSION" => "authoring.expression_context",
        "ANIMATION_ORDER_ONLY" => "authoring.animation_order_only",
        "NO_CHANGE" => "authoring.no_change",
        "PHYSICS_CHAIN_TOO_SHORT" => "authoring.physics_chain_too_short",
        "PHYSICS_NODE_IN_USE" => "authoring.physics_node_in_use",
        "PHYSICS_ROOT_NODE" => "authoring.physics_root_node",
        "PALETTE_DEFORMATION_SIBLINGS" => "authoring.step_deformation_siblings",
        "OFFSCREEN_REQUIRES_PART" => "authoring.move_requires_part",
        "DUPLICATE_OWNER" => "authoring.move_offscreen_owner",
        "OFFSCREEN_REBIND_REQUIRES_CONVERSION" => "authoring.move_requires_conversion",
        "CDI_GROUP_CYCLE" | "PART_CYCLE" | "RELATION_CYCLE" => "authoring.move_cycle",
        "SUBTREE_CONVERSION_UNPROVEN" => "reparent.subtree_unproven",
        "NONLINEAR_PARENT_UNPROVEN" => "reparent.nonlinear_unproven",
        "PARAMETER_DOMAIN_UNPROVEN" => "reparent.domain_unproven",
        "SINGULAR_PARENT" => "reparent.singular",
        "STALE_PLAN" | "STALE_VERSION" => "authoring.move_stale",
        "AUTHORING_CANCELLED" => "reparent.cancelled",
        "EXPECTED_DEFORMATION_MEMBERS" => "authoring.deformation_context",
        _ => return None,
    };
    Some(t!(key).into_owned())
}
pub(crate) fn error_code(error: &anyhow::Error) -> Option<&str> {
    error
        .downcast_ref::<kasane_sdk::SdkError>()
        .map(|e| e.code.as_ref())
        .or_else(|| {
            error
                .downcast_ref::<editor_core::PaletteMovePlanningError>()
                .map(|e| e.code())
        })
        .or_else(|| {
            error
                .downcast_ref::<editor_core::AuthoringPlanningError>()
                .map(|e| e.code())
        })
}
pub(crate) fn authoring_error(error: &anyhow::Error) -> String {
    error_code(error)
        .and_then(reason_message)
        .unwrap_or_else(|| error.to_string())
}
impl ActionAvailability {
    pub fn for_object(
        document: &editor_core::Document,
        object: &ObjectRef,
        action: AuthoringAction,
    ) -> Option<Self> {
        match document.authoring_capabilities(object) {
            Ok(capabilities) => Self::from_authoring(&capabilities, action),
            Err(error) => Some(Self {
                enabled: false,
                reason: Some(authoring_error(&error).into()),
            }),
        }
    }
    pub fn from_authoring(
        capabilities: &ObjectAuthoringCapabilities,
        action: AuthoringAction,
    ) -> Option<Self> {
        let mut value = Self::from_core(&capabilities.state(action))?;
        if value.enabled
            && action != AuthoringAction::Inspect
            && capabilities.session_reason.is_some()
        {
            value.enabled = false;
            value.reason = reason_message("AUTHORING_CANDIDATE_ACTIVE").map(Into::into);
        }
        Some(value)
    }
}

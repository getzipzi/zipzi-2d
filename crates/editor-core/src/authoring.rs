//! UI-independent adaptation of SDK authoring results. Selection and location
//! suggestions are transient; they never activate an expression or enter history.
use super::*;
use crate::{ObjectKind, ObjectRef};
#[path = "authoring/expression.rs"]
mod expression;
#[path = "authoring/publication.rs"]
mod publication;

/// Identifies one editor-owned gesture. Cancel/finish permanently retires it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpressionEntryEditToken(Uuid);

#[derive(Debug, Clone)]
pub struct OperationResult {
    pub receipt: kasane_sdk::EditReceipt,
    pub suggested_selection: Vec<ObjectRef>,
    pub locations: Vec<ObjectRef>,
}

#[derive(Debug, Clone)]
pub struct ExpressionAuthoringCapabilities {
    pub domain: kasane_sdk::ExpressionCapabilities,
    /// Editor-only conditions remain separate from SDK domain reasons.
    pub session_reason: Option<AuthoringSessionReason>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoringSessionReason {
    CandidateActive,
}

#[path = "authoring/binding.rs"]
mod binding;
#[path = "authoring/motion.rs"]
mod motion;
#[path = "authoring/physics.rs"]
mod physics;
#[derive(Debug, Clone)]
pub struct PhysicsAuthoringCapabilities {
    pub domain: kasane_sdk::PhysicsCapabilities,
    pub session_reason: Option<AuthoringSessionReason>,
}

#[path = "authoring/pose.rs"]
mod pose;
pub use pose::PoseAuthoringCapabilities;

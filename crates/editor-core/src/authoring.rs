//! UI-independent adaptation of SDK authoring results. Selection and location
//! suggestions are transient; they never activate an expression or enter history.
use super::*;
use crate::{ObjectKind, ObjectRef};
#[path = "authoring/creation.rs"]
mod creation;
#[path = "authoring/scene_creation.rs"]
mod scene_creation;
pub use scene_creation::{SceneObjectCreationPlan, SceneObjectType};
#[path = "authoring/moving.rs"]
mod moving;
#[path = "authoring/stepping.rs"]
mod stepping;
pub use moving::{
    PaletteMove, PaletteMoveDestination, PaletteMovePlan, PalettePosition, PaletteProjection,
};
pub use stepping::PaletteMovePlanningError;
#[path = "authoring/animation_library.rs"]
mod animation_library;
#[path = "authoring/expression.rs"]
mod expression;
pub use animation_library::AnimationLibraryPlan;
#[path = "authoring/animation_management.rs"]
mod animation_management;
pub use animation_management::{
    AnimationDeletionPlan, AnimationRemoval, MotionCopyKind, MotionCopyPlan,
};
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

#[path = "authoring/organization.rs"]
mod organization;
pub use organization::OrganizationAuthoringCapabilities;

#[path = "authoring/scene.rs"]
mod scene;
pub use scene::SceneAuthoringCapabilities;

#[path = "authoring/content.rs"]
mod content;
pub use content::SceneContentAuthoringCapabilities;

#[path = "authoring/references.rs"]
mod references;
pub use references::ReferenceAuthoringCapabilities;

#[path = "authoring/resources.rs"]
mod resources;

pub use resources::ResourceAuthoringCapabilities;

impl Document {
    fn ensure_authoring_idle(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.pending.is_none(),
            "Finish the active editor gesture first"
        );
        Ok(())
    }
}

impl crate::Editor {
    pub(crate) fn ensure_authoring_target(
        &self,
        id: crate::DocumentId,
        context: &str,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.active_id() == Some(id) && !self.is_busy() && !self.exiting(),
            "{context} context changed"
        );
        self.active_session().unwrap().ensure_authoring_idle()
    }
}

impl crate::DocumentSession {
    fn ensure_authoring_idle(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.animation.timeline.edit.is_none(),
            "Finish the active Timeline gesture first"
        );
        Ok(())
    }

    /// Reconcile playback and Timeline only after a successful document operation.
    fn execute_authoring(
        &mut self,
        operation: impl FnOnce(&mut Document) -> anyhow::Result<OperationResult>,
    ) -> anyhow::Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let result = operation(&mut self.document)?;
        self.reconcile_animation();
        Ok(result)
    }
}

#[path = "authoring/reparenting.rs"]
mod reparenting;
pub use reparenting::{PalettePreservation, PaletteReparentCapture, PaletteReparentPlan};

#[path = "authoring/capabilities.rs"]
mod capabilities;
pub(super) use capabilities::AuthoringCapabilityCache;
pub use capabilities::{
    AuthoringAction, AuthoringCollection, AuthoringCollectionCapabilities, AuthoringPlanningError,
    ObjectAuthoringCapabilities,
};

#[path = "authoring/results.rs"]
mod results;

pub use physics::PhysicsChannelAction;

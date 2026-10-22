use super::*;
use anyhow::Result;
use kasane_core::document::PhysicsEdit;
impl Document {
    pub fn plan_physics_authoring(
        &self,
        intent: PhysicsEdit,
    ) -> Result<kasane_sdk::PhysicsEditPlan> {
        self.ensure_authoring_idle()?;
        Ok(self.session.plan_physics_edit(intent)?)
    }
    pub fn physics_authoring_capabilities(
        &self,
        object: &ObjectRef,
    ) -> Result<PhysicsAuthoringCapabilities> {
        let kind = match object.kind {
            ObjectKind::Physics => kasane_sdk::ObjectKind::Physics,
            ObjectKind::PhysicsGroup => kasane_sdk::ObjectKind::PhysicsGroup,
            ObjectKind::PhysicsInput => kasane_sdk::ObjectKind::PhysicsInput,
            ObjectKind::PhysicsOutput => kasane_sdk::ObjectKind::PhysicsOutput,
            ObjectKind::PhysicsNode => kasane_sdk::ObjectKind::PhysicsNode,
            _ => anyhow::bail!("Expected a physics object"),
        };
        let handle = self.session.handle(kind, &object.id)?;
        Ok(PhysicsAuthoringCapabilities {
            domain: self.session.physics_capabilities(&handle)?,
            session_reason: self
                .pending
                .is_some()
                .then_some(AuthoringSessionReason::CandidateActive),
        })
    }
    pub fn execute_physics_authoring(
        &mut self,
        plan: &kasane_sdk::PhysicsEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_authoring_idle()?;
        let receipt = self.session.execute_physics_plan(plan)?;
        let selection = receipt
            .changes
            .created
            .iter()
            .filter_map(|e| {
                Some(ObjectRef::new(
                    match e.kind {
                        kasane_sdk::ObjectKind::Physics => ObjectKind::Physics,
                        kasane_sdk::ObjectKind::PhysicsGroup => ObjectKind::PhysicsGroup,
                        kasane_sdk::ObjectKind::PhysicsInput => ObjectKind::PhysicsInput,
                        kasane_sdk::ObjectKind::PhysicsOutput => ObjectKind::PhysicsOutput,
                        kasane_sdk::ObjectKind::PhysicsNode => ObjectKind::PhysicsNode,
                        _ => return None,
                    },
                    &e.id,
                ))
            })
            .collect::<Vec<_>>();
        if receipt.changed {
            self.pristine_blank = false;
        }
        self.consume_authoring_receipt(&receipt, false);
        Ok(OperationResult {
            receipt,
            suggested_selection: selection.clone(),
            locations: selection,
        })
    }
}
impl crate::Editor {
    pub fn plan_physics_authoring(
        &self,
        document: crate::DocumentId,
        intent: PhysicsEdit,
    ) -> Result<kasane_sdk::PhysicsEditPlan> {
        self.ensure_animation_target(document)?;
        self.document().unwrap().plan_physics_authoring(intent)
    }
    pub fn execute_physics_authoring(
        &mut self,
        document: crate::DocumentId,
        plan: &kasane_sdk::PhysicsEditPlan,
    ) -> Result<OperationResult> {
        self.ensure_animation_target(document)?;
        self.active_session_mut()
            .unwrap()
            .execute_authoring(|document| document.execute_physics_authoring(plan))
    }
}

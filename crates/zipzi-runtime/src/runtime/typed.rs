//! Typed publication routes; modeling rules remain in SDK.
use super::{ClientId, Runtime, SessionId};
use zipzi_sdk::*;
impl Runtime {
    pub fn execute_attachment_plan(
        &mut self,
        client: ClientId,
        session: SessionId,
        plan: &AttachmentEditPlan,
    ) -> crate::Result<EditReceipt> {
        self.publish_plan(client, session, "Edit attachment", plan.version(), |edit| {
            edit.apply_attachment_plan(plan)
        })
    }
    pub fn execute_binding_plan(
        &mut self,
        client: ClientId,
        session: SessionId,
        plan: &BindingEditPlan,
    ) -> crate::Result<EditReceipt> {
        self.publish_plan(client, session, "Edit binding", plan.version(), |edit| {
            edit.apply_binding_plan(plan)
        })
    }
    pub fn execute_document_plan(
        &mut self,
        client: ClientId,
        session: SessionId,
        p: &DocumentEditPlan,
    ) -> crate::Result<EditReceipt> {
        self.publish_plan(client, session, "Edit document", p.version(), |edit| {
            edit.apply_document_plan(p)
        })
    }
    pub fn execute_expression_plan(
        &mut self,
        client: ClientId,
        session: SessionId,
        plan: &ExpressionEditPlan,
    ) -> crate::Result<EditReceipt> {
        self.publish_plan(client, session, "Edit expression", plan.version(), |edit| {
            edit.apply_expression_plan(plan)
        })
    }
    pub fn execute_motion_plan(
        &mut self,
        client: ClientId,
        session: SessionId,
        plan: &MotionEditPlan,
    ) -> crate::Result<EditReceipt> {
        self.publish_plan(client, session, "Edit motion", plan.version(), |edit| {
            edit.apply_motion_plan(plan)
        })
    }
    pub fn execute_organization_plan(
        &mut self,
        client: ClientId,
        session: SessionId,
        plan: &OrganizationEditPlan,
    ) -> crate::Result<EditReceipt> {
        self.publish_plan(
            client,
            session,
            "Edit organization",
            plan.version(),
            |edit| edit.apply_organization_plan(plan),
        )
    }
    pub fn execute_physics_plan(
        &mut self,
        client: ClientId,
        session: SessionId,
        plan: &PhysicsEditPlan,
    ) -> crate::Result<EditReceipt> {
        self.publish_plan(client, session, "Edit physics", plan.version(), |edit| {
            edit.apply_physics_plan(plan)
        })
    }
    pub fn execute_pose_plan(
        &mut self,
        client: ClientId,
        session: SessionId,
        plan: &PoseEditPlan,
    ) -> crate::Result<EditReceipt> {
        self.publish_plan(client, session, "Edit pose", plan.version(), |edit| {
            edit.apply_pose_plan(plan)
        })
    }
    pub fn execute_reference_plan(
        &mut self,
        client: ClientId,
        session: SessionId,
        p: &ReferenceEditPlan,
    ) -> crate::Result<EditReceipt> {
        self.publish_plan(client, session, "Edit reference", p.version(), |edit| {
            edit.apply_reference_plan(p)
        })
    }
    pub fn execute_scene_content_plan(
        &mut self,
        client: ClientId,
        session: SessionId,
        plan: &SceneContentEditPlan,
    ) -> crate::Result<EditReceipt> {
        self.publish_plan(
            client,
            session,
            "Edit scene_content",
            plan.version(),
            |edit| edit.apply_scene_content_plan(plan),
        )
    }
    pub fn execute_scene_plan(
        &mut self,
        client: ClientId,
        session: SessionId,
        plan: &SceneEditPlan,
    ) -> crate::Result<EditReceipt> {
        self.publish_plan(client, session, "Edit scene", plan.version(), |edit| {
            edit.apply_scene_plan(plan)
        })
    }
    pub fn import_prepared_image(
        &mut self,
        client: ClientId,
        session: SessionId,
        id: String,
        name: String,
        image: &PreparedImageContent,
    ) -> crate::Result<EditReceipt> {
        self.typed_write(client, session, |sdk| {
            sdk.begin_import_prepared_image(id, name, image)
        })
    }
    pub fn replace_attachment_content(
        &mut self,
        client: ClientId,
        session: SessionId,
        token: &AttachmentLoadToken,
        bytes: Option<std::sync::Arc<Vec<u8>>>,
    ) -> crate::Result<EditReceipt> {
        self.typed_write(client, session, |sdk| {
            sdk.begin_attachment_content_edit(token, bytes)
        })
    }
    pub fn replace_prepared_image_content(
        &mut self,
        client: ClientId,
        session: SessionId,
        token: &ImageLoadToken,
        image: &PreparedImageContent,
    ) -> crate::Result<EditReceipt> {
        self.typed_write(client, session, |sdk| {
            sdk.begin_prepared_image_content_edit(token, image)
        })
    }
    pub fn remesh_rectangle_grid(
        &mut self,
        client: ClientId,
        session: SessionId,
        mesh_id: &str,
        columns: usize,
        rows: usize,
        expected: Option<Version>,
    ) -> crate::Result<EditReceipt> {
        self.typed_write(client, session, |sdk| {
            sdk.begin_remesh_rectangle_grid(mesh_id, columns, rows, expected)
        })
    }
}

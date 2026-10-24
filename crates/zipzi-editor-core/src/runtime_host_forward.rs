use super::{HostedEdit, HostedSession};
use zipzi_core::document::*;
use zipzi_core::*;
use zipzi_sdk::*;
impl HostedSession {
    pub fn asset(&self, id: &str) -> Option<ImageAsset> {
        self.read().asset(id)
    }
    pub fn attachment_capabilities(
        &self,
        handle: &ObjectHandle,
    ) -> Result<AttachmentCapabilities, SdkError> {
        self.read().attachment_capabilities(handle)
    }
    pub fn attachment_load_token(&self, id: &str) -> Result<AttachmentLoadToken, SdkError> {
        self.read().attachment_load_token(id)
    }
    pub fn attachment_path_address(&self, id: &str) -> Result<AttachmentPathAddress, SdkError> {
        self.read().attachment_path_address(id)
    }
    pub fn authoring_collection_capabilities(
        &self,
        collection: &AuthoringCollection,
    ) -> Result<CollectionCapabilities, SdkError> {
        self.read().authoring_collection_capabilities(collection)
    }
    pub fn authoring_limits(&self) -> AuthoringLimits {
        self.read().authoring_limits()
    }
    pub fn binding_capabilities(
        &self,
        handle: &zipzi_sdk::ObjectHandle,
    ) -> Result<BindingCapabilities, SdkError> {
        self.read().binding_capabilities(handle)
    }
    pub fn binding_field_address(&self, id: &str) -> Result<BindingFieldAddress, SdkError> {
        self.read().binding_field_address(id)
    }
    pub fn binding_for_mesh(&self, mesh_id: &str) -> Option<MeshBinding> {
        self.read().binding_for_mesh(mesh_id)
    }
    pub fn blend_binding(&self, id: &str) -> Option<BlendShapeBinding> {
        self.read().blend_binding(id)
    }
    pub fn blend_binding_ids(&self) -> Vec<String> {
        self.read().blend_binding_ids().to_vec()
    }
    pub fn document_field_address(
        &self,
        field: DocumentField,
    ) -> Result<DocumentFieldAddress, SdkError> {
        self.read().document_field_address(field)
    }
    pub fn document_field_capability(&self, field: DocumentField) -> CapabilityState {
        self.read().document_field_capability(field)
    }
    pub fn drawing_group_address(&self, owner: &str) -> Result<DrawingGroupAddress, SdkError> {
        self.read().drawing_group_address(owner)
    }
    pub fn evaluation_revision(&self) -> u64 {
        self.document().evaluation_revision()
    }
    pub fn expression_capabilities(
        &self,
        handle: &ObjectHandle,
    ) -> Result<ExpressionCapabilities, SdkError> {
        self.read().expression_capabilities(handle)
    }
    pub fn expression_entry_address(&self, id: &str) -> Result<ExpressionEntryAddress, SdkError> {
        self.read().expression_entry_address(id)
    }
    pub fn geometry(&self, id: &str) -> Option<GeometrySnapshot> {
        self.read().geometry(id)
    }
    pub fn glue(&self, id: &str) -> Option<Glue> {
        self.read().glue(id)
    }
    pub fn glue_ids(&self) -> Vec<String> {
        self.read().glue_ids().to_vec()
    }
    pub fn handle(&self, kind: ObjectKind, id: &str) -> Result<ObjectHandle, SdkError> {
        self.read().handle(kind, id)
    }
    pub fn history_lengths(&self) -> (usize, usize) {
        self.host
            .read()
            .session_info(self.id)
            .expect("hosted session")
            .history_lengths
    }
    pub fn image_load_token(&self, id: &str) -> Result<ImageLoadToken, SdkError> {
        self.read().image_load_token(id)
    }
    pub fn mesh(&self, id: &str) -> Option<Mesh> {
        self.document().get_mesh(id).cloned()
    }
    pub fn mesh_ids(&self) -> Vec<String> {
        self.document().mesh_order().to_vec()
    }
    pub fn modified(&self) -> bool {
        self.host
            .read()
            .session_info(self.id)
            .expect("hosted session")
            .modified
    }
    pub fn motion_capabilities(
        &self,
        handle: &ObjectHandle,
    ) -> Result<MotionCapabilities, SdkError> {
        self.read().motion_capabilities(handle)
    }
    pub fn organization_address(&self, id: &str) -> Result<OrganizationAddress, SdkError> {
        self.read().organization_address(id)
    }
    pub fn organization_capabilities(
        &self,
        h: &ObjectHandle,
    ) -> Result<OrganizationCapabilities, SdkError> {
        self.read().organization_capabilities(h)
    }
    pub fn organization_collection(
        &self,
        collection: OrganizationCollection,
    ) -> Result<Vec<ObjectHandle>, SdkError> {
        self.read().organization_collection(collection)
    }
    pub fn organization_collection_capabilities(
        &self,
        c: &OrganizationCollection,
    ) -> Result<OrganizationCollectionCapabilities, SdkError> {
        self.read().organization_collection_capabilities(c)
    }
    pub fn parameter(&self, id: &str) -> Option<Parameter> {
        self.read().parameter(id)
    }
    pub fn parameter_ids(&self) -> Vec<String> {
        self.read().parameter_ids().to_vec()
    }
    pub fn physics_capabilities(
        &self,
        handle: &ObjectHandle,
    ) -> Result<PhysicsCapabilities, SdkError> {
        self.read().physics_capabilities(handle)
    }
    pub fn physics_collection(
        &self,
        collection: PhysicsCollection,
    ) -> Result<Vec<ObjectHandle>, SdkError> {
        self.read().physics_collection(collection)
    }
    pub fn plan_attachment_edit(
        &self,
        intent: zipzi_core::document::AttachmentEdit,
    ) -> Result<AttachmentEditPlan, SdkError> {
        self.read().plan_attachment_edit(intent)
    }
    pub fn plan_attachment_path_patch(
        &self,
        a: &AttachmentPathAddress,
        path: String,
    ) -> Result<zipzi_sdk::AttachmentEditPlan, SdkError> {
        self.read().plan_attachment_path_patch(a, path)
    }
    pub fn plan_binding_authoring(
        &self,
        edit: BindingAuthoringEdit,
    ) -> Result<BindingEditPlan, SdkError> {
        self.read().plan_binding_authoring(edit)
    }
    pub fn plan_binding_authoring_with_control(
        &self,
        edit: BindingAuthoringEdit,
        control: &ConversionControl,
    ) -> Result<BindingEditPlan, SdkError> {
        self.read()
            .plan_binding_authoring_with_control(edit, control)
    }
    pub fn plan_binding_field_patch(
        &self,
        address: &BindingFieldAddress,
        patch: BindingFieldPatch,
    ) -> Result<BindingEditPlan, SdkError> {
        self.read().plan_binding_field_patch(address, patch)
    }
    pub fn plan_document_edit(
        &self,
        intent: DocumentAuthoringEdit,
    ) -> Result<DocumentEditPlan, SdkError> {
        self.read().plan_document_edit(intent)
    }
    pub fn plan_document_field_patch(
        &self,
        a: &DocumentFieldAddress,
        value: DocumentValue,
    ) -> Result<DocumentEditPlan, SdkError> {
        self.read().plan_document_field_patch(a, value)
    }
    pub fn plan_drawing_range_patch(
        &self,
        address: &DrawingGroupAddress,
        range: [i32; 2],
    ) -> Result<SceneEditPlan, SdkError> {
        self.read().plan_drawing_range_patch(address, range)
    }
    pub fn plan_expression_edit(
        &self,
        edit: ExpressionEdit,
    ) -> Result<ExpressionEditPlan, SdkError> {
        self.read().plan_expression_edit(edit)
    }
    pub fn plan_expression_entry_patch(
        &self,
        address: &ExpressionEntryAddress,
        fields: ExpressionEntryFields,
    ) -> Result<ExpressionEditPlan, SdkError> {
        self.read().plan_expression_entry_patch(address, fields)
    }
    pub fn plan_motion_edit(&self, intent: MotionEdit) -> Result<MotionEditPlan, SdkError> {
        self.read().plan_motion_edit(intent)
    }
    pub fn plan_organization_edit(
        &self,
        intent: OrganizationEdit,
    ) -> Result<OrganizationEditPlan, SdkError> {
        self.read().plan_organization_edit(intent)
    }
    pub fn plan_physics_edit(&self, intent: PhysicsEdit) -> Result<PhysicsEditPlan, SdkError> {
        self.read().plan_physics_edit(intent)
    }
    pub fn plan_pose_edit(&self, intent: PoseEdit) -> Result<PoseEditPlan, SdkError> {
        self.read().plan_pose_edit(intent)
    }
    pub fn plan_pose_field_patch(
        &self,
        address: &PoseFieldAddress,
        fields: PoseFields,
    ) -> Result<PoseEditPlan, SdkError> {
        self.read().plan_pose_field_patch(address, fields)
    }
    pub fn plan_reference_edit(
        &self,
        intent: ReferenceEdit,
    ) -> Result<ReferenceEditPlan, SdkError> {
        self.read().plan_reference_edit(intent)
    }
    pub fn plan_reference_field_patch(
        &self,
        a: &ReferenceFieldAddress,
        fields: ReferenceFields,
    ) -> Result<ReferenceEditPlan, SdkError> {
        self.read().plan_reference_field_patch(a, fields)
    }
    pub fn plan_reference_selection_patch(
        &self,
        a: &ReferenceSelectionAddress,
        targets: Vec<String>,
    ) -> Result<ReferenceEditPlan, SdkError> {
        self.read().plan_reference_selection_patch(a, targets)
    }
    pub fn plan_resource_edit(&self, intent: ResourceEdit) -> Result<ResourceEditPlan, SdkError> {
        self.read().plan_resource_edit(intent)
    }
    pub fn plan_resource_name_patch(
        &self,
        address: &ResourceNameAddress,
        name: String,
    ) -> Result<ResourceEditPlan, SdkError> {
        self.read().plan_resource_name_patch(address, name)
    }
    pub fn plan_scene_content(
        &self,
        intent: SceneContentEdit,
    ) -> Result<SceneContentEditPlan, SdkError> {
        self.read().plan_scene_content(intent)
    }
    pub fn plan_scene_content_patch(
        &self,
        address: &SceneContentAddress,
        value: SceneContentValue,
    ) -> Result<SceneContentEditPlan, SdkError> {
        self.read().plan_scene_content_patch(address, value)
    }
    pub fn plan_scene_edit(&self, intent: SceneEdit) -> Result<SceneEditPlan, SdkError> {
        self.read().plan_scene_edit(intent)
    }
    pub fn plan_scene_field_patch(
        &self,
        address: &SceneFieldAddress,
        value: SceneFieldValue,
        clear_glue_pairs: bool,
    ) -> Result<SceneEditPlan, SdkError> {
        self.read()
            .plan_scene_field_patch(address, value, clear_glue_pairs)
    }
    pub fn pose_capabilities(&self, handle: &ObjectHandle) -> Result<PoseCapabilities, SdkError> {
        self.read().pose_capabilities(handle)
    }
    pub fn pose_collection(
        &self,
        collection: PoseCollection,
    ) -> Result<Vec<ObjectHandle>, SdkError> {
        self.read().pose_collection(collection)
    }
    pub fn pose_field_address(&self, id: &str) -> Result<PoseFieldAddress, SdkError> {
        self.read().pose_field_address(id)
    }
    pub fn prepare_rgba_asset(
        &self,
        id: &str,
        name: &str,
        width: u32,
        height: u32,
        rgba: std::sync::Arc<[u8]>,
    ) -> Result<zipzi_core::ImageAsset, SdkError> {
        self.read()
            .prepare_rgba_asset(id, name, width, height, rgba)
    }
    pub fn read_snapshot(&self) -> AuthoringSnapshot {
        self.read().read_snapshot()
    }
    pub fn reference_capabilities(
        &self,
        h: &ObjectHandle,
    ) -> Result<ReferenceCapabilities, SdkError> {
        self.read().reference_capabilities(h)
    }
    pub fn reference_collection(
        &self,
        c: &ReferenceCollection,
    ) -> Result<Vec<ObjectHandle>, SdkError> {
        self.read().reference_collection(c)
    }
    pub fn reference_field_address(&self, id: &str) -> Result<ReferenceFieldAddress, SdkError> {
        self.read().reference_field_address(id)
    }
    pub fn reference_selection_address(
        &self,
        owner: &str,
        field: ReferenceSelectionField,
    ) -> Result<ReferenceSelectionAddress, SdkError> {
        self.read().reference_selection_address(owner, field)
    }
    pub fn references_to(&self, id: &str) -> Vec<String> {
        self.read().references_to(id)
    }
    pub fn reparent_support(&self, edit: &ReparentEdit) -> Result<ReparentSupport, SdkError> {
        self.read().reparent_support(edit)
    }
    pub fn resource_capabilities(
        &self,
        handle: &ObjectHandle,
    ) -> Result<ResourceCapabilities, SdkError> {
        self.read().resource_capabilities(handle)
    }
    pub fn resource_collection(&self) -> Result<Vec<ObjectHandle>, SdkError> {
        self.read().resource_collection()
    }
    pub fn resource_collection_capabilities(&self) -> ResourceCollectionCapabilities {
        self.read().resource_collection_capabilities()
    }
    pub fn resource_name_address(&self, id: &str) -> Result<ResourceNameAddress, SdkError> {
        self.read().resource_name_address(id)
    }
    pub fn scene_capabilities(&self, handle: &ObjectHandle) -> Result<SceneCapabilities, SdkError> {
        self.read().scene_capabilities(handle)
    }
    pub fn scene_collection(&self, c: &SceneCollection) -> Result<Vec<ObjectHandle>, SdkError> {
        self.read().scene_collection(c)
    }
    pub fn scene_collection_capabilities(
        &self,
        c: &SceneCollection,
    ) -> Result<SceneCollectionCapabilities, SdkError> {
        self.read().scene_collection_capabilities(c)
    }
    pub fn scene_content_address(
        &self,
        owner_id: &str,
        target: SceneContentTarget,
    ) -> Result<SceneContentAddress, SdkError> {
        self.read().scene_content_address(owner_id, target)
    }
    pub fn scene_content_capabilities(
        &self,
        owner: &ObjectHandle,
    ) -> Result<SceneContentCapabilities, SdkError> {
        self.read().scene_content_capabilities(owner)
    }
    pub fn scene_field_address(
        &self,
        id: &str,
        field: SceneField,
    ) -> Result<SceneFieldAddress, SdkError> {
        self.read().scene_field_address(id, field)
    }
    pub fn transfer_snapshot(&self) -> Result<zipzi_project::TransferSnapshot, SdkError> {
        self.read().transfer_snapshot()
    }
    pub fn version(&self) -> Version {
        self.host
            .read()
            .session_info(self.id)
            .expect("hosted session")
            .version
    }
    pub fn execute_attachment_plan(
        &mut self,
        plan: &AttachmentEditPlan,
    ) -> zipzi_runtime::Result<EditReceipt> {
        self.host
            .write()
            .execute_attachment_plan(self.client, self.id, plan)
    }
    pub fn execute_binding_plan(
        &mut self,
        plan: &BindingEditPlan,
    ) -> zipzi_runtime::Result<EditReceipt> {
        self.host
            .write()
            .execute_binding_plan(self.client, self.id, plan)
    }
    pub fn execute_document_plan(
        &mut self,
        p: &DocumentEditPlan,
    ) -> zipzi_runtime::Result<EditReceipt> {
        self.host
            .write()
            .execute_document_plan(self.client, self.id, p)
    }
    pub fn execute_expression_plan(
        &mut self,
        plan: &ExpressionEditPlan,
    ) -> zipzi_runtime::Result<EditReceipt> {
        self.host
            .write()
            .execute_expression_plan(self.client, self.id, plan)
    }
    pub fn execute_motion_plan(
        &mut self,
        plan: &MotionEditPlan,
    ) -> zipzi_runtime::Result<EditReceipt> {
        self.host
            .write()
            .execute_motion_plan(self.client, self.id, plan)
    }
    pub fn execute_organization_plan(
        &mut self,
        plan: &OrganizationEditPlan,
    ) -> zipzi_runtime::Result<EditReceipt> {
        self.host
            .write()
            .execute_organization_plan(self.client, self.id, plan)
    }
    pub fn execute_physics_plan(
        &mut self,
        plan: &PhysicsEditPlan,
    ) -> zipzi_runtime::Result<EditReceipt> {
        self.host
            .write()
            .execute_physics_plan(self.client, self.id, plan)
    }
    pub fn execute_pose_plan(&mut self, plan: &PoseEditPlan) -> zipzi_runtime::Result<EditReceipt> {
        self.host
            .write()
            .execute_pose_plan(self.client, self.id, plan)
    }
    pub fn execute_reference_plan(
        &mut self,
        p: &ReferenceEditPlan,
    ) -> zipzi_runtime::Result<EditReceipt> {
        self.host
            .write()
            .execute_reference_plan(self.client, self.id, p)
    }
    pub fn execute_scene_content_plan(
        &mut self,
        plan: &SceneContentEditPlan,
    ) -> zipzi_runtime::Result<EditReceipt> {
        self.host
            .write()
            .execute_scene_content_plan(self.client, self.id, plan)
    }
    pub fn execute_scene_plan(
        &mut self,
        plan: &SceneEditPlan,
    ) -> zipzi_runtime::Result<EditReceipt> {
        self.host
            .write()
            .execute_scene_plan(self.client, self.id, plan)
    }
    pub fn import_prepared_image(
        &mut self,
        id: String,
        name: String,
        image: &PreparedImageContent,
    ) -> zipzi_runtime::Result<EditReceipt> {
        self.host
            .write()
            .import_prepared_image(self.client, self.id, id, name, image)
    }
    pub fn replace_attachment_content(
        &mut self,
        token: &AttachmentLoadToken,
        bytes: Option<std::sync::Arc<Vec<u8>>>,
    ) -> zipzi_runtime::Result<EditReceipt> {
        self.host
            .write()
            .replace_attachment_content(self.client, self.id, token, bytes)
    }
    pub fn replace_prepared_image_content(
        &mut self,
        token: &ImageLoadToken,
        image: &PreparedImageContent,
    ) -> zipzi_runtime::Result<EditReceipt> {
        self.host
            .write()
            .replace_prepared_image_content(self.client, self.id, token, image)
    }
    pub fn remesh_rectangle_grid(
        &mut self,
        mesh_id: &str,
        columns: usize,
        rows: usize,
        expected: Option<Version>,
    ) -> zipzi_runtime::Result<EditReceipt> {
        self.host.write().remesh_rectangle_grid(
            self.client,
            self.id,
            mesh_id,
            columns,
            rows,
            expected,
        )
    }
}
impl HostedEdit {
    pub fn apply_motion_plan(&mut self, plan: &MotionEditPlan) -> zipzi_runtime::Result<()> {
        self.apply_sdk(false, |edit| edit.apply_motion_plan(plan))
    }
    pub fn apply_resource_plan(&mut self, plan: &ResourceEditPlan) -> zipzi_runtime::Result<()> {
        self.apply_sdk(false, |edit| edit.apply_resource_plan(plan))
    }
    pub fn edit_motion(&mut self, intent: MotionEdit) -> zipzi_runtime::Result<()> {
        self.apply_sdk(false, |edit| edit.edit_motion(intent))
    }
    pub fn edit_organization(&mut self, intent: OrganizationEdit) -> zipzi_runtime::Result<()> {
        self.apply_sdk(false, |edit| edit.edit_organization(intent))
    }
    pub fn patch_physics(&mut self, patch: &PhysicsPatch) -> zipzi_runtime::Result<()> {
        self.apply_sdk(false, |edit| edit.patch_physics(patch))
    }
    pub fn preview_expression_entry(
        &mut self,
        id: &str,
        fields: ExpressionEntryFields,
    ) -> zipzi_runtime::Result<()> {
        self.apply_sdk(true, |edit| edit.preview_expression_entry(id, fields))
    }
    pub fn preview_property(
        &mut self,
        patch: &zipzi_core::document::PropertyPatch,
        preview: &mut zipzi_core::preview::PreviewState,
        values: zipzi_core::PreviewValues,
    ) -> zipzi_runtime::Result<()> {
        self.apply_sdk(true, |edit| edit.preview_property(patch, preview, values))
    }
    pub fn rename_mesh(&mut self, id: &str, name: impl Into<String>) -> zipzi_runtime::Result<()> {
        self.apply_sdk(false, |edit| edit.rename_mesh(id, name))
    }
    pub fn replace_export_settings(
        &mut self,
        settings: zipzi_core::ExportSettings,
    ) -> zipzi_runtime::Result<()> {
        self.apply_sdk(false, |edit| edit.replace_export_settings(settings))
    }
    pub fn replace_parameter(&mut self, parameter: Parameter) -> zipzi_runtime::Result<()> {
        self.apply_sdk(false, |edit| edit.replace_parameter(parameter))
    }
    pub fn set_mesh_opacity(&mut self, id: &str, opacity: f32) -> zipzi_runtime::Result<()> {
        self.apply_sdk(false, |edit| edit.set_mesh_opacity(id, opacity))
    }
    pub fn set_physics(&mut self, asset: PhysicsAsset) -> zipzi_runtime::Result<()> {
        self.apply_sdk(false, |edit| edit.set_physics(asset))
    }
}

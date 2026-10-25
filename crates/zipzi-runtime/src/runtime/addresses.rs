use super::*;
use crate::{
    addressing::{AddressPatch as P, AddressSelector as S, CapturedAddress as A},
    authoring::PlanSnapshot,
};
impl Runtime {
    pub fn capture_address(
        &mut self,
        client: ClientId,
        session: SessionId,
        selector: S,
    ) -> Result<FieldAddressId> {
        self.client(client)?;
        // Captures copy expected values; admit the source's complete content as
        // a conservative upper bound before any SDK address allocates.
        let bytes = self
            .ready(session)?
            .document()
            .estimated_content_bytes()
            .saturating_add(4096);
        self.admit_reference(client, bytes)?;
        let sdk = self.ready(session)?;
        let address = match selector {
            S::Scene { id, field } => A::Scene(sdk.scene_field_address(&id, field)?),
            S::DrawingGroup { owner } => A::DrawingGroup(sdk.drawing_group_address(&owner)?),
            S::Binding { id } => A::Binding(sdk.binding_field_address(&id)?),
            S::Organization { id } => A::Organization(sdk.organization_address(&id)?),
            S::Content { id, target } => A::Content(sdk.scene_content_address(&id, target)?),
            S::Reference { id } => A::Reference(sdk.reference_field_address(&id)?),
            S::Selection { owner, field } => {
                A::Selection(sdk.reference_selection_address(&owner, field)?)
            }
            S::ResourceName { id } => A::ResourceName(sdk.resource_name_address(&id)?),
            S::AttachmentPath { id } => A::AttachmentPath(sdk.attachment_path_address(&id)?),
            S::Document { field } => A::Document(sdk.document_field_address(field)?),
        };
        let token = FieldAddressId::from_parts(self.instance, self.serial());
        self.field_addresses.insert(
            token,
            references::AddressEntry {
                client,
                session,
                address,
                bytes,
            },
        );
        Ok(token)
    }
    pub fn plan_address_patch(
        &mut self,
        client: ClientId,
        address: FieldAddressId,
        patch: P,
    ) -> Result<PlanSnapshot> {
        self.client(client)?;
        let entry = self
            .field_addresses
            .get(&address)
            .ok_or(RuntimeError::InvalidReference)?;
        if entry.client != client {
            return Err(RuntimeError::WrongOwner);
        }
        let session = entry.session;
        if self.plans.values().filter(|p| p.client == client).count()
            >= self.limits.plans_per_client
        {
            return Err(RuntimeError::Budget("plans"));
        }
        let input = serialized_bytes(&patch)?.saturating_mul(4);
        let source_bytes = self.ready(session)?.document().estimated_content_bytes();
        let upper = source_bytes
            .saturating_mul(4)
            .saturating_add(input)
            .saturating_add(
                self.ready(session)?
                    .authoring_limits()
                    .max_conversion_bytes
                    .saturating_mul(2),
            );
        let remaining = self
            .limits
            .plan_bytes
            .saturating_sub(self.plans.values().map(|p| p.bytes).sum());
        // Domain algorithms enforce their own costs. Their maximum temporary
        // bound joins the aggregate budget before a conversion is started.
        self.check_bytes(upper, 0, 0)?;
        let sdk = self.ready(session)?;
        let plan = match (&entry.address, patch) {
            (
                A::Scene(a),
                P::Scene {
                    value,
                    clear_glue_pairs,
                },
            ) => zipzi_sdk::ModelingPlan::Scene(sdk.plan_scene_field_patch(
                a,
                value,
                clear_glue_pairs,
            )?),
            (A::DrawingGroup(a), P::DrawingRange { range }) => {
                zipzi_sdk::ModelingPlan::Scene(sdk.plan_drawing_range_patch(a, range)?)
            }
            (A::Binding(a), P::Binding { patch }) => {
                zipzi_sdk::ModelingPlan::Binding(sdk.plan_binding_field_patch(a, patch)?)
            }
            (A::Organization(a), P::Organization { intent }) => {
                zipzi_sdk::ModelingPlan::Organization(sdk.plan_organization_edit_at(a, intent)?)
            }
            (A::Content(a), P::Content { value }) => {
                zipzi_sdk::ModelingPlan::Content(sdk.plan_scene_content_patch(a, value)?)
            }
            (A::Reference(a), P::Reference { fields }) => {
                zipzi_sdk::ModelingPlan::References(sdk.plan_reference_field_patch(a, fields)?)
            }
            (A::Selection(a), P::Selection { targets }) => {
                zipzi_sdk::ModelingPlan::References(sdk.plan_reference_selection_patch(a, targets)?)
            }
            (A::ResourceName(a), P::ResourceName { name }) => {
                zipzi_sdk::ModelingPlan::Resource(sdk.plan_resource_name_patch(a, name)?)
            }
            (A::AttachmentPath(a), P::AttachmentPath { path }) => {
                zipzi_sdk::ModelingPlan::Attachment(sdk.plan_attachment_path_patch(a, path)?)
            }
            (A::Document(a), P::Document { value }) => {
                zipzi_sdk::ModelingPlan::Document(sdk.plan_document_field_patch(a, value)?)
            }
            _ => return Err(RuntimeError::InvalidReference),
        };
        let report = plan.report();
        let bytes = source_bytes
            .saturating_mul(3)
            .saturating_add(input)
            .saturating_add(
                report
                    .cost
                    .map_or(0, |c| c.payload_bytes.saturating_add(c.temporary_bytes)),
            );
        if bytes > remaining {
            return Err(RuntimeError::Budget("plan bytes"));
        }
        self.check_bytes(bytes, 0, 0)?;
        let source = self.source_snapshot(session)?;
        let token = PlanId::from_parts(self.instance, self.serial());
        let identity = source.source;
        self.plans.insert(
            token,
            modeling::PlanEntry {
                client,
                source,
                plan: Arc::new(plan),
                bytes,
            },
        );
        Ok(PlanSnapshot {
            plan: token,
            source: identity,
            report,
        })
    }
}
impl Runtime {
    pub fn modeling_capabilities(
        &self,
        client: ClientId,
        reference: ObjectRefId,
        domain: crate::addressing::CapabilityDomain,
    ) -> Result<crate::addressing::Capabilities> {
        use crate::addressing::{Capabilities as C, CapabilityDomain as D};
        let entry = self.object_ref(client, reference)?;
        let sdk = self.ready(entry.session)?;
        let h = &entry.handle;
        Ok(match domain {
            D::Scene => C::Scene(sdk.scene_capabilities(h)?),
            D::Organization => C::Organization(sdk.organization_capabilities(h)?),
            D::Binding => C::Binding(sdk.binding_capabilities(h)?),
            D::Content => C::Content(sdk.scene_content_capabilities(h)?),
            D::References => C::References(sdk.reference_capabilities(h)?),
            D::Resource => C::Resource(sdk.resource_capabilities(h)?),
            D::Attachment => C::Attachment(sdk.attachment_capabilities(h)?),
        })
    }
    pub fn collection_capabilities(
        &self,
        client: ClientId,
        session: SessionId,
        query: crate::addressing::CollectionQuery,
    ) -> Result<crate::addressing::Capabilities> {
        use crate::addressing::{Capabilities as C, CollectionQuery as Q};
        self.client(client)?;
        let sdk = self.ready(session)?;
        let query = match query {
            Q::Scene { collection } => zipzi_sdk::AuthoringCollection::Scene(collection),
            Q::Organization { collection } => {
                zipzi_sdk::AuthoringCollection::Organization(collection)
            }
            Q::References { collection } => zipzi_sdk::AuthoringCollection::References(collection),
            Q::Resources => zipzi_sdk::AuthoringCollection::Resources,
            Q::Attachments => zipzi_sdk::AuthoringCollection::Attachments,
            Q::Document { field } => return Ok(C::Document(sdk.document_field_capability(field))),
        };
        Ok(C::Collection(
            sdk.authoring_collection_capabilities(&query)?,
        ))
    }
}

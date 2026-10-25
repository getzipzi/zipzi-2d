//! Finite selectors for the SDK's stable addresses. Tokens retain the actual SDK
//! address; later patches must pass SDK incarnation/context/field epoch checks.
use serde::{Deserialize, Serialize};
use zipzi_sdk::*;
#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AddressSelector {
    Scene {
        id: String,
        field: SceneField,
    },
    DrawingGroup {
        owner: String,
    },
    Binding {
        id: String,
    },
    Organization {
        id: String,
    },
    Content {
        id: String,
        target: SceneContentTarget,
    },
    Reference {
        id: String,
    },
    Selection {
        owner: String,
        field: ReferenceSelectionField,
    },
    ResourceName {
        id: String,
    },
    AttachmentPath {
        id: String,
    },
    Document {
        field: DocumentField,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AddressPatch {
    Scene {
        value: SceneFieldValue,
        clear_glue_pairs: bool,
    },
    DrawingRange {
        range: [i32; 2],
    },
    Binding {
        patch: BindingFieldPatch,
    },
    Organization {
        intent: OrganizationEdit,
    },
    Content {
        value: SceneContentValue,
    },
    Reference {
        fields: ReferenceFields,
    },
    Selection {
        targets: Vec<String>,
    },
    ResourceName {
        name: String,
    },
    AttachmentPath {
        path: String,
    },
    Document {
        value: DocumentValue,
    },
}
pub(crate) enum CapturedAddress {
    Scene(SceneFieldAddress),
    DrawingGroup(DrawingGroupAddress),
    Binding(BindingFieldAddress),
    Organization(OrganizationAddress),
    Content(SceneContentAddress),
    Reference(ReferenceFieldAddress),
    Selection(ReferenceSelectionAddress),
    ResourceName(ResourceNameAddress),
    AttachmentPath(AttachmentPathAddress),
    Document(DocumentFieldAddress),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityDomain {
    Scene,
    Organization,
    Binding,
    Content,
    References,
    Resource,
    Attachment,
}
#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CollectionQuery {
    Scene { collection: SceneCollection },
    Organization { collection: OrganizationCollection },
    References { collection: ReferenceCollection },
    Resources,
    Attachments,
    Document { field: DocumentField },
}
#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "domain", content = "values", rename_all = "snake_case")]
pub enum Capabilities {
    Scene(SceneCapabilities),
    Organization(OrganizationCapabilities),
    Binding(BindingCapabilities),
    Content(SceneContentCapabilities),
    References(ReferenceCapabilities),
    Resource(ResourceCapabilities),
    Attachment(AttachmentCapabilities),
    Collection(CollectionCapabilities),
    Document(CapabilityState),
}

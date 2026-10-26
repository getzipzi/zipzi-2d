//! Finite selectors for the SDK's stable addresses. Tokens retain the actual SDK
//! address; later patches must pass SDK incarnation/context/field epoch checks.
use serde::{Deserialize, Serialize};
use zipzi_sdk::*;
#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AddressSelector {
    Scene {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Explicit field discriminator; must match the addressed object/domain.
        field: SceneField,
    },
    DrawingGroup {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
    },
    Binding {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
    },
    Organization {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
    },
    Content {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Explicit base/ordinary/BlendShape content selector; default Base edits the
        /// undeformed base shape.
        target: SceneContentTarget,
    },
    Reference {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
    },
    Selection {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
        /// Explicit field discriminator; must match the addressed object/domain.
        field: ReferenceSelectionField,
    },
    ResourceName {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
    },
    AttachmentPath {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
    },
    Document {
        /// Explicit field discriminator; must match the addressed object/domain.
        field: DocumentField,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AddressPatch {
    Scene {
        /// Scalar value in the referenced parameter domain.
        value: SceneFieldValue,
        /// Explicitly clear existing Glue vertex pairs when changing Glue endpoints.
        clear_glue_pairs: bool,
    },
    DrawingRange {
        /// Inclusive lower and upper drawing-order bounds, in that order.
        range: [i32; 2],
    },
    Binding {
        /// Typed patch for the addressed object; apply through the Edit to mutate candidate state.
        patch: BindingFieldPatch,
    },
    Organization {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: OrganizationEdit,
    },
    Content {
        /// Scalar value in the referenced parameter domain.
        value: SceneContentValue,
    },
    Reference {
        /// Typed replacement metadata fields; their variant must match the addressed reference
        /// kind.
        fields: ReferenceFields,
    },
    Selection {
        /// Complete replacement ordered target-ID selection; an empty list explicitly clears it.
        targets: Vec<String>,
    },
    ResourceName {
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
    },
    AttachmentPath {
        /// Normalized package-relative attachment path using forward slashes; absolute paths
        /// and .. components are invalid.
        path: String,
    },
    Document {
        /// Scalar value in the referenced parameter domain.
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
    Scene {
        /// Explicit native collection selector; it does not imply loading every object payload.
        collection: SceneCollection,
    },
    Organization {
        /// Explicit native collection selector; it does not imply loading every object payload.
        collection: OrganizationCollection,
    },
    References {
        /// Explicit native collection selector; it does not imply loading every object payload.
        collection: ReferenceCollection,
    },
    Resources,
    Attachments,
    Document {
        /// Explicit field discriminator; must match the addressed object/domain.
        field: DocumentField,
    },
}
#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "domain", content = "values", rename_all = "snake_case")]
pub enum Capabilities {
    Scene(
        /// Current scene lifecycle, hierarchy and drawing capability states and reasons; do not
        /// infer support from method presence.
        SceneCapabilities,
    ),
    Organization(
        /// Current organization and parameter groups capability states and reasons; do not
        /// infer support from method presence.
        OrganizationCapabilities,
    ),
    Binding(
        /// Current parameter bindings and grid conversions capability states and reasons; do
        /// not infer support from method presence.
        BindingCapabilities,
    ),
    Content(
        /// Current geometry and topology content capability states and reasons; do not infer
        /// support from method presence.
        SceneContentCapabilities,
    ),
    References(
        /// Current metadata references and selections capability states and reasons; do not
        /// infer support from method presence.
        ReferenceCapabilities,
    ),
    Resource(
        /// Current image asset metadata and references capability states and reasons; do not
        /// infer support from method presence.
        ResourceCapabilities,
    ),
    Attachment(
        /// Current package attachments and their references capability states and reasons; do
        /// not infer support from method presence.
        AttachmentCapabilities,
    ),
    Collection(
        /// Current collection capability states and reasons; do not infer support from method
        /// presence.
        CollectionCapabilities,
    ),
    Document(
        /// Current document configuration and metadata repair capability states and reasons; do
        /// not infer support from method presence.
        CapabilityState,
    ),
}

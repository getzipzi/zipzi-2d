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
    MotionRegistration {
        /// Stable identity of the addressed native animation object.
        id: String,
    },
    ExpressionEntry {
        /// Stable identity of the addressed native animation object.
        id: String,
    },
    Physics {
        /// Stable identity of the addressed native animation object.
        id: String,
    },
    Pose {
        /// Stable identity of the addressed native animation object.
        id: String,
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
    MotionRegistration {
        /// Complete replacement registration with its existing stable identity.
        entry: crate::document::MotionRegistration,
    },
    ExpressionEntry {
        /// Replacement expression target/value/blend fields, under captured owner epochs.
        fields: crate::document::ExpressionEntryFields,
    },
    Physics {
        /// Atomic property changes targeting only the captured Physics entity.
        changes: Vec<crate::physics_document::PhysicsProperty>,
    },
    Pose {
        /// Replacement fields for the captured Pose singleton/group/entry/link.
        fields: crate::document::PoseFields,
    },
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
    Motion {
        /// Root or stable motion-group container for library registrations.
        collection: crate::document::MotionContainer,
    },
    MotionGroups,
    Clips,
    Expressions,
    ExpressionEntries {
        /// Stable expression asset ID owning the entry collection.
        expression_id: String,
    },
    Physics {
        /// Native Physics group/input/output/node collection selector.
        collection: PhysicsCollection,
    },
    Pose {
        /// Native Pose group/entry/link collection selector.
        collection: crate::document::PoseCollection,
    },
}

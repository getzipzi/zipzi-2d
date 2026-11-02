#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
/// Select a domain-specific object or field to capture as a source-bound SDK address.
pub enum AddressSelector {
    /// Select a typed scene field on a stable object.
    Scene {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Explicit field discriminator; must match the addressed object/domain.
        field: SceneField,
    },
    /// Select a drawing group by owner identity.
    DrawingGroup {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
    },
    /// Select a binding-owned address.
    Binding {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
    },
    /// Select an organization object.
    Organization {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
    },
    /// Select a mesh, Warp, Glue or Offscreen content field.
    Content {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Typed content field selector, including its shape or slot where applicable.
        target: SceneContentTarget,
    },
    /// Select a reference metadata entry.
    Reference {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
    },
    /// Select a reference selection field on an owner.
    Selection {
        /// Stable owner ID for the addressed drawing group, collection or reference selection.
        owner: String,
        /// Explicit field discriminator; must match the addressed object/domain.
        field: ReferenceSelectionField,
    },
    /// Select an image-resource name.
    ResourceName {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
    },
    /// Select a package attachment path.
    AttachmentPath {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
    },
    /// Select a document metadata field.
    Document {
        /// Explicit field discriminator; must match the addressed object/domain.
        field: DocumentField,
    },
    /// Select a motion registration.
    MotionRegistration {
        /// Stable identity of the addressed native animation object.
        id: String,
    },
    /// Select an expression entry.
    ExpressionEntry {
        /// Stable identity of the addressed native animation object.
        id: String,
    },
    /// Select a Physics entity.
    Physics {
        /// Stable identity of the addressed native animation object.
        id: String,
    },
    /// Select a Pose entity.
    Pose {
        /// Stable identity of the addressed native animation object.
        id: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
/// Select the typed replacement accepted by a matching captured SDK address.
pub enum AddressPatch {
    /// Replace a matching typed scene field.
    Scene {
        /// Replacement scene property; its variant must match the captured field.
        value: SceneFieldValue,
        /// Explicitly clear existing Glue vertex pairs when changing Glue endpoints.
        clear_glue_pairs: bool,
    },
    /// Replace a drawing group range.
    DrawingRange {
        /// Inclusive lower and upper drawing-order bounds, in that order.
        range: [i32; 2],
    },
    /// Apply a typed binding-field patch.
    Binding {
        /// Typed patch for the addressed object; apply through the Edit to mutate candidate state.
        patch: BindingFieldPatch,
    },
    /// Apply a matching organization edit.
    Organization {
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        intent: OrganizationEdit,
    },
    /// Replace a selected scene content field.
    Content {
        /// Typed replacement content; its variant must match the captured field.
        value: SceneContentValue,
    },
    /// Replace fields of a matching reference metadata entry.
    Reference {
        /// Typed replacement metadata fields; their variant must match the addressed reference
        /// kind.
        fields: ReferenceFields,
    },
    /// Replace a reference target selection.
    Selection {
        /// Complete replacement ordered target-ID selection; an empty list explicitly clears it.
        targets: Vec<String>,
    },
    /// Replace an image-resource name.
    ResourceName {
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
    },
    /// Replace a package attachment path.
    AttachmentPath {
        /// Normalized package-relative attachment path using forward slashes; absolute paths
        /// and .. components are invalid.
        path: String,
    },
    /// Replace a typed document metadata field.
    Document {
        /// Replacement document property; its variant must match the captured field.
        value: DocumentValue,
    },
    /// Replace a motion registration.
    MotionRegistration {
        /// Complete replacement registration with its existing stable identity.
        entry: crate::document::MotionRegistration,
    },
    /// Replace expression entry fields.
    ExpressionEntry {
        /// Replacement expression target/value/blend fields, under captured owner epochs.
        fields: crate::document::ExpressionEntryFields,
    },
    /// Apply Physics property changes.
    Physics {
        /// Atomic property changes targeting only the captured Physics entity.
        changes: Vec<crate::physics_document::PhysicsProperty>,
    },
    /// Replace Pose fields.
    Pose {
        /// Replacement fields for the captured Pose singleton/group/entry/link.
        fields: crate::document::PoseFields,
    },
}
/// Select a collection for a source-bound read; choosing a selector does not write or publish.
#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CollectionQuery {
    /// Read a selected scene collection.
    Scene {
        /// Explicit native collection selector; it does not imply loading every object payload.
        collection: SceneCollection,
    },
    /// Read a selected organization collection.
    Organization {
        /// Explicit native collection selector; it does not imply loading every object payload.
        collection: OrganizationCollection,
    },
    /// Read a selected reference collection.
    References {
        /// Explicit native collection selector; it does not imply loading every object payload.
        collection: ReferenceCollection,
    },
    /// Read the resource collection.
    Resources,
    /// Read the package attachment collection.
    Attachments,
    /// Read the collection selected by a document field.
    Document {
        /// Explicit field discriminator; must match the addressed object/domain.
        field: DocumentField,
    },
    /// Read registrations in a motion container.
    Motion {
        /// Root or stable motion-group container for library registrations.
        collection: crate::document::MotionContainer,
    },
    /// Read motion registration groups.
    MotionGroups,
    /// Read motion clips.
    Clips,
    /// Read expression assets.
    Expressions,
    /// Read entries owned by one expression asset.
    ExpressionEntries {
        /// Stable expression asset ID owning the entry collection.
        expression_id: String,
    },
    /// Read a selected Physics collection.
    Physics {
        /// Native Physics group/input/output/node collection selector.
        collection: PhysicsCollection,
    },
    /// Read a selected Pose collection.
    Pose {
        /// Native Pose group/entry/link collection selector.
        collection: crate::document::PoseCollection,
    },
}

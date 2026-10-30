#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "domain",
    content = "intent",
    rename_all = "snake_case",
    deny_unknown_fields
)]
#[derive(zipzi_api_derive::ApiShape)]
pub enum ModelingIntent {
    Deletion(
        /// Shared dependency-cleaning deletion request and explicit resolutions.
        DeletionIntent,
    ),
    Scene(
        /// Typed intent for scene lifecycle, hierarchy and drawing; apply/plan it against a
        /// Session/Edit source rather than mutating this value.
        crate::document::SceneEdit,
    ),
    Organization(
        /// Typed intent for organization and parameter groups; apply/plan it against a
        /// Session/Edit source rather than mutating this value.
        crate::document::OrganizationEdit,
    ),
    Binding(
        /// Typed intent for parameter bindings and grid conversions; apply/plan it against a
        /// Session/Edit source rather than mutating this value.
        BindingAuthoringEdit,
    ),
    Content(
        /// Typed intent for geometry and topology content; apply/plan it against a Session/Edit
        /// source rather than mutating this value.
        crate::document::SceneContentEdit,
    ),
    References(
        /// Typed intent for metadata references and selections; apply/plan it against a
        /// Session/Edit source rather than mutating this value.
        crate::document::ReferenceEdit,
    ),
    Resource(
        /// Typed intent for image asset metadata and references; apply/plan it against a
        /// Session/Edit source rather than mutating this value.
        crate::document::ResourceEdit,
    ),
    Document(
        /// Typed intent for document configuration and metadata repair; apply/plan it against a
        /// Session/Edit source rather than mutating this value.
        crate::document::DocumentAuthoringEdit,
    ),
    Attachment(
        /// Typed intent for package attachments and their references; apply/plan it against a
        /// Session/Edit source rather than mutating this value.
        crate::document::AttachmentEdit,
    ),
    /// Typed motion intent evaluated against the exact formal/candidate source.
    Motion(
        /// Typed native motion operation against this captured source.
        crate::document::MotionEdit,
    ),
    /// Typed expression intent evaluated against the exact formal/candidate source.
    Expression(
        /// Typed native expression operation against this captured source.
        crate::document::ExpressionEdit,
    ),
    /// Typed physics intent evaluated against the exact formal/candidate source.
    Physics(
        /// Typed native physics operation against this captured source.
        crate::physics_document::PhysicsEdit,
    ),
    /// Typed pose intent evaluated against the exact formal/candidate source.
    Pose(
        /// Typed native pose operation against this captured source.
        crate::document::PoseEdit,
    ),
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct ManualVertexTarget {
    /// Stable vertex identity in the edited mesh, not its array index.
    pub id: VertexId,
    /// Absolute target UV coordinate in the mesh texture domain.
    pub uv: Vec2,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ManualMeshOperation {
    /// The caller supplies a fresh ID; its allocator must not reuse canceled candidate IDs.
    InsertVertex {
        /// Fresh vertex identity that has not been used by a cancelled candidate.
        id: VertexId,
        /// Absolute UV position of the new vertex.
        uv: Vec2,
    },
    /// Insert and connect in a single atomic operation/history step.
    InsertConnectedVertex {
        /// Existing vertex identity to connect to the inserted vertex.
        from: VertexId,
        /// Fresh vertex identity that has not been used by a cancelled candidate.
        id: VertexId,
        /// Absolute UV position of the new vertex.
        uv: Vec2,
    },
    /// Remove an outer edge's incident face and recover the remaining outline.
    /// Internal fixed edges become automatic edges.
    DeleteEdge {
        /// First endpoint identity of the edge to remove.
        a: VertexId,
        /// Second endpoint identity of the edge to remove.
        b: VertexId,
    },
    /// Absolute UV targets; motion_from is the last accepted preview when supplied.
    MoveVertices {
        /// Absolute UV targets keyed by persistent mesh vertex identities.
        targets: Vec<ManualVertexTarget>,
        /// Last accepted preview positions for swept crossing checks; None uses source positions.
        motion_from: Option<Vec<ManualVertexTarget>>,
    },
    DeleteVertices {
        /// Persistent vertex identities to remove atomically.
        ids: Vec<VertexId>,
    },
    ConstrainEdge {
        /// First endpoint identity of the fixed edge.
        a: VertexId,
        /// Second endpoint identity of the fixed edge.
        b: VertexId,
    },
    ReleaseConstraint {
        /// First endpoint identity of the constraint to release.
        a: VertexId,
        /// Second endpoint identity of the constraint to release.
        b: VertexId,
    },
    Reconnect,
}

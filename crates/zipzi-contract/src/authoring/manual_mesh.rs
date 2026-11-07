#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Absolute UV destination for an existing Manual Mesh vertex, selected by stable ID.
pub struct ManualVertexTarget {
    /// Stable vertex identity in the edited mesh, not its array index.
    pub id: VertexId,
    /// Absolute target UV coordinate in the mesh texture domain.
    pub uv: Vec2,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
/// Manual Mesh UV triangulation intent. Applying it through a Draft requires an exact
/// candidate source and transfers dependent shapes through the SDK.
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
    /// Remove the selected vertices and rebuild affected connectivity.
    DeleteVertices {
        /// Persistent vertex identities to remove atomically.
        ids: Vec<VertexId>,
    },
    /// Add a user-fixed edge or chain between existing vertices.
    ConstrainEdge {
        /// First endpoint identity of the fixed edge.
        a: VertexId,
        /// Second endpoint identity of the fixed edge.
        b: VertexId,
    },
    /// Release a structural edge constraint, leaving its endpoints intact.
    ReleaseConstraint {
        /// First endpoint identity of the constraint to release.
        a: VertexId,
        /// Second endpoint identity of the constraint to release.
        b: VertexId,
    },
    /// Rebuild automatic triangulation while retaining the structural constraints.
    Reconnect,
}

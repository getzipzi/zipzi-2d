#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiFields)]
pub struct EventCursor {
    /// Runtime instance identity; handles and cursors from another instance cannot be substituted.
    pub runtime: RuntimeInstanceId,
    /// None is the registry stream; it has no cross-session ordering guarantee.
    pub session: Option<SessionId>,
    /// Monotonic sequence within the selected registry or Session stream; no cross-stream
    /// ordering is implied.
    pub sequence: u64,
}

crate::api_record! {
#[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(zipzi_api_derive::BinaryOutput)]
pub struct MeshSnapshot {
    /// Exact formal or candidate source captured for this result; later host changes do not
    /// rewrite it.
    pub source: SourceRef,
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Owned vertex IDs, positions, UVs and triangles; triangles contain vertex IDs, not
    /// position offsets.
    pub geometry: crate::authoring::MeshGeometry,
}
}

crate::api_record! {
#[derive(Debug, Clone, Serialize)]
pub struct PreparedImageInfo {
    /// Client-owned prepared-image handle; creating an operation value does not transfer ownership.
    pub image: ImageId,
    /// Image width in pixels; prepared image content requires a positive dimension.
    pub width: u32,
    /// Image height in pixels; prepared image content requires a positive dimension.
    pub height: u32,
}
}

crate::api_record! {
#[derive(Debug, Clone, Serialize, zipzi_api_derive::BinaryOutput)]
pub struct ImagePixels {
    /// Image width in pixels; prepared image content requires a positive dimension.
    pub width: u32,
    /// Image height in pixels; prepared image content requires a positive dimension.
    pub height: u32,
    /// Owned tightly packed RGBA8 pixels in row-major order, top row first; length is width *
    /// height * 4 bytes, with straight alpha.
    pub rgba: std::sync::Arc<[u8]>,
}
}

crate::api_record! {
#[derive(Debug, Clone, Serialize, zipzi_api_derive::BinaryOutput)]
pub struct ImageSnapshot {
    /// Exact formal or candidate source captured for this result; later host changes do not
    /// rewrite it.
    pub source: SourceRef,
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Owned decoded image pixels; reading this value does not perform another host request.
    pub pixels: ImagePixels,
}
}

crate::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct MeshQuery {
        /// Runtime Session handle; distinct from the persistent document UUID.
        pub session: SessionId,
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        pub id: String,
        /// Edit candidate handle; None selects formal document state.
        pub edit: Option<EditId>,
    }
}

crate::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct ImageQuery {
        /// Runtime Session handle; distinct from the persistent document UUID.
        pub session: SessionId,
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        pub id: String,
        /// Edit candidate handle; None selects formal document state.
        pub edit: Option<EditId>,
    }
}

crate::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct PreparedPixelsQuery {
        /// Client-owned prepared-image handle; creating an operation value does not transfer
        /// ownership.
        pub image: ImageId,
    }
}

crate::api_union! {
tag = "kind";
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlanIntent {
    MeshRename {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Required current display name; None disables this name precondition.
        expected_name: Option<String>,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
    },
    ImageRename {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Required current display name; None disables this name precondition.
        expected_name: Option<String>,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
    },
}
}

crate::api_record! {
#[derive(Debug, Clone, Serialize)]
pub struct PlanSnapshot {
    /// Client-owned plan handle, valid only for its captured source and owning client.
    pub plan: PlanId,
    /// Exact formal or candidate source captured for this result; later host changes do not
    /// rewrite it.
    pub source: SourceRef,
    /// Cached native plan report containing source, estimated costs and predicted losses.
    pub report: crate::authoring::ModelingPlanReport,
}
}

crate::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct PlanQuery {
        /// Runtime Session handle; distinct from the persistent document UUID.
        pub session: SessionId,
        /// Edit candidate handle; None selects formal document state.
        pub edit: Option<EditId>,
        /// Typed domain edit executed against the candidate; constructing this intent does not
        /// mutate or publish.
        pub intent: PlanIntent,
    }
}

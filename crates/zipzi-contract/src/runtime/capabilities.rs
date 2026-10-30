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
    Motion,
    Expression,
    Physics,
    Pose,
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
    Motion(
        /// Current native motion eligibility; host Preview support is advertised separately.
        MotionCapabilities,
    ),
    Expression(
        /// Current native expression eligibility; host Preview support is advertised separately.
        ExpressionCapabilities,
    ),
    Physics(
        /// Current native physics eligibility; host Preview support is advertised separately.
        PhysicsCapabilities,
    ),
    Pose(
        /// Current native pose eligibility; host Preview support is advertised separately.
        PoseCapabilities,
    ),
}

#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape)]
pub struct ObjectSummary {
    /// Persistent canonical UUID in the selected document.
    pub id: String,
    /// Exact, case-sensitive display name; binding names use their mesh name.
    pub name: String,
    /// Stable SDK kind tag, e.g. mesh, parameter, mesh_binding or image.
    pub object_kind: String,
    /// SDK runtime label; empty when this kind has no runtime label.
    pub runtime_id: String,
    /// Owning Part UUID when applicable; None means no Part ownership.
    pub part_id: Option<String>,
    /// Deformation or organization parent UUID when applicable.
    pub parent_id: Option<String>,
    /// Whether the scene object is enabled; true for metadata without that field.
    pub enabled: bool,
    /// Bound mesh UUID for mesh bindings; None for other collections.
    pub mesh_id: Option<String>,
    /// Decoded image width in pixels when known, without reading pixels.
    pub width: Option<u32>,
    /// Decoded image height in pixels when known, without reading pixels.
    pub height: Option<u32>,
    /// Number of mesh vertices, without cloning vertex data.
    pub vertex_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape)]
pub struct RuntimeCapabilities {
    /// Finite domain names supported by this host; absence means the domain is unavailable.
    pub domains: Vec<String>,
    /// Whether this host supports source-bound bounded collection pagination.
    pub pagination: bool,
    /// Whether selected reads accept an exact expected Source.
    pub source_checked_reads: bool,
    /// Whether this host supports explicit Session detach/close.
    pub session_close: bool,
    /// Whether this host supports the paired binary array transport.
    pub binary_arrays: bool,
    /// Whether Session window display is supported by this host.
    pub editor_windows: bool,
    /// GPU availability: false without GPU support; otherwise None before probing, true after rendering, false after adapter/device failure.
    /// This cached observation is not a guarantee for a later render; CPU sampling is independent.
    pub gpu: Option<bool>,
    /// Compiled observation backend family: Metal on macOS, WGPU elsewhere, or None without GPU support. No device is initialized by this read.
    pub gpu_backend: String,
    /// GPU_UNAVAILABLE without GPU support; otherwise the last adapter/device failure code, or None before probing or after a successful render.
    pub gpu_error: Option<String>,
}

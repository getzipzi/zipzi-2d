#[derive(Debug, Clone, Copy, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(rename_all = "snake_case")]
/// Select the native capability family to query for a referenced object.
pub enum CapabilityDomain {
    /// Scene lifecycle, hierarchy and drawing capabilities.
    Scene,
    /// Organization and parameter-group capabilities.
    Organization,
    /// Binding and grid conversion capabilities.
    Binding,
    /// Scene content and topology capabilities.
    Content,
    /// Metadata reference and selection capabilities.
    References,
    /// Image resource capabilities.
    Resource,
    /// Package attachment capabilities.
    Attachment,
    /// Native motion capabilities.
    Motion,
    /// Native expression capabilities.
    Expression,
    /// Native Physics capabilities.
    Physics,
    /// Native Pose capabilities.
    Pose,
}

#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "domain", content = "values", rename_all = "snake_case")]
/// Source-bound snapshot of capability states; availability does not reserve future admission.
pub enum Capabilities {
    /// Scene object capabilities.
    Scene(
        /// Current scene lifecycle, hierarchy and drawing capability states and reasons; do not
        /// infer support from method presence.
        SceneCapabilities,
    ),
    /// Organization capabilities.
    Organization(
        /// Current organization and parameter groups capability states and reasons; do not
        /// infer support from method presence.
        OrganizationCapabilities,
    ),
    /// Binding capabilities.
    Binding(
        /// Current parameter bindings and grid conversions capability states and reasons; do
        /// not infer support from method presence.
        BindingCapabilities,
    ),
    /// Scene content capabilities.
    Content(
        /// Current geometry and topology content capability states and reasons; do not infer
        /// support from method presence.
        SceneContentCapabilities,
    ),
    /// Reference metadata capabilities.
    References(
        /// Current metadata references and selections capability states and reasons; do not
        /// infer support from method presence.
        ReferenceCapabilities,
    ),
    /// Image resource capabilities.
    Resource(
        /// Current image asset metadata and references capability states and reasons; do not
        /// infer support from method presence.
        ResourceCapabilities,
    ),
    /// Attachment capabilities.
    Attachment(
        /// Current package attachments and their references capability states and reasons; do
        /// not infer support from method presence.
        AttachmentCapabilities,
    ),
    /// Selected collection capabilities.
    Collection(
        /// Current collection capability states and reasons; do not infer support from method
        /// presence.
        CollectionCapabilities,
    ),
    /// Document metadata capability state.
    Document(
        /// Current document configuration and metadata repair capability states and reasons; do
        /// not infer support from method presence.
        CapabilityState,
    ),
    /// Native motion capabilities.
    Motion(
        /// Current native motion eligibility; host Preview support is advertised separately.
        MotionCapabilities,
    ),
    /// Native expression capabilities.
    Expression(
        /// Current native expression eligibility; host Preview support is advertised separately.
        ExpressionCapabilities,
    ),
    /// Native Physics capabilities.
    Physics(
        /// Current native physics eligibility; host Preview support is advertised separately.
        PhysicsCapabilities,
    ),
    /// Native Pose capabilities.
    Pose(
        /// Current native pose eligibility; host Preview support is advertised separately.
        PoseCapabilities,
    ),
}

#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape)]
/// Discovery metadata for a document object, without its full payload. Use its
/// stable ID for a selected read; display names can be duplicated or changed.
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
/// Host capabilities observed at discovery; availability can change before a later operation.
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

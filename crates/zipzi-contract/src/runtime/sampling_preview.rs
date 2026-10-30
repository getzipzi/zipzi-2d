/// Captured sampling data; the Runtime retains its separate resource reservation.
#[derive(Debug, serde::Serialize, zipzi_api_derive::ApiFields)]
pub struct ModelingSampleData {
    /// Exact formal or candidate source captured for this result; later host changes do not
    /// rewrite it.
    pub source: SourceRef,
    /// Captured sampling source state, distinguishing formal content from an Edit candidate.
    pub state: &'static str,
    /// Owned CPU sampling result captured from this frozen source.
    pub samples: crate::authoring::ModelingSamples,
}

#[derive(Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PreviewCommand {
    SetParameter {
        /// Document parameter UUID; the SDK validates existence and domain.
        id: String,
        /// Finite base value in the parameter's authored domain.
        value: f32,
    },
    ScheduleMotion {
        /// Clip UUID; uses the clip's own fades.
        id: String,
        /// Absolute activation time in seconds on this Preview's clock.
        time: f32,
    },
    ScheduleRegistration {
        /// Registration UUID; uses the registration's SDK fade overrides.
        id: String,
        /// Absolute activation time in seconds on this Preview's clock.
        time: f32,
    },
    ScheduleExpression {
        /// Native expression asset UUID.
        id: String,
        /// Absolute activation time in seconds on this Preview's clock.
        time: f32,
    },
    ScheduleParameter {
        /// Document parameter UUID for an SDK input activation.
        id: String,
        /// Absolute activation time in seconds on this Preview's clock.
        time: f32,
        /// Finite parameter input in the authored parameter domain.
        value: f32,
    },
    SetPhysics {
        /// Enable the SDK Physics stage in this Preview only.
        enabled: bool,
        /// Use the SDK stabilized-start policy on canonical replay.
        stabilized_start: bool,
    },
    Reset,
}

#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape)]
pub struct PreviewState {
    /// Monotonic accepted operation ordinal, including cancelled sample requests.
    pub sequence: u64,
    /// Actual SDK clock after the last completed operation, in seconds.
    pub time: f32,
}

#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ObservedTexture {
    /// Stable asset UUID resolved from the frozen authoring source.
    pub asset_id: String,
    /// SHA256 of the exact immutable texture content uploaded by this capture.
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ObservedBounds {
    /// Stable authoring drawable UUID.
    pub id: String,
    /// Evaluated visibility; geometric bounds do not prove unoccluded painted pixels.
    pub visible: bool,
    /// Clipped image bounds [min_x,min_y,max_x,max_y], exclusive maximum; None if empty.
    pub bounds: Option<[u32; 4]>,
}
#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ObservationImage {
    /// Output image width in pixels.
    pub width: u32,
    /// Output image height in pixels.
    pub height: u32,
    /// Raw renderer RGBA8, row-major, top-left image origin. Renderer alpha convention is preserved.
    pub rgba: Vec<u8>,
    /// PNG encoding of exactly the returned RGBA8; encoded by Rust, never by Python.
    pub png: Vec<u8>,
    /// Captured authoring document UUID.
    pub document_id: String,
    /// Captured geometry evaluation revision, distinct from the outer formal/candidate Source.
    pub evaluation_revision: u64,
    /// Shared acquisition UUID for this batch's once-resolved immutable texture set.
    pub capture_id: String,
    /// Digest of captured evaluated geometry, authoring records and resolved texture content.
    pub scene_digest: String,
    /// Digest of the exact source scene and explicit rendering settings.
    pub render_digest: String,
    /// Renderer hash of evaluated input, texture content and output view.
    pub input_sha256: String,
    /// Actual GPU adapter name, obtained only when a render executes.
    pub adapter_name: String,
    /// Actual renderer backend, Metal on macOS or the selected WGPU backend elsewhere.
    pub adapter_backend: String,
    /// Effective evaluated parameters keyed by stable document UUID.
    pub parameters: PreviewValues,
    /// Actual viewport model extent [min_x,min_y,max_x,max_y], including aspect-ratio expansion.
    pub visible_roi: [f32; 4],
    /// Model-to-image scale. Image X=x*scale+offset[0], Y=-y*scale+offset[1].
    pub scale: f32,
    /// Translation in top-left image pixels, parallel to the documented scale formula.
    pub offset: [f32; 2],
    /// Source texture identities and exact content hashes shared across this batch.
    pub textures: Vec<ObservedTexture>,
    /// Drawable geometric image bounds in evaluated order; masks/occlusion are not pixel-tested here.
    pub drawable_bounds: Vec<ObservedBounds>,
    /// Mesh quality in evaluated drawable order when View.quality is true; empty otherwise.
    /// Texture coverage uses alpha >= 8, pixel centers, at most a 256x256 grid, ignoring masks,
    /// filtering and UV repeat outside [0,1]. A step above 1 denotes approximate coverage.
    pub quality: Vec<crate::authoring::MeshQuality>,
}

#[derive(
    Debug, Clone, Serialize, Deserialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire,
)]
#[serde(deny_unknown_fields)]
pub struct ObservationView {
    /// Output width in pixels, 1..4096, subject to device and Runtime budgets.
    pub width: u32,
    /// Output height in pixels, 1..4096, subject to device and Runtime budgets.
    pub height: u32,
    /// Nonempty finite [min_x,min_y,max_x,max_y] in Y-up model coordinates.
    pub roi: [f32; 4],
    /// Nonnegative padding around the ROI in model units; aspect ratio is preserved.
    #[serde(default)]
    pub padding_model: f32,
    /// Optional topology overlay in the same frozen Source and ROI mapping. It ignores
    /// masks and occlusion; None preserves the ordinary GPU image.
    #[serde(default)]
    pub topology: Option<crate::authoring::TopologyOverlay>,
    /// Include per-mesh evaluated quality and approximate source-alpha coverage.
    /// Uses default parameters of the same source as reference; checks requested frames only.
    #[serde(default)]
    pub quality: bool,
}

impl ObservationView {
    pub fn needs_quality(&self) -> bool {
        self.quality
            || self
                .topology
                .as_ref()
                .is_some_and(|o| o.strain || o.anomalies_only)
    }
}

#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct ObservationBatch {
    /// Owned explicit view shared by these ordered frames.
    pub view: ObservationView,
    /// Owned requested parameter maps in input order, including duplicates.
    pub requested: Vec<PreviewValues>,
    /// Rendered frames in requested pose order, sharing one resolved texture acquisition.
    pub frames: Vec<ObservationImage>,
}

#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct PreviewFrame {
    /// Optional GPU image from this exact animation state and once-resolved batch textures.
    pub observation: Option<ObservationImage>,
    /// Geometric query results in the corresponding request order, from this exact SDK state.
    pub spatial: Vec<crate::spatial::SpatialResult>,
    /// Runtime-wide Preview serial; pair with the outer Source.runtime for view identity.
    pub preview_id: u64,
    /// Accepted sample request ordinal; pair with the outer Source and Preview identity.
    pub sequence: u64,
    /// Zero-based target slot in the ordered sample request.
    pub index: usize,
    /// SDK Motion/Expression/Physics/Pose result and actual sample coordinates.
    pub motion: crate::animation::MotionSnapshot,
    /// Evaluated CPU geometry when requested; model_opacity remains a separate SDK channel.
    pub geometry: Option<crate::evaluation::DrawableFrame>,
}

#[derive(Debug, Clone, Serialize, zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryOutput)]
pub struct PreviewBatch {
    /// Explicit image view when GPU observation was requested; None keeps CPU-only behavior.
    pub view: Option<ObservationView>,
    /// Owned geometric query descriptors aligned with every frame.spatial slot.
    pub queries: Vec<crate::spatial::SpatialQuery>,
    /// Accepted sample request ordinal shared by these ordered frames.
    pub sequence: u64,
    /// Owned frames in requested target order, retaining duplicate and backward targets.
    pub frames: Vec<PreviewFrame>,
}

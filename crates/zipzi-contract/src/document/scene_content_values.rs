#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape)]
pub enum SceneContentTarget {
    VertexPosition {
        /// Stable u32 vertex ID within the mesh, not an offset into its position array.
        vertex_id: VertexId,
        #[serde(default)]
        /// Base, ordinary-sample or BlendShape-sample target; omitted/default selects the base
        /// shape.
        shape: ShapeTarget,
    },
    VertexUv {
        /// Stable u32 vertex ID within the mesh, not an offset into its position array.
        vertex_id: VertexId,
    },
    Triangle {
        /// Zero-based slot in the addressed triangle or Offscreen keyform array; never an
        /// object ID.
        slot: usize,
    },
    WarpPoint {
        /// Zero-based lattice point row, in the inclusive range 0..rows.
        row: u32,
        /// Zero-based lattice point column, in the inclusive range 0..columns.
        column: u32,
        #[serde(default)]
        /// Base, ordinary-sample or BlendShape-sample target; omitted/default selects the base
        /// shape.
        shape: ShapeTarget,
    },
    /// Component 0 is the anchor; 1..4 are left, right, up, down handles.
    WarpBezierControl {
        /// Zero-based Bezier node offset in the row-major controller array.
        node: usize,
        /// Bezier component: 0 is the anchor, 1..4 are left, right, up and down handles.
        component: usize,
        #[serde(default)]
        /// Base, ordinary-sample or BlendShape-sample target; omitted/default selects the base
        /// shape.
        shape: ShapeTarget,
    },
    GluePair {
        /// Stable Glue pair ID, not its offset in the pair array.
        pair_id: String,
    },
    OffscreenKeyform {
        /// Zero-based slot in the addressed triangle or Offscreen keyform array; never an
        /// object ID.
        slot: usize,
    },
    OffscreenMapping {
        /// Static Part slot or stable Part-binding sample combination, not an Offscreen array
        /// offset.
        slot: PartKeyformSlot,
    },
}
impl SceneContentTarget {
    pub fn domain(&self) -> SceneContentDomain {
        match self {
            Self::VertexPosition { .. } | Self::VertexUv { .. } | Self::Triangle { .. } => {
                SceneContentDomain::MeshTopology
            }
            Self::WarpPoint { .. } | Self::WarpBezierControl { .. } => {
                SceneContentDomain::WarpTopology
            }
            Self::GluePair { .. } => SceneContentDomain::GluePairs,
            _ => SceneContentDomain::OffscreenKeyforms,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
#[derive(zipzi_api_derive::ApiShape)]
pub enum SceneContentValue {
    Point(
        #[doc = "Replacement XY value in the selected shape/UV space; BlendShape targets use deltas."]
         Vec2,
    ),
    Triangle(
        /// Replacement triangle as three stable vertex IDs, not position-array indices.
        [VertexId; 3],
    ),
    GluePair(
        /// Complete replacement Glue pair with its identity and endpoint vertices.
        GlueVertexPair,
    ),
    OffscreenKeyform(#[doc = "Complete replacement Offscreen appearance sample."] OffscreenKeyform),
    Mapping(#[doc = "Replacement Offscreen keyform-array index; -1 selects neutral/unmapped."] i32),
}

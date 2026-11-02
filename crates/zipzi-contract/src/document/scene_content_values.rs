#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape)]
/// Select one mesh, Warp, Glue or Offscreen content field for an address.
pub enum SceneContentTarget {
    /// Select one vertex position in the chosen shape.
    VertexPosition {
        /// Stable u32 vertex ID within the mesh, not an offset into its position array.
        vertex_id: VertexId,
        #[serde(default)]
        /// Base, ordinary-sample or BlendShape-sample target; omitted/default selects the base
        /// shape.
        shape: ShapeTarget,
    },
    /// Select the shared base UV of one vertex.
    VertexUv {
        /// Stable u32 vertex ID within the mesh, not an offset into its position array.
        vertex_id: VertexId,
    },
    /// Select one triangle by its array slot.
    Triangle {
        /// Zero-based slot in the mesh triangle array, not an object ID.
        slot: usize,
    },
    /// Select one Warp lattice point in the chosen shape.
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
    /// Select one stable Glue pair.
    GluePair {
        /// Stable Glue pair ID, not its offset in the pair array.
        pair_id: String,
    },
    /// Select an Offscreen appearance form by array slot.
    OffscreenKeyform {
        /// Zero-based slot in the Offscreen keyform array, not an object ID.
        slot: usize,
    },
    /// Select the Offscreen mapping of one Part slot.
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
/// Typed replacement content matching the selected scene content field.
pub enum SceneContentValue {
    /// Supply a position, displacement or UV pair as required by the target.
    Point(
        #[doc = "Replacement XY value in the selected shape/UV space; BlendShape targets use deltas."]
         Vec2,
    ),
    /// Supply three stable vertex IDs for a triangle.
    Triangle(
        /// Replacement triangle as three stable vertex IDs, not position-array indices.
        [VertexId; 3],
    ),
    /// Supply a complete Glue pair.
    GluePair(
        /// Complete replacement Glue pair with its identity and endpoint vertices.
        GlueVertexPair,
    ),
    /// Supply a complete Offscreen appearance form.
    OffscreenKeyform(#[doc = "Complete replacement Offscreen appearance sample."] OffscreenKeyform),
    /// Supply an Offscreen keyform index or neutral sentinel.
    Mapping(#[doc = "Replacement Offscreen keyform-array index; -1 selects neutral/unmapped."] i32),
}

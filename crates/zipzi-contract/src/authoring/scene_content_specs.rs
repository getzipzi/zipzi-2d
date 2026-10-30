crate::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct ScalarSample {
        /// Stable UUID assigned to this parameter sample.
        pub id: String,
        /// Finite parameter sample coordinate in the driving parameter domain.
        pub value: f32,
    }
}

crate::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct VertexPatchSpec {
        /// Stable u32 vertex ID within the mesh, not an offset into its position array.
        pub vertex_id: u32,
        /// Optional BlendShape XY displacement for this vertex; None preserves its delta.
        pub position: Option<[f32; 2]>,
        /// UV edits require the base-shape content API; keep None for BlendShape patches.
        pub uv: Option<[f32; 2]>,
    }
}

crate::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct GluePairSpec {
        /// Dimensionless contribution of endpoint A; default Glue pair weight is 0.5.
        pub weight_a: f32,
        /// Dimensionless contribution of endpoint B; default Glue pair weight is 0.5.
        pub weight_b: f32,
        /// Optional new Glue-pair UUID; None allocates a fresh pair identity in the SDK.
        pub id: Option<String>,
        /// Vertex ID in endpoint mesh A; None explicitly leaves this endpoint unattached.
        pub vertex_a: Option<u32>,
        /// Vertex ID in endpoint mesh B; None explicitly leaves this endpoint unattached.
        pub vertex_b: Option<u32>,
    }
}

crate::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct OffscreenFormSpec {
        /// Finite nonnegative opacity factor; 0 is transparent and 1 is opaque. Imported values
        /// above 1 are retained without clamping.
        pub opacity: f32,
        /// RGB multiply factors in channel order R, G, B; (1, 1, 1) is neutral.
        pub multiply: Option<[f32; 3]>,
        /// RGB screen factors in channel order R, G, B; (0, 0, 0) is neutral.
        pub screen: Option<[f32; 3]>,
    }
}

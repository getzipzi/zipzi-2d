#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct GlueVertexPair {
    /// Stable UUID of this Glue pair; not the identity of its target.
    pub id: String,
    /// Vertex ID in endpoint mesh A; None explicitly leaves this endpoint unattached.
    pub vertex_a: Option<VertexId>,
    /// Vertex ID in endpoint mesh B; None explicitly leaves this endpoint unattached.
    pub vertex_b: Option<VertexId>,
    /// Dimensionless contribution of endpoint A; default Glue pair weight is 0.5.
    pub weight_a: f32,
    /// Dimensionless contribution of endpoint B; default Glue pair weight is 0.5.
    pub weight_b: f32,
}
impl Default for GlueVertexPair {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            vertex_a: None,
            vertex_b: None,
            weight_a: 0.5,
            weight_b: 0.5,
        }
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct Glue {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// External format/runtime identifier retained for import/export; not the document object ID.
    pub runtime_id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Stable mesh ID for the A endpoint of this Glue.
    pub mesh_a_id: crate::scene_reference::SceneReference,
    /// Stable mesh ID for the B endpoint of this Glue.
    pub mesh_b_id: crate::scene_reference::SceneReference,
    /// Ordered Glue vertex pairs with stable pair identities and endpoint vertex IDs.
    pub pairs: Vec<GlueVertexPair>,
    /// Glue influence factor; 0 disables the connection and 1 applies its full influence.
    pub intensity: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Optional parameter-driven Glue binding; None leaves the Glue intensity static.
    pub binding: Option<GlueBinding>,
}

impl Default for Glue {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            name: String::new(),
            mesh_a_id: Default::default(),
            mesh_b_id: Default::default(),
            pairs: vec![],
            intensity: 1.,
            binding: None,
        }
    }
}

/// Ordinary parameter grid for Glue intensity, owned and edited atomically with its Glue.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct GlueBinding {
    /// Ordered parameter axes; keyform coordinates follow this order and samples retain their
    /// own identities.
    pub axes: Vec<BindingAxis>,
    /// First axis varies fastest, as in MOC3 ordinary bindings.
    pub keyforms: Vec<GlueKeyform>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct GlueKeyform {
    #[serde(default)]
    /// Stable sample IDs in active binding-axis order; not numeric coordinates.
    pub samples: Vec<String>,
    /// Glue influence factor; 0 disables the connection and 1 applies its full influence.
    pub intensity: f32,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct BindingSample {
    /// Stable sample UUID within the binding axis; distinct from its numeric value.
    pub id: String,
    /// Finite sample coordinate in the driving parameter domain; axis samples are strictly
    /// increasing.
    pub value: f32,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct ConstraintSample {
    /// Stable constraint-sample UUID.
    pub id: String,
    /// Finite parameter coordinate for this constraint sample.
    pub value: f32,
    /// Dimensionless BlendShape constraint weight at this parameter sample.
    pub weight: f32,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    Default,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct BindingAxis {
    #[serde(default)]
    /// Stable axis UUID; an empty ID is allocated during native binding normalization.
    pub id: String,
    #[serde(default)]
    /// Ordered stable samples with strictly increasing finite parameter values.
    pub samples: Vec<BindingSample>,
    /// Document parameter ID driving the axis; an empty string represents a pending/unassigned
    /// axis.
    pub parameter_id: String,
    #[serde(skip)]
    pub keys: Vec<f32>,
}

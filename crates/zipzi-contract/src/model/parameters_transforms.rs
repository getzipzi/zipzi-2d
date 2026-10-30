#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct Parameter {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// External format/runtime identifier retained for import/export; not the document object ID.
    pub runtime_id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Lower endpoint of the parameter domain, in model-defined parameter units.
    pub minimum: f32,
    /// Upper endpoint of the parameter domain, in the same units as minimum.
    pub maximum: f32,
    /// Default parameter value used when no override is supplied.
    pub default_value: f32,
    #[serde(default = "default_decimal_places")]
    /// Decimal precision used for runtime parameter handling; native default is 6.
    pub decimal_places: i32,
    #[serde(default)]
    /// Normal or BlendShape parameter behavior; native default is Normal.
    pub kind: ParameterKind,
    #[serde(default)]
    /// Whether parameter values repeat across the declared range; native default is false.
    pub repeat: bool,
}

impl Default for Parameter {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            name: String::new(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 6,
            kind: ParameterKind::Normal,
            repeat: false,
        }
    }
}

macro_rules! scene_id {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        #[derive(zipzi_api_derive::BinaryWire)]
        pub struct $name(String);
        impl crate::api_shape::ApiShape for $name {
            fn api_shape() -> crate::api_shape::Shape {
                crate::api_shape::Shape::SerdeRecord {
                    name: concat!(module_path!(), "::", stringify!($name)).into(),
                    transparent: true,
                    tuple: true,
                    fields: vec![crate::api_shape::SerdeField {
                        doc: "Persistent scene object identifier.".into(),
                        rust_name: "0".into(),
                        name: "value".into(),
                        shape: crate::api_shape::Shape::String,
                        skip: false,
                        flatten: false,
                        default: None,
                    }],
                }
            }
        }
        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }
            pub fn capacity(&self) -> usize {
                self.0.capacity()
            }
            pub fn optional(id: impl Into<String>) -> Option<Self> {
                let id = id.into();
                if id.is_empty() {
                    None
                } else {
                    Some(Self(id))
                }
            }
        }
        impl From<String> for $name {
            fn from(id: String) -> Self {
                Self(id)
            }
        }
        impl From<&str> for $name {
            fn from(id: &str) -> Self {
                Self(id.into())
            }
        }
    };
}
scene_id!(TransformId);
scene_id!(PartId);

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct Transform {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// External format/runtime identifier retained for import/export; not the document object ID.
    pub runtime_id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Organizational Part owner ID; independent of the deformation parent. None selects the
    /// root/unassigned container.
    pub part_id: Option<PartId>,
    /// Parent object ID in this hierarchy; independent of storage and drawing order. None
    /// selects the root/unassigned container.
    pub parent_id: Option<TransformId>,
    /// Typed Warp or Rotation deformation definition; its variant determines lattice versus
    /// rotation-pose interpretation.
    pub data: TransformData,
    /// Whether this object participates in evaluation; editor hiding is a separate state.
    pub enabled: bool,
    /// Opacity and multiply/screen color factors for this object or shape.
    pub appearance: Appearance,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            name: String::new(),
            part_id: None,
            parent_id: None,
            data: TransformData::default(),
            enabled: true,
            appearance: Appearance::default(),
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
pub enum TransformData {
    Warp(
        /// Warp deformation definition with its baked lattice and optional authoring controller.
        WarpTransform,
    ),
    Rotation(
        /// Rotation deformation definition with its reference angle and base pose.
        RotationTransform,
    ),
}
impl Default for TransformData {
    fn default() -> Self {
        Self::Rotation(RotationTransform::default())
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
pub struct WarpTransform {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Optional authoring Bezier controller; None leaves the baked lattice without a controller.
    pub bezier: Option<crate::warp_bezier::WarpBezier>,
    /// Number of warp lattice cells vertically; a lattice has rows + 1 point rows.
    pub rows: u32,
    /// Number of warp lattice cells horizontally; a lattice has columns + 1 point columns.
    pub columns: u32,
    /// Whether to use the quad warp interpolation mode for the lattice.
    pub quad: bool,
    /// Absolute lattice points in row-major order; index = row * (columns + 1) + column.
    pub points: Vec<Vec2>,
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
pub struct RotationTransform {
    /// Reference angle in degrees used to orient the rotation control.
    pub base_angle: f32,
    /// Base rotation pose; its origin is expressed in the parent authoring space.
    pub pose: RotationPose,
}
impl Transform {
    /// Borrow a lookup key; an absent parent uses the root coordinate space.
    pub fn parent(&self) -> &str {
        self.parent_id.as_ref().map_or("", TransformId::as_str)
    }
    pub fn part(&self) -> &str {
        self.part_id.as_ref().map_or("", PartId::as_str)
    }
    pub fn kind(&self) -> TransformKind {
        match self.data {
            TransformData::Warp(_) => TransformKind::Warp,
            TransformData::Rotation(_) => TransformKind::Rotation,
        }
    }
    pub fn warp(&self) -> Option<&WarpTransform> {
        match &self.data {
            TransformData::Warp(w) => Some(w),
            _ => None,
        }
    }
    pub fn warp_mut(&mut self) -> Option<&mut WarpTransform> {
        match &mut self.data {
            TransformData::Warp(w) => Some(w),
            _ => None,
        }
    }
    pub fn rotation(&self) -> Option<&RotationTransform> {
        match &self.data {
            TransformData::Rotation(r) => Some(r),
            _ => None,
        }
    }
    pub fn rotation_mut(&mut self) -> Option<&mut RotationTransform> {
        match &mut self.data {
            TransformData::Rotation(r) => Some(r),
            _ => None,
        }
    }
}

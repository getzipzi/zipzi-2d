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
pub struct OffscreenKeyform {
    /// Finite nonnegative opacity factor; 0 is transparent and 1 is opaque. Imported values
    /// above 1 are retained without clamping.
    pub opacity: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// RGB multiply factors in channel order R, G, B; (1, 1, 1) is neutral.
    pub multiply: Option<[f32; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// RGB screen factors in channel order R, G, B; (0, 0, 0) is neutral.
    pub screen: Option<[f32; 3]>,
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
pub struct Offscreen {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// External format/runtime identifier retained for import/export; not the document object ID.
    pub runtime_id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Owning Part object ID; it must name an existing Part rather than a root sentinel.
    pub part_id: String,
    /// Controls visibility of the composited group without changing its contents.
    #[serde(default = "offscreen_enabled_default")]
    pub enabled: bool,
    /// Compositing mode used when drawing this object.
    pub blend_mode: u32,
    /// Preserved format-specific Offscreen compositing flags.
    pub flags: u8,
    /// Ordered stable mesh IDs used as clipping-mask sources, not texture IDs.
    pub masks: Vec<String>,
    /// Part-keyform-to-Offscreen-keyform mapping; entries are keyform slots, not object IDs.
    pub part_keyform_indices: Vec<i32>,
    /// Shape samples for this binding; coordinate/sample identities must match its axes or key
    /// table.
    pub keyforms: Vec<OffscreenKeyform>,
}

fn offscreen_enabled_default() -> bool {
    true
}

impl Default for Offscreen {
    fn default() -> Self {
        Self {
            id: String::new(),
            runtime_id: String::new(),
            name: String::new(),
            part_id: String::new(),
            enabled: true,
            blend_mode: 0,
            flags: 4,
            masks: Vec::new(),
            part_keyform_indices: Vec::new(),
            keyforms: Vec::new(),
        }
    }
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
pub struct MeshKeyform {
    /// Import-only numeric coordinates; canonical documents derive these from samples.
    #[serde(skip)]
    pub keys: Vec<f32>,
    #[serde(default)]
    /// Stable sample IDs, one per active binding axis in axis order; not numeric sample
    /// coordinates.
    pub samples: Vec<String>,
    /// Absolute mesh-local shape positions in target mesh vertex order; not displacement vectors.
    pub positions: Vec<Vec2>,
    /// Opacity and multiply/screen color factors for this object or shape.
    pub appearance: Appearance,
    /// Optional keyform drawing-order coordinate; None lets native evaluation use its
    /// unoverridden order.
    pub draw_order: Option<f32>,
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
pub struct MeshBinding {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Stable ID of the target mesh in this document.
    pub mesh_id: String,
    /// Ordered parameter axes; keyform coordinates follow this order and samples retain their
    /// own identities.
    pub axes: Vec<BindingAxis>,
    /// Shape samples for this binding; coordinate/sample identities must match its axes or key
    /// table.
    pub keyforms: Vec<MeshKeyform>,
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
pub enum SceneTrack {
    Warp {
        /// Stable warp transform ID driven by this ordinary binding.
        target_id: TransformId,
        /// Ordered Warp shape records whose samples refer to this binding's active axes.
        keyforms: Vec<WarpKeyform>,
    },
    Rotation {
        /// Stable rotation transform ID driven by this ordinary binding.
        target_id: TransformId,
        /// Ordered Rotation shape records whose samples refer to this binding's active axes.
        keyforms: Vec<RotationKeyform>,
    },
    Part {
        /// Stable Part ID driven by this ordinary binding.
        target_id: PartId,
        /// Ordered Part shape records whose samples refer to this binding's active axes.
        keyforms: Vec<PartKeyform>,
    },
}
impl SceneTrack {
    pub fn target_id(&self) -> &str {
        match self {
            Self::Warp { target_id, .. } | Self::Rotation { target_id, .. } => target_id.as_str(),
            Self::Part { target_id, .. } => target_id.as_str(),
        }
    }
    pub fn len(&self) -> usize {
        match self {
            Self::Warp { keyforms, .. } => keyforms.len(),
            Self::Rotation { keyforms, .. } => keyforms.len(),
            Self::Part { keyforms, .. } => keyforms.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn sample_ids(&self, index: usize) -> &[String] {
        match self {
            Self::Warp { keyforms, .. } => &keyforms[index].samples,
            Self::Rotation { keyforms, .. } => &keyforms[index].samples,
            Self::Part { keyforms, .. } => &keyforms[index].samples,
        }
    }
    pub fn part_keyforms_mut(&mut self) -> Option<&mut Vec<PartKeyform>> {
        match self {
            Self::Part { keyforms, .. } => Some(keyforms),
            _ => None,
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
pub struct SceneBinding {
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Ordered parameter axes; keyform coordinates follow this order and samples retain their
    /// own identities.
    pub axes: Vec<BindingAxis>,
    /// Typed Warp, Rotation or Part keyform track; the track target determines shape
    /// interpretation.
    pub track: SceneTrack,
}
impl SceneBinding {
    pub fn target_id(&self) -> &str {
        self.track.target_id()
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct MotionPoint {
    /// Absolute time in seconds within the owning motion clip.
    pub time: f32,
    /// Channel value or replacement payload in the domain declared by this field type.
    pub value: f32,
}

/// Persistent identity belongs to keys, never to Bezier control points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MotionKey {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Absolute time in seconds within the owning motion clip.
    pub time: f32,
    /// Channel value or replacement payload in the domain declared by this field type.
    pub value: f32,
}

impl MotionKey {
    pub fn point(&self) -> MotionPoint {
        MotionPoint {
            time: self.time,
            value: self.value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum MotionSegment {
    Linear {
        /// Segment endpoint key or source-range end time, as declared by the enclosing type.
        end: MotionKey,
    },
    Bezier {
        /// First Bezier control point in seconds/channel-value coordinates; it has no key identity.
        control1: MotionPoint,
        /// Second Bezier control point in seconds/channel-value coordinates; it has no key identity.
        control2: MotionPoint,
        /// Segment endpoint key or source-range end time, as declared by the enclosing type.
        end: MotionKey,
    },
    Stepped {
        /// Segment endpoint key or source-range end time, as declared by the enclosing type.
        end: MotionKey,
    },
    InverseStepped {
        /// Segment endpoint key or source-range end time, as declared by the enclosing type.
        end: MotionKey,
    },
}

impl MotionSegment {
    pub fn end(&self) -> &MotionKey {
        match self {
            Self::Linear { end }
            | Self::Bezier { end, .. }
            | Self::Stepped { end }
            | Self::InverseStepped { end } => end,
        }
    }

    pub fn end_mut(&mut self) -> &mut MotionKey {
        match self {
            Self::Linear { end }
            | Self::Bezier { end, .. }
            | Self::Stepped { end }
            | Self::InverseStepped { end } => end,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum MotionTrackTarget {
    Unspecified,
    Model {
        /// Imported/exported runtime label, distinct from the document object UUID.
        runtime_id: String,
    },
    Parameter {
        /// Stable document parameter UUID; evaluation uses its declared range and default.
        parameter_id: String,
    },
    PartOpacity {
        /// Stable document Part UUID for this opacity or visibility reference.
        part_id: String,
    },
    Unresolved {
        /// Imported Physics target category retained by this wire metadata record.
        category: String,
        /// Imported/exported runtime label, distinct from the document object UUID.
        runtime_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MotionTrack {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Typed target reference; unresolved imported runtime labels remain explicit.
    pub target: MotionTrackTarget,
    /// First persistent key of the track; following segment endpoints carry their own key IDs.
    pub initial: MotionKey,
    /// Ordered interpolation segments after the initial key, with stable endpoint identities.
    pub segments: std::sync::Arc<Vec<MotionSegment>>,
    /// Optional fade-in duration in seconds; None retains the format/runtime default.
    pub fade_in: Option<f32>,
    /// Optional fade-out duration in seconds; None retains the format/runtime default.
    pub fade_out: Option<f32>,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
}

impl MotionTrack {
    /// Ordered projection; there is only one persistent representation of a key.
    pub fn keys(&self) -> impl Iterator<Item = &MotionKey> {
        std::iter::once(&self.initial).chain(self.segments.iter().map(MotionSegment::end))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MotionEvent {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Absolute time in seconds within the owning motion clip.
    pub time: f32,
    /// Channel value or replacement payload in the domain declared by this field type.
    pub value: String,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MotionClip {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Human-readable display or registration name; distinct from persistent object identity.
    pub name: String,
    /// Motion playback duration in seconds; authoring keys may retain source-range provenance.
    pub duration: f32,
    /// Nominal motion frames per second or explicit Physics fixed-step frequency, as declared by the owner.
    pub fps: f32,
    /// Whether motion playback repeats after its duration.
    pub looping: bool,
    /// Whether motion Bezier export follows the restricted control-time convention.
    pub restricted_beziers: bool,
    /// Optional fade-in duration in seconds; None retains the format/runtime default.
    pub fade_in: Option<f32>,
    /// Optional fade-out duration in seconds; None retains the format/runtime default.
    pub fade_out: Option<f32>,
    /// Display order only; tracks retain their execution order. Empty uses execution order.
    pub track_order: Vec<String>,
    /// Ordered persistent motion tracks; playback state is owned separately.
    pub tracks: crate::SharedVec<MotionTrack>,
    /// Ordered owned events in the motion clip; reading them does not advance playback.
    pub events: crate::SharedVec<MotionEvent>,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
    /// Owned opaque motion metadata fields preserved for format roundtrips.
    pub meta_extensions: BTreeMap<String, Value>,
    /// Original runtime-ID namespace used to validate preservation of opaque imported fields.
    pub opaque_source_ids: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Original imported-content hash used to detect unsupported changes to opaque fields.
    pub opaque_source_content_hash: Option<String>,
}

/// model3 registration is separate from a clip: one file may appear in multiple groups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MotionRegistration {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Stable motion clip ID referenced by this registration.
    pub clip_id: String,
    /// Optional fade-in duration in seconds; None retains the format/runtime default.
    pub fade_in: Option<f32>,
    /// Optional fade-out duration in seconds; None retains the format/runtime default.
    pub fade_out: Option<f32>,
    /// Optional attachment reference for the registered motion sound.
    pub sound: Option<AttachmentRef>,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MotionGroup {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Human-readable display or registration name; distinct from persistent object identity.
    pub name: String,
    /// Ordered owned registrations or expression/Pose entries, with stable identities.
    pub entries: Vec<MotionRegistration>,
}

/// Source-authoring key storage can extend outside the scene playback range.
/// Native projects preserve this recognized provenance; runtime export splits
/// curves to the playback range and omits the source-only metadata.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MotionSourceRange {
    /// Start time in seconds of the retained source-authoring range.
    pub start: f32,
    /// Segment endpoint key or source-range end time, as declared by the enclosing type.
    pub end: f32,
}

const SOURCE_RANGE: &str = "ZipziSourceKeyRange";
impl MotionClip {
    pub fn runtime_meta_extensions(&self) -> BTreeMap<String, Value> {
        self.meta_extensions
            .iter()
            .filter(|(key, _)| key.as_str() != SOURCE_RANGE)
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }
    pub fn source_key_range(&self) -> Option<MotionSourceRange> {
        self.meta_extensions
            .get(SOURCE_RANGE)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
    pub fn has_extensions(&self) -> bool {
        !self.extensions.is_empty()
            || self.meta_extensions.keys().any(|key| key != SOURCE_RANGE)
            || self.tracks.iter().any(|track| !track.extensions.is_empty())
            || self.events.iter().any(|event| !event.extensions.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct ExpressionAsset {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Registration name in model3; independent of parameter display names.
    pub name: String,
    /// Optional format type label retained from the imported expression or Pose asset.
    pub file_type: Option<String>,
    /// Optional fade-in duration in seconds; None retains the format/runtime default.
    pub fade_in: Option<f32>,
    /// Optional fade-out duration in seconds; None retains the format/runtime default.
    pub fade_out: Option<f32>,
    /// Ordered owned registrations or expression/Pose entries, with stable identities.
    pub entries: Vec<ExpressionEntry>,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
    /// Full Parameter UUID-to-runtime-ID namespace when opaque fields exist.
    pub opaque_source_ids: Option<BTreeMap<String, String>>,
    /// Hash of imported content when opaque fields exist; known-field edits
    /// cannot silently change the meaning of those fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opaque_source_content_hash: Option<String>,
}

impl ExpressionAsset {
    /// CAN3 scene layout and initial non-parameter state are authoring data,
    /// separate from the expression's runtime parameter overrides.
    pub fn source_scene(&self) -> Option<MotionClip> {
        self.extensions
            .get("ZipziSourceScene")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
    }

    pub fn preserve_source_scene(&mut self, scene: &MotionClip) {
        self.extensions.insert(
            "ZipziSourceScene".into(),
            serde_json::to_value(scene).expect("validated source scene serializes"),
        );
    }

    pub fn runtime_extensions(&self) -> BTreeMap<String, Value> {
        self.extensions
            .iter()
            .filter(|(key, _)| key.as_str() != "ZipziSourceScene")
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }

    pub fn has_extensions(&self) -> bool {
        self.extensions.keys().any(|key| key != "ZipziSourceScene")
            || self
                .entries
                .iter()
                .any(|entry| !entry.extensions.is_empty())
    }
}

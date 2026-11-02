#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(rename_all = "snake_case")]
/// How a stored original deformer shape was captured.
pub enum DeformerOriginalSource {
    /// Captured at deformer creation.
    Created,
    /// Captured when the first ordinary keyform was created.
    FirstKey,
    /// Captured by an explicit update.
    Manual,
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
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
/// Independent canvas-space Warp or Rotation original shape.
pub enum DeformerOriginalShape {
    /// Store a baked Warp lattice and controller in canvas space.
    Warp {
        /// Baked Warp lattice and optional controller in canvas coordinates, X right/Y up.
        warp: WarpTransform,
        /// False when a nonlinear captured parent cannot preserve a cubic controller as cubics.
        controller_canvas_exact: bool,
    },
    /// Store a Rotation pose in canvas space.
    Rotation {
        /// Canvas origin, absolute direction (including reference angle), inherited scale and reflections.
        pose: RotationPose,
    },
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
/// Captured original shape and the event that produced it.
pub struct DeformerOriginalSnapshot {
    /// Creation, first ordinary keyform transition, or explicit manual update.
    pub source: DeformerOriginalSource,
    /// Independent canvas-space shape; does not reference undo history or binding samples.
    pub shape: DeformerOriginalShape,
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
/// Persistent original-shape snapshot and first-key capture policy.
pub struct DeformerOriginal {
    /// Update once when the first ordinary keyform is created. Manual update disables this.
    pub auto_update: bool,
    /// Persistent transition marker; deleting/recreating a binding does not replace the baseline again.
    pub first_key_seen: bool,
    /// None means no original has been recorded, for example an imported object.
    pub snapshot: Option<DeformerOriginalSnapshot>,
    /// Native error code if automatic capture was unavailable; None after successful capture.
    pub capture_error: Option<String>,
}
impl Default for DeformerOriginal {
    fn default() -> Self {
        Self {
            auto_update: true,
            first_key_seen: false,
            snapshot: None,
            capture_error: None,
        }
    }
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
/// Capture, restore or configure the original deformer shape.
pub enum DeformerOriginalAction {
    /// Capture the current selected shape as the new original.
    Update,
    /// Restore the recorded original with explicit resize permission.
    Restore {
        /// Explicit permission to resample every stored Warp shape when the saved conversion density differs.
        allow_resize: bool,
    },
    /// Change the future first-key capture policy.
    AutoUpdate {
        /// Changes the future first-key policy without changing geometry or immediately replacing the snapshot.
        enabled: bool,
    },
}

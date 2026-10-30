#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Status {
    pub code: String,
    pub message: String,
}

impl Status {
    pub fn ok() -> Self {
        Self {
            code: String::new(),
            message: String::new(),
        }
    }

    pub fn is_ok(&self) -> bool {
        self.code.is_empty()
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

crate::api_record! {
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[derive(zipzi_api_derive::BinaryWire)]
pub struct Vec2 {
    /// Horizontal coordinate; its space and units are determined by the containing geometry or
    /// pose.
    pub x: f32,
    /// Vertical coordinate; its space and units are determined by the containing geometry or pose.
    pub y: f32,
}
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

pub type VertexId = u32;

/// Source-format provenance and independent output settings. Neither affects evaluation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct ModelSetup {
    /// Optional imported canvas metadata; None means this model has no source-canvas provenance.
    pub source_canvas: Option<Canvas>,
    /// Export-frame settings; padding is applied when computing an exported frame.
    pub export: ExportSettings,
}
impl From<Canvas> for ModelSetup {
    fn from(canvas: Canvas) -> Self {
        Self {
            source_canvas: Some(canvas),
            export: ExportSettings {
                pixels_per_unit: canvas.pixels_per_unit,
                ..Default::default()
            },
        }
    }
}
impl ModelSetup {
    pub fn is_valid(self) -> bool {
        self.source_canvas.is_none_or(Canvas::is_valid) && self.export.is_valid()
    }
}

/// Cubism output policy. A model unit corresponds to one canvas pixel; PPU
/// converts those lengths into Cubism runtime units only at the file boundary.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct ExportSettings {
    /// Positive model-length to Cubism-runtime-unit conversion scale at the file boundary;
    /// model units themselves remain canvas-pixel lengths.
    pub pixels_per_unit: f32,
    /// Optional stable rectangle [left, bottom, right, top] in model units.
    /// Specified bounds bypass automatic padding; pixel edges round outwards.
    pub bounds: Option<[f32; 4]>,
    /// Fractional export padding relative to the frame extent; dimensionless.
    pub padding_fraction: f32,
    /// Additional export padding measured in pixels.
    pub padding_pixels: f32,
    /// Finite nonnegative minimum automatic export extent in model units; native default is 256.
    pub minimum_extent: f32,
}
impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            pixels_per_unit: 1.,
            bounds: None,
            padding_fraction: 0.1,
            padding_pixels: 50.,
            minimum_extent: 256.,
        }
    }
}
impl ExportSettings {
    pub fn is_valid(self) -> bool {
        self.pixels_per_unit.is_finite()
            && self.pixels_per_unit > 0.
            && [
                self.padding_fraction,
                self.padding_pixels,
                self.minimum_extent,
            ]
            .into_iter()
            .all(|v| v.is_finite() && v >= 0.)
            && self.bounds.is_none_or(|[l, b, r, t]| {
                [l, b, r, t].into_iter().all(f32::is_finite) && r > l && t > b
            })
    }
}

/// Model and rotation-local lengths are X-right/Y-up. Warp parameters
/// have different dimensions, even though both are represented by Vec2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoordinateSpace {
    Model,
    RotationLocal,
    WarpParameter,
}

fn default_canvas_flag() -> u8 {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
/// A finite rectangle carried by an import/export format, not the model space.
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct Canvas {
    /// Source canvas width in pixels.
    pub width: f32,
    /// Source canvas height in pixels.
    pub height: f32,
    /// Source canvas origin in the imported pixel coordinate system; this metadata does not
    /// redefine model space.
    pub origin: Vec2,
    /// Positive pixels-per-unit value retained from the source format for import/export;
    /// this metadata does not rescale model geometry or change evaluation.
    pub pixels_per_unit: f32,
    #[serde(default = "default_canvas_flag")]
    /// Preserved imported source-canvas flag; no implicit interpretation is applied by this DTO.
    pub flag: u8,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            width: 1.0,
            height: 1.0,
            origin: Vec2::default(),
            pixels_per_unit: 1.0,
            flag: 1,
        }
    }
}

impl Canvas {
    pub fn is_valid(self) -> bool {
        self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
            && self.origin.x.is_finite()
            && self.origin.y.is_finite()
            && self.pixels_per_unit.is_finite()
            && self.pixels_per_unit > 0.0
    }

    pub fn new(width: f32, height: f32, origin: Vec2, pixels_per_unit: f32) -> Self {
        Self {
            width,
            height,
            origin,
            pixels_per_unit,
            flag: 1,
        }
    }

    pub fn with_flag(
        width: f32,
        height: f32,
        origin: Vec2,
        pixels_per_unit: f32,
        flag: u8,
    ) -> Self {
        Self {
            width,
            height,
            origin,
            pixels_per_unit,
            flag,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct ImageAsset {
    /// Original imported path or external slot when no decoded content exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unresolved_source: Option<String>,
    /// Stable document object ID used by references; it is distinct from the exported runtime_id.
    pub id: String,
    /// Human-readable display name; use the stable ID for identity because names can change.
    pub name: String,
    /// Image content metadata; None explicitly represents an asset with no loaded image content.
    pub content: Option<ImageContent>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct ImageContent {
    /// Stored resource location, normally a package-relative path or generated memory-resource
    /// locator; reading metadata does not open it.
    pub source: String,
    /// Image width in pixels; prepared image content requires a positive dimension.
    pub width: u32,
    /// Image height in pixels; prepared image content requires a positive dimension.
    pub height: u32,
    /// Hexadecimal SHA-256 of the stored image content; an empty string means no recorded digest.
    pub sha256: String,
}
impl ImageContent {
    pub fn is_valid(&self) -> bool {
        !self.source.is_empty() && !self.source.contains('\0') && self.width > 0 && self.height > 0
    }
}
impl ImageAsset {
    pub fn image_content(&self) -> Result<&ImageContent, Status> {
        self.content
            .as_ref()
            .ok_or_else(|| match &self.unresolved_source {
                Some(source) => {
                    Status::error("MISSING_IMAGE_SOURCE", format!("{}: {source}", self.id))
                }
                None => Status::error("MISSING_IMAGE_CONTENT", &self.id),
            })
    }
}

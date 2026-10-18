mod inspection;
pub(crate) use inspection::{inspect, inspect_with_animation};
mod meshes;
mod morph;
mod parameters;
mod physics;
mod scene;
mod textures;

use crate::{
    xml::{Graph, Node as N},
    *,
};
use kasane_core::{types::*, Document};
use std::collections::{BTreeSet, HashMap};

const ROOT_DEFORMER: &str = "71fae776-e218-4aee-873e-78e8ac0cb48a";
const ROOT_PARAMETER_GROUP: &str = "e9fe6eff-953b-4ce2-be7c-4a7c3913686b";

fn ordered_drawing_children(doc: &Document, part: &str, items: &[String]) -> Vec<String> {
    let mut children: Vec<String> = doc
        .part_order()
        .iter()
        .filter(|id| doc.get_part(id).unwrap().parent_id == part)
        .cloned()
        .collect();
    children.extend(
        doc.mesh_order()
            .iter()
            .filter(|id| doc.get_mesh(id).unwrap().part_id == part)
            .cloned(),
    );
    fn rank(doc: &Document, id: &str, items: &[String]) -> usize {
        if let Some(index) = items.iter().position(|v| v == id) {
            return index;
        }
        if doc.get_part(id).is_some() {
            doc.part_order()
                .iter()
                .filter(|child| doc.get_part(child).unwrap().parent_id == id)
                .chain(
                    doc.mesh_order()
                        .iter()
                        .filter(|child| doc.get_mesh(child).unwrap().part_id == id),
                )
                .map(|child| rank(doc, child, items))
                .min()
                .unwrap_or(usize::MAX)
        } else {
            usize::MAX
        }
    }
    children.sort_by_key(|id| rank(doc, id, items));
    children
}
fn flatten_drawing_children(
    doc: &Document,
    part: &str,
    owners: &BTreeSet<&str>,
    items: &[String],
    out: &mut Vec<String>,
) {
    for id in ordered_drawing_children(doc, part, items) {
        if doc.get_mesh(&id).is_some() || owners.contains(id.as_str()) {
            out.push(id);
        } else {
            flatten_drawing_children(doc, &id, owners, items, out);
        }
    }
}
fn offscreen_owner<'a>(doc: &'a Document, id: &str) -> &'a str {
    &doc.get_offscreen(id).unwrap().part_id
}

fn s(v: impl ToString) -> N {
    N::scalar("s", v)
}
fn i(v: impl ToString) -> N {
    N::scalar("i", v)
}
fn f(v: impl ToString) -> N {
    N::scalar("f", v)
}
fn b(v: bool) -> N {
    N::scalar("b", v)
}
fn guid(tag: &str, id: &str) -> N {
    N::new(tag).attr("uuid", id).attr("note", "Kasane")
}
fn ident(tag: &str, id: &str) -> N {
    N::new(tag).attr("idstr", id)
}
fn color(c: [f32; 3]) -> N {
    N::new("CFloatColor")
        .attr("red", c[0])
        .attr("green", c[1])
        .attr("blue", c[2])
        .attr("alpha", 1)
}
fn affine(m: [f64; 6]) -> N {
    N::new("CAffine")
        .attr("m00", m[0])
        .attr("m01", m[1])
        .attr("m02", m[2])
        .attr("m10", m[3])
        .attr("m11", m[4])
        .attr("m12", m[5])
}
fn color_mode(mode: u32) -> &'static str {
    [
        "NORMAL",
        "ADD",
        "MULTIPLY",
        "ADD_R2_TSL",
        "ADD_R2",
        "DARKEN",
        "MULTIPLY_R2",
        "COLORBURN_TSL",
        "LINEARBURN_TSL",
        "LIGHTEN",
        "SCREEN",
        "COLORDODGE_TSL",
        "OVERLAY",
        "SOFTLIGHT",
        "HARDLIGHT",
        "LINEARLIGHT_TSL",
        "HSL_HUE",
        "HSL_COLOR",
    ][(mode & 255) as usize]
}
fn alpha_mode(mode: u32) -> &'static str {
    ["OVER", "ATOP", "OUT", "CONJOINT", "DISJOINT"][((mode >> 8) & 255) as usize]
}
fn identity() -> N {
    affine([1., 0., 0., 0., 1., 0.])
}
fn coord(parent: &str) -> N {
    N::new("CoordType").field(
        "coordName",
        s(if parent.is_empty() {
            "Canvas"
        } else {
            "DeformerLocal"
        }),
    )
}
fn points(p: &[Vec2]) -> N {
    N::array("float-array", p.iter().flat_map(|p| [p.x, p.y]))
}
fn label() -> N {
    N::new("CLabelColor")
        .attr("customizedColorInt", -1)
        .field("labelType", N::enumeration("CLabelColorType", "UNDEFINED"))
}
fn morphs() -> N {
    N::new("KeyFormMorphTargetSet")
        .field("_morphTargets", N::empty_list())
        .field(
            "blendWeightConstraintSet",
            N::new("MorphTargetBlendWeightConstraintSet").field("_constraints", N::empty_list()),
        )
}
fn appearance(n: N, a: Appearance) -> N {
    n.field("opacity", f(a.opacity))
        .field("multiplyColor", color(a.multiply))
        .field("screenColor", color(a.screen))
}

struct TextureBinding {
    texture: N,
    atlas_guid: N,
}
struct TextureSet {
    manager: N,
    bindings: HashMap<String, TextureBinding>,
}

struct Exporter<'a> {
    doc: &'a Document,
    canvas: Canvas,
    graph: Graph,
    sources: HashMap<String, N>,
    images: Vec<(String, Vec<u8>)>,
    serial: u64,
    root_part: String,
    rest: HashMap<String, Vec<Vec2>>,
}
impl<'a> Exporter<'a> {
    fn uuid(&mut self) -> String {
        // Deterministic, document-local GUID namespace. Model object GUIDs use their
        // original Kasane UUIDs; generated forms/extensions use a separate prefix.
        self.serial += 1;
        let id = format!("ca5a0000-0000-4000-8000-{:012x}", self.serial);
        if self.doc.contains_id(&id) || id == self.doc.id() {
            self.uuid()
        } else {
            id
        }
    }
    fn new_guid(&mut self, tag: &str) -> N {
        let id = self.uuid();
        guid(tag, &id)
    }
    fn embed(&mut self, png: Vec<u8>) -> String {
        let path = format!("imageFileBuf_{}.png", self.images.len());
        self.images.push((path.clone(), png));
        path
    }
    fn source(&self, id: &str) -> N {
        self.sources[id].clone()
    }
    fn object_guid(&self, id: &str) -> N {
        if id.is_empty() {
            guid("CPartGuid", &self.root_part)
        } else if self.doc.get_mesh(id).is_some() {
            guid("CDrawableGuid", id)
        } else if self.doc.get_transform(id).is_some() {
            guid("CDeformerGuid", id)
        } else if self.doc.get_glue(id).is_some() {
            guid("CAffecterGuid", id)
        } else {
            guid("CPartGuid", id)
        }
    }
    fn rotation_sign(&self) -> f32 {
        -1.
    }
    fn source_positions(&self, parent: &str, values: &[Vec2]) -> N {
        if parent.is_empty() {
            points(
                &values
                    .iter()
                    .map(|p| {
                        Vec2::new(
                            self.source_origin_x(parent, f64::from(p.x)) as f32,
                            self.source_origin_y(parent, f64::from(p.y)) as f32,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        } else {
            points(values)
        }
    }
    fn source_origin_x(&self, parent: &str, value: f64) -> f64 {
        if parent.is_empty() {
            value + f64::from(self.canvas.origin.x)
        } else {
            value
        }
    }
    fn source_origin_y(&self, parent: &str, value: f64) -> f64 {
        if parent.is_empty() {
            f64::from(self.canvas.origin.y) - value
        } else {
            value
        }
    }
    fn source_delta(&self, parent: &str, value: Vec2) -> Vec2 {
        if parent.is_empty() {
            Vec2::new(value.x, -value.y)
        } else {
            value
        }
    }
    fn parent_deformer(&self, id: &str) -> N {
        guid(
            "CDeformerGuid",
            if id.is_empty() { ROOT_DEFORMER } else { id },
        )
        .attr("note", if id.is_empty() { "ROOT" } else { "Kasane" })
    }
    fn form_base(&mut self, owner: &N) -> (N, N) {
        let g = self.new_guid("CFormGuid");
        let n = N::new("ACForm")
            .field("guid", g.clone())
            .field("isAnimatedForm", b(false))
            .field("isLocalAnimatedForm", b(false))
            .field("_source", owner.clone())
            .field("name", N::null())
            .field("notes", s(""));
        (n, g)
    }
    fn grid(&mut self, axes: &[BindingAxis], forms: &[N], keys: Option<Vec<Vec<f32>>>) -> N {
        let grid = self.graph.reserve("KeyformGridSource");
        let bindings: Vec<_> = axes
            .iter()
            .map(|axis| {
                self.graph.add(
                    N::new("KeyformBindingSource")
                        .field("_gridSource", grid.clone())
                        .field("parameterGuid", guid("CParameterGuid", &axis.parameter_id))
                        .field("keys", N::list("array_list", axis.keys.iter().map(f)))
                        .field(
                            "interpolationType",
                            N::enumeration("InterpolationType", "LINEAR"),
                        )
                        .field(
                            "extendedInterpolationType",
                            N::enumeration("ExtendedInterpolationType", "LINEAR"),
                        )
                        .field("insertPointCount", i(1))
                        .field("extendedInterpolationScale", f(1))
                        .field("description", s("")),
                )
            })
            .collect();
        let records: Vec<_> = forms
            .iter()
            .enumerate()
            .map(|(slot, form)| {
                let mut stride = 1;
                let on_parameter: Vec<_> = axes
                    .iter()
                    .enumerate()
                    .map(|(axis_index, axis)| {
                        let index = if let Some(ref keys) = keys {
                            axis.keys
                                .iter()
                                .position(|v| *v == keys[slot][axis_index])
                                .unwrap()
                        } else {
                            (slot / stride) % axis.keys.len()
                        };
                        stride *= axis.keys.len();
                        N::new("KeyOnParameter")
                            .field("binding", bindings[axis_index].clone())
                            .field("keyIndex", i(index))
                    })
                    .collect();
                N::new("KeyformOnGrid")
                    .field(
                        "accessKey",
                        N::new("KeyformGridAccessKey")
                            .field("_keyOnParameterList", N::list("array_list", on_parameter)),
                    )
                    .field("keyformGuid", form.clone())
            })
            .collect();
        self.graph.set(
            &grid,
            N::new("KeyformGridSource")
                .field("keyformsOnGrid", N::list("array_list", records))
                .field("keyformBindings", N::list("array_list", bindings)),
        );
        grid
    }
    fn controllable(
        &self,
        name: &str,
        visible: bool,
        parent: &str,
        grid: N,
        extensions: Vec<N>,
    ) -> N {
        N::new("ACParameterControllableSource")
            .field("localName", s(name))
            .field("isVisible", b(visible))
            .field("isLocked", b(false))
            .field(
                "parentGuid",
                if parent == self.root_part {
                    N::null()
                } else {
                    self.object_guid(parent)
                },
            )
            .field("keyformGridSource", grid)
            .field("keyformMorphTargetSet", morphs())
            .field("_extensions", N::list("carray_list", extensions))
            .field("internalColor_direct_argb", N::null())
            .field("labelColor", label())
    }
    fn extension_base(&mut self, owner: &N) -> N {
        let g = self.new_guid("CExtensionGuid");
        N::new("ACExtension")
            .field("guid", g)
            .field("_owner", owner.clone())
    }
    fn effect_parameter_groups(&self) -> N {
        use kasane_core::document::ModelTargetRef;
        N::list(
            "hash_map",
            self.doc
                .model3_settings()
                .groups
                .iter()
                .flatten()
                .map(|group| {
                    N::new("entry")
                        .field(
                            "key",
                            N::enumeration(
                                "EffectParameterType",
                                if group.name == "EyeBlink" {
                                    "EYE_BLINK"
                                } else {
                                    "LIPSYNC"
                                },
                            ),
                        )
                        .field(
                            "value",
                            N::list(
                                "carray_list",
                                group.parameters.iter().map(|target| match target {
                                    ModelTargetRef::Resolved { object_id } => {
                                        guid("CParameterGuid", object_id)
                                    }
                                    _ => unreachable!("inspection rejects unresolved effects"),
                                }),
                            ),
                        )
                }),
        )
    }
    fn model(
        &mut self,
        assets: &dyn AssetResolver,
        options: &Cmo3Options,
    ) -> Result<Vec<u8>, Cmo3Error> {
        let textures = self.textures(assets)?;
        let parts = self.parts();
        let transforms = self.transforms();
        let meshes = self.meshes(&textures.bindings);
        let glues = self.glues();
        let (parameters, groups, root_group) = self.parameters();
        let canvas = self.canvas;
        let physics = self.physics();
        let root = N::new("CModelSource")
            .attr("isDefaultKeyformLocked", false)
            .field("guid", guid("CModelGuid", self.doc.id()))
            .field("name", s(&options.model_name))
            .field(
                "editorEdition",
                N::new("EditorEdition").field("edition", i(11)),
            )
            .field(
                "canvas",
                N::new("CImageCanvas")
                    .field("pixelWidth", i(canvas.width.round() as i32))
                    .field("pixelHeight", i(canvas.height.round() as i32))
                    .field("background", N::new("CColor")),
            )
            .field("parameterSourceSet", parameters)
            .field("textureManager", textures.manager)
            .field("useLegacyDrawOrder__testImpl", b(false))
            .field(
                "drawableSourceSet",
                N::new("CDrawableSourceSet").field("_sources", N::list("carray_list", meshes)),
            )
            .field(
                "deformerSourceSet",
                N::new("CDeformerSourceSet").field("_sources", N::list("carray_list", transforms)),
            )
            .field(
                "affecterSourceSet",
                N::new("CAffecterSourceSet").field("_sources", N::list("carray_list", glues)),
            )
            .field(
                "partSourceSet",
                N::new("CPartSourceSet").field("_sources", N::list("carray_list", parts)),
            )
            .field("physicsSettingsSourceSet", physics)
            .field("rootPart", self.source(&self.root_part))
            .field("parameterGroupSet", groups)
            .field("rootParameterGroup", root_group)
            .field(
                "modelInfo",
                N::new("CModelInfo")
                    .field("pixelsPerUnit", f(canvas.pixels_per_unit))
                    .field(
                        "originInPixels",
                        N::new("CPoint")
                            .field("x", i(canvas.origin.x.round() as i32))
                            .field("y", i(canvas.origin.y.round() as i32)),
                    )
                    .field(
                        "_effectParameterGroups",
                        N::new("CEffectParameterGroups")
                            .field("_parameterGroups", self.effect_parameter_groups()),
                    ),
            )
            .field("modelOptions", N::empty_map());
        let mut root = root;
        for size in [64, 32, 16] {
            let mut bytes = Vec::new();
            {
                let mut encoder = png::Encoder::new(&mut bytes, size, size);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                encoder
                    .write_header()
                    .map_err(std::io::Error::other)?
                    .write_image_data(&vec![0; (size * size * 4) as usize])
                    .map_err(std::io::Error::other)?;
            }
            let path = self.embed(bytes);
            root = root.field(
                &format!("_icon{size}"),
                N::new("CImageIcon").field(
                    "image",
                    N::new("CWritableImage")
                        .attr("width", size)
                        .attr("height", size)
                        .attr("type", "INT_ARGB")
                        .field("image", N::new("file").attr("path", path)),
                ),
            );
        }
        root = root
            .field(
                "gameMotionSet",
                N::new("CGameMotionSet")
                    .field("gameMotions", N::empty_list())
                    .field("gameMotionGroups", N::empty_list()),
            )
            .field(
                "modelViewerSetting",
                N::new("ModelViewerSetting")
                    .field("trackCursorSettings", N::list("array_list", [])),
            )
            .field(
                "guides",
                N::new("CGuidesSetting").field("guidesModeling", N::empty_list()),
            )
            .field("targetVersionNo", i(5_030_000))
            .field("latestVersionOfLastModelerNo", i(5_030_000))
            .field(
                "artPathBrushesSetting",
                N::new("CArtPathBrushSetting").field("brushes", N::empty_list()),
            )
            .field(
                "randomPoseSetting",
                N::new("CRandomPoseSettingManager").field("_settings", N::list("array_list", [])),
            )
            .field(
                "motionSyncSettingsSet",
                N::new("CMotionSyncSettingSourceSet")
                    .field("_settingSourceSetMotionSync", N::list("linked_set", [])),
            );
        self.apply_morphs();
        self.graph.document(root)
    }
}

/// Least-squares atlas-pixel to canvas placement for the mesh-edit view. Mesh
/// forms retain their exact geometry, so a non-affine UV map stays representable.
fn fit_texture(uvs: &[Vec2], positions: &[Vec2], w: u32, h: u32) -> [f64; 6] {
    let mut matrix = [[0.; 5]; 3];
    for (uv, p) in uvs.iter().zip(positions) {
        let row = [uv.x as f64 * w as f64, uv.y as f64 * h as f64, 1.];
        for a in 0..3 {
            for b in 0..3 {
                matrix[a][b] += row[a] * row[b];
            }
            matrix[a][3] += row[a] * p.x as f64;
            matrix[a][4] += row[a] * p.y as f64;
        }
    }
    for col in 0..3 {
        let pivot = (col..3)
            .max_by(|&a, &b| matrix[a][col].abs().total_cmp(&matrix[b][col].abs()))
            .unwrap();
        if matrix[pivot][col].abs() < 1e-12 {
            return [1., 0., 0., 0., 1., 0.];
        }
        matrix.swap(col, pivot);
        let d = matrix[col][col];
        for value in &mut matrix[col][col..] {
            *value /= d;
        }
        let pivot_row = matrix[col];
        for (index, row) in matrix.iter_mut().enumerate() {
            if index != col {
                let scale = row[col];
                for (value, pivot) in row[col..].iter_mut().zip(&pivot_row[col..]) {
                    *value -= scale * pivot;
                }
            }
        }
    }
    [
        matrix[0][3],
        matrix[1][3],
        matrix[2][3],
        matrix[0][4],
        matrix[1][4],
        matrix[2][4],
    ]
}

pub(crate) fn encode(
    doc: &Document,
    assets: &dyn AssetResolver,
    options: &Cmo3Options,
) -> Result<Cmo3Artifact, Cmo3Error> {
    encode_with_animation(doc, assets, options, false)
}

pub(crate) fn encode_with_animation(
    doc: &Document,
    assets: &dyn AssetResolver,
    options: &Cmo3Options,
    animation: bool,
) -> Result<Cmo3Artifact, Cmo3Error> {
    let mut report = inspect_with_animation(doc, animation)?;
    if options.loss_policy == LossPolicy::Reject && report.has_loss() {
        return Err(Cmo3Error::Unsupported(report));
    }
    let mut frame = kasane_core::DrawableFrame::default();
    // Editable source geometry precedes additive morphs and Glue. Keyform
    // pools still retain every authored pose; only this independent rest copy
    // omits the post-processing that Cubism applies again during evaluation.
    let rest_doc = if doc.blend_binding_order().is_empty() && doc.glue_order().is_empty() {
        std::borrow::Cow::Borrowed(doc)
    } else {
        let mut candidate = doc.fork_candidate();
        for id in doc.blend_binding_order().iter().chain(doc.glue_order()) {
            let result = candidate.erase_object(id);
            if !result.status.is_ok() {
                return Err(Cmo3Error::InvalidDocument(result.status.message));
            }
        }
        std::borrow::Cow::Owned(candidate)
    };
    let status = kasane_core::evaluation::evaluate_frame_including_hidden(
        &rest_doc,
        &Default::default(),
        &mut frame,
    );
    if !status.is_ok() {
        return Err(Cmo3Error::InvalidDocument(status.message));
    }
    let canvas = crate::canvas::for_document(doc)?;
    let rest = frame
        .drawables
        .into_iter()
        .map(|d| {
            (
                d.id,
                d.positions
                    .into_iter()
                    .map(|p| Vec2::new(p.x + canvas.origin.x, canvas.origin.y - p.y))
                    .collect(),
            )
        })
        .collect();
    let mut exporter = Exporter {
        doc,
        canvas,
        graph: Graph::default(),
        sources: HashMap::new(),
        images: Vec::new(),
        serial: 0,
        root_part: String::new(),
        rest,
    };
    let root_part = exporter.uuid();
    exporter.root_part = root_part.clone();
    let root = exporter.graph.reserve("CPartSource");
    exporter.sources.insert(root_part, root);
    for (ids, tag) in [
        (doc.part_order(), "CPartSource"),
        (doc.mesh_order(), "CArtMeshSource"),
        (doc.glue_order(), "CGlueSource"),
    ] {
        for id in ids {
            let r = exporter.graph.reserve(tag);
            exporter.sources.insert(id.clone(), r);
        }
    }
    for id in doc.transform_order() {
        let tag = if doc.get_transform(id).unwrap().warp().is_some() {
            "CWarpDeformerSource"
        } else {
            "CRotationDeformerSource"
        };
        let r = exporter.graph.reserve(tag);
        exporter.sources.insert(id.clone(), r);
    }
    let xml = exporter.model(assets, options)?;
    if !doc.editor_state().is_empty()
        || !doc.hierarchy_order().is_empty()
        || doc.transform_order().iter().any(|id| {
            doc.get_transform(id)
                .unwrap()
                .warp()
                .is_some_and(|w| w.bezier.is_some())
        })
    {
        report.diagnostics.push(Cmo3Diagnostic {severity:DiagnosticSeverity::Info,code:"AUTHORING_METADATA".into(),object_id:doc.id().into(),message:"Kasane palette preferences and Bezier handles are not stored; mesh and sampled warp geometry remain editable".into()});
    }
    let bytes = crate::caff::encode(exporter.images, &xml, options.obfuscation_key)?;
    report.diagnostics.push(Cmo3Diagnostic { severity:DiagnosticSeverity::Info,code:"ATLAS_ARTWORK".into(),object_id:doc.id().into(),message:"Artwork is embedded as texture atlases; original PSD layer history is unavailable in a Kasane document".into() });
    Ok(Cmo3Artifact { bytes, report })
}

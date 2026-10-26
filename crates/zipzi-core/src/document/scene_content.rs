//! Dedicated geometry and scene-content edits. Array positions are topology
//! coordinates, never object identities. Every plan binds the native snapshot.
use super::organization::Delta;
use super::*;
use crate::{GlueVertexPair, OffscreenKeyform, Vec2, VertexId, WarpBezier};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
mod address;
mod effects;
mod mesh;
mod warp;
pub use address::*;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ShapeTarget {
    #[default]
    Base,
    Ordinary {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Stable sample IDs selecting one ordinary-binding combination, in active axis order.
        samples: Vec<String>,
    },
    Blend {
        /// Stable binding ID owning the addressed axes or keyforms.
        binding_id: String,
        /// Stable sample ID from the associated axis or BlendShape key table; not a numeric
        /// parameter value.
        sample_id: String,
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
pub struct VertexContentPatch {
    /// Stable u32 vertex ID within the mesh, not an offset into its position array.
    pub vertex_id: VertexId,
    /// Replacement XY value: absolute for Base/Ordinary, displacement for BlendShape.
    /// None preserves the current value in the selected shape.
    pub position: Option<Vec2>,
    /// Optional texture UV pair; None preserves the current UV.
    pub uv: Option<Vec2>,
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
pub struct VertexWeight {
    /// Stable u32 vertex ID within the mesh, not an offset into its position array.
    pub vertex_id: VertexId,
    /// Dimensionless interpolation weight applied to the referenced source vertex.
    pub weight: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum VertexSeed {
    Base,
    Interpolate {
        /// Source vertex IDs and interpolation weights used to seed dependent shape content.
        weights: Vec<VertexWeight>,
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
pub struct NewMeshVertex {
    /// Stable u32 vertex ID within the mesh, not an offset into its position array.
    pub vertex_id: VertexId,
    /// Absolute XY authoring position for this vertex, not a displacement.
    pub position: Vec2,
    /// Texture UV pair for this vertex; not a model-space position.
    pub uv: Vec2,
    /// Explicit policy for initializing this new vertex in dependent ordinary/BlendShape keyforms.
    pub seed: VertexSeed,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum VertexDeletePolicy {
    #[default]
    RejectReferences,
    RemoveDependentContent,
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
pub struct WarpPointPatch {
    /// Zero-based lattice point row, in the inclusive range 0..rows.
    pub row: u32,
    /// Zero-based lattice point column, in the inclusive range 0..columns.
    pub column: u32,
    /// Replacement lattice XY value: absolute for Base/Ordinary, displacement for BlendShape.
    pub point: Vec2,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum OffscreenDeletePolicy {
    RejectReferences,
    Neutral,
    Remap {
        #[serde(deserialize_with = "crate::serde_maps::usize_map")]
        /// Deleted-keyform-slot to replacement-slot map; None selects neutral/unmapped for that
        /// slot.
        replacements: BTreeMap<usize, Option<usize>>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum PartKeyformSlot {
    Static,
    Samples {
        /// Stable sample IDs selecting one ordinary-binding combination, in active axis order.
        samples: Vec<String>,
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
pub struct OffscreenMappingPatch {
    /// Static Part slot or stable Part-binding sample combination to remap.
    pub slot: PartKeyformSlot,
    /// Offscreen keyform-array index; -1 explicitly selects the neutral/unmapped state.
    pub keyform: i32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum SceneContentEdit {
    MeshVertices {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        #[serde(default)]
        /// Explicit base/ordinary/BlendShape content selector; default Base edits the
        /// undeformed base shape.
        target: ShapeTarget,
        /// Ordered content patches applied together by the native edit.
        updates: Vec<VertexContentPatch>,
    },
    InsertMeshVertices {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// New vertex records in insertion order; each has a stable vertex ID and explicit
        /// dependent-shape seed.
        vertices: Vec<NewMeshVertex>,
        /// Optional stable vertex ID before which to insert new vertices; None appends. This is
        /// not a position-array index.
        before: Option<VertexId>,
    },
    DeleteMeshVertices {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Stable vertex IDs in this mesh, not offsets into positions.
        vertices: Vec<VertexId>,
        /// Explicit native conflict/conversion policy; no implicit removal or remapping is
        /// requested.
        policy: VertexDeletePolicy,
    },
    ReorderMeshVertices {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Complete new order of stable vertex IDs; preserves identities and associated per-
        /// vertex payload.
        order: Vec<VertexId>,
    },
    InsertTriangle {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Triangle corners as three stable vertex IDs; not position-array indices.
        vertices: [VertexId; 3],
        /// Optional zero-based array slot before which to insert; None appends. The anchor must
        /// belong to the selected destination.
        before: Option<usize>,
    },
    PatchTriangle {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Zero-based position within the addressed content array; not a stable object ID.
        slot: usize,
        /// Triangle corners as three stable vertex IDs; not position-array indices.
        vertices: [VertexId; 3],
    },
    DeleteTriangles {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Zero-based content-array positions selected by this operation.
        slots: Vec<usize>,
    },
    ReplaceTriangles {
        /// Stable ID of the target mesh in this document.
        mesh_id: String,
        /// Triangle triples of stable vertex IDs; preserve winding and do not substitute
        /// position-array offsets.
        triangles: Vec<[VertexId; 3]>,
    },
    WarpPoints {
        /// Stable deformation-transform ID in this document.
        transform_id: String,
        #[serde(default)]
        /// Explicit base/ordinary/BlendShape content selector; default Base edits the
        /// undeformed base shape.
        target: ShapeTarget,
        /// Ordered content patches applied together by the native edit.
        updates: Vec<WarpPointPatch>,
        #[serde(default)]
        /// Explicitly remove the authoring Bezier controller when editing baked lattice points.
        clear_bezier: bool,
    },
    WarpBezier {
        /// Stable deformation-transform ID in this document.
        transform_id: String,
        #[serde(default)]
        /// Explicit base/ordinary/BlendShape content selector; default Base edits the
        /// undeformed base shape.
        target: ShapeTarget,
        /// Complete authoring Bezier controller; its baked lattice is updated by the native edit.
        controller: Option<WarpBezier>,
    },
    ResizeWarp {
        /// Stable deformation-transform ID in this document.
        transform_id: String,
        /// Number of warp lattice cells vertically; a lattice has rows + 1 point rows.
        rows: u32,
        /// Number of warp lattice cells horizontally; a lattice has columns + 1 point columns.
        columns: u32,
        /// Whether to use the quad warp interpolation mode for the lattice.
        quad: bool,
    },
    InsertGluePair {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Complete Glue pair to insert or replace; endpoint IDs belong to the two connected
        /// meshes.
        pair: GlueVertexPair,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    PatchGluePair {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Complete Glue pair to insert or replace; endpoint IDs belong to the two connected
        /// meshes.
        pair: GlueVertexPair,
    },
    MoveGluePairs {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    TransferGluePairs {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Explicit destination collection; organization, deformation and drawing memberships
        /// are independent.
        destination: String,
        /// Ordered Glue vertex pairs with stable pair identities and endpoint vertex IDs.
        pairs: Vec<GlueVertexPair>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    DuplicateGluePairs {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    DeleteGluePairs {
        /// Stable Glue object ID owning the affected vertex pairs.
        glue_id: String,
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
    },
    InsertOffscreenKeyform {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Scalar value in the referenced parameter domain.
        value: OffscreenKeyform,
        /// Optional zero-based array slot before which to insert; None appends. The anchor must
        /// belong to the selected destination.
        before: Option<usize>,
    },
    PatchOffscreenKeyform {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Zero-based slot in the addressed triangle or Offscreen keyform array; never an
        /// object ID.
        slot: usize,
        /// Scalar value in the referenced parameter domain.
        value: OffscreenKeyform,
    },
    DuplicateOffscreenKeyforms {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Zero-based Offscreen keyform slots; native execution checks/remaps dependent Part
        /// mappings.
        slots: Vec<usize>,
        /// Optional zero-based array slot before which to insert; None appends. The anchor must
        /// belong to the selected destination.
        before: Option<usize>,
    },
    ReorderOffscreenKeyforms {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Permutation of zero-based Offscreen keyform slots; native execution remaps
        /// references consistently.
        order: Vec<usize>,
    },
    DeleteOffscreenKeyforms {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Zero-based Offscreen keyform slots; native execution checks/remaps dependent Part
        /// mappings.
        slots: Vec<usize>,
        /// Explicit native conflict/conversion policy; no implicit removal or remapping is
        /// requested.
        policy: OffscreenDeletePolicy,
    },
    SetOffscreenMapping {
        /// Stable Offscreen object ID; not a texture asset ID.
        offscreen_id: String,
        /// Ordered content patches applied together by the native edit.
        updates: Vec<OffscreenMappingPatch>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SceneContentDomain {
    MeshTopology,
    WarpTopology,
    GluePairs,
    OffscreenKeyforms,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct SceneContentReport {
    pub generated_pair_ids: BTreeMap<String, String>,
    pub removed_pair_ids: Vec<String>,
    pub removed_vertices: Vec<VertexId>,
    pub removed_triangle_slots: Vec<usize>,
    /// Original Offscreen keyform index -> new index, None for deleted forms.
    pub keyform_remap: Vec<Option<usize>>,
    pub resampled_shapes: usize,
    pub vertex_sources: Vec<NewMeshVertex>,
    pub losses: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct SceneContentPlan {
    domain: OrganizationPlan,
    report: SceneContentReport,
    kind: ChangeKind,
    cost: ConversionCost,
}
impl SceneContentPlan {
    pub fn is_noop(&self) -> bool {
        self.domain.no_op
    }
    pub fn affected_ids(&self) -> &[String] {
        &self.domain.affected
    }
    pub fn report(&self) -> &SceneContentReport {
        &self.report
    }
    pub fn cost(&self) -> ConversionCost {
        self.cost
    }
}
impl Document {
    pub fn plan_scene_content(&self, edit: SceneContentEdit) -> Result<SceneContentPlan, Status> {
        self.plan_scene_content_with_control(
            edit,
            AuthoringLimits::default(),
            &ConversionControl::default(),
        )
    }
    pub fn plan_scene_content_with_control(
        &self,
        edit: SceneContentEdit,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<SceneContentPlan, Status> {
        control.check()?;
        if !self.initialized() {
            return Err(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        // Includes isolated validation and history storage. Recheck at publication.
        let (work, growth) = self.content_growth(&edit)?;
        let retained = self.estimated_content_bytes();
        let payload = retained.checked_add(growth).ok_or_else(|| {
            Status::error("AUTHORING_CAPACITY_OVERFLOW", "Content payload overflows")
        })?;
        let cost = limits.check_conversion(work, payload, retained)?;
        let mut plan = SceneContentPlan {
            domain: OrganizationPlan {
                baseline: self.snapshot_identity.clone(),
                revision: self.revision(),
                delta: Delta::default(),
                affected: vec![],
                copied: BTreeMap::new(),
                effects: Default::default(),
                no_op: true,
            },
            report: Default::default(),
            kind: ChangeKind::Structure,
            cost,
        };
        match &edit {
            SceneContentEdit::MeshVertices { .. }
            | SceneContentEdit::InsertMeshVertices { .. }
            | SceneContentEdit::DeleteMeshVertices { .. }
            | SceneContentEdit::ReorderMeshVertices { .. }
            | SceneContentEdit::InsertTriangle { .. }
            | SceneContentEdit::PatchTriangle { .. }
            | SceneContentEdit::DeleteTriangles { .. }
            | SceneContentEdit::ReplaceTriangles { .. } => {
                self.plan_mesh_content(&mut plan, edit, control)?
            }
            SceneContentEdit::WarpPoints { .. }
            | SceneContentEdit::WarpBezier { .. }
            | SceneContentEdit::ResizeWarp { .. } => {
                self.plan_warp_content(&mut plan, edit, control)?
            }
            _ => self.plan_effect_content(&mut plan, edit, control)?,
        }
        plan.domain.delta.removed = plan.report.removed_pair_ids.clone();
        // A plan owns only changed resources; no-op preserves redo and revisions.
        plan.domain
            .delta
            .meshes
            .retain(|v| self.get_mesh(&v.id) != Some(v));
        plan.domain
            .delta
            .transforms
            .retain(|v| self.get_transform(&v.id) != Some(v));
        plan.domain
            .delta
            .bindings
            .retain(|v| self.get_binding(&v.id) != Some(v));
        plan.domain
            .delta
            .scene_bindings
            .retain(|v| self.get_scene_binding(&v.id) != Some(v));
        plan.domain
            .delta
            .blends
            .retain(|v| self.get_blend_binding(&v.id) != Some(v));
        plan.domain
            .delta
            .glues
            .retain(|v| self.get_glue(&v.id) != Some(v));
        plan.domain
            .delta
            .offscreens
            .retain(|v| self.get_offscreen(&v.id) != Some(v));
        plan.domain.no_op = !plan.domain.delta.changed(self);
        if !plan.is_noop() {
            self.validate_compound_organization(&plan.domain)?;
        }
        plan.domain.affected.sort();
        plan.domain.affected.dedup();
        control.check()?;
        Ok(plan)
    }
    pub fn apply_scene_content_plan(&mut self, plan: &SceneContentPlan) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Finish the active edit",
            ));
        }
        if !Arc::ptr_eq(&self.snapshot_identity, &plan.domain.baseline)
            || self.revision() != plan.domain.revision
        {
            return self.failed(Status::error("STALE_PLAN", "Replan scene content"));
        }
        if plan.is_noop() {
            return self.failed(Status::ok());
        }
        plan.domain.delta.install(self);
        for id in &plan.report.removed_pair_ids {
            self.content.editor_state.objects.remove(id);
        }
        self.changed(
            plan.kind,
            self.mesh_order().to_vec(),
            plan.domain.affected.clone(),
        )
    }
    pub(super) fn content_unlocked(&self, id: &str) -> Result<(), Status> {
        self.authoring_unlocked(id)
    }
}
fn check(status: Status) -> Result<(), Status> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(status)
    }
}
fn permutation(order: &[usize], len: usize) -> Result<(), Status> {
    if order.len() != len
        || order.iter().any(|&i| i >= len)
        || order.iter().copied().collect::<HashSet<_>>().len() != len
    {
        Err(Status::error(
            "INVALID_PERMUTATION",
            "Every original slot must occur exactly once",
        ))
    } else {
        Ok(())
    }
}
fn distinct<T: Eq + std::hash::Hash>(ids: &[T]) -> Result<(), Status> {
    if ids.iter().collect::<HashSet<_>>().len() != ids.len() {
        Err(Status::error(
            "DUPLICATE_SELECTION",
            "Repeated content address",
        ))
    } else {
        Ok(())
    }
}

impl Document {
    fn content_growth(&self, edit: &SceneContentEdit) -> Result<(usize, usize), Status> {
        use SceneContentEdit as E;
        let shapes = |id: &str| {
            1 + self.binding_for_mesh(id).map_or(0, |b| b.keyforms.len())
                + self.binding_for_scene(id).map_or(0, |b| b.track.len())
                + self
                    .iter_blend_bindings_for_target(id)
                    .map(|b| b.keyforms.len())
                    .sum::<usize>()
        };
        let (work, items, bytes) = match edit {
            E::InsertMeshVertices {
                mesh_id, vertices, ..
            } => (shapes(mesh_id), vertices.len(), std::mem::size_of::<Vec2>()),
            E::ResizeWarp {
                transform_id,
                rows,
                columns,
                ..
            } => {
                if *rows > 64 || *columns > 64 || *rows == 0 || *columns == 0 {
                    return Err(Status::error("INVALID_WARP_DIMENSIONS", transform_id));
                }
                (
                    shapes(transform_id),
                    (*rows as usize + 1) * (*columns as usize + 1),
                    std::mem::size_of::<Vec2>(),
                )
            }
            E::DuplicateGluePairs { ids, .. } => {
                (1, ids.len(), std::mem::size_of::<GlueVertexPair>() + 36)
            }
            E::InsertGluePair { .. } => (1, 1, std::mem::size_of::<GlueVertexPair>() + 36),
            E::DuplicateOffscreenKeyforms { slots, .. } => {
                (1, slots.len(), std::mem::size_of::<OffscreenKeyform>())
            }
            E::InsertOffscreenKeyform { .. } => (1, 1, std::mem::size_of::<OffscreenKeyform>()),
            E::ReplaceTriangles { triangles, .. } => {
                (1, triangles.len(), std::mem::size_of::<[VertexId; 3]>())
            }
            E::WarpBezier {
                controller: Some(c),
                ..
            } => (
                1,
                c.nodes.len(),
                std::mem::size_of::<crate::WarpBezierNode>(),
            ),
            _ => (0, 0, 0),
        };
        // Three arrays for newly inserted vertices cover base positions, UV and
        // IDs in addition to every ordinary/Blend shape's positions.
        let multiplier = if matches!(edit, E::InsertMeshVertices { .. }) {
            work.checked_add(3)
        } else {
            Some(work)
        };
        let growth = multiplier
            .and_then(|n| n.checked_mul(items))
            .and_then(|n| n.checked_mul(bytes))
            .ok_or_else(|| {
                Status::error(
                    "AUTHORING_CAPACITY_OVERFLOW",
                    "Content allocation overflows",
                )
            })?;
        Ok((work, growth))
    }
}

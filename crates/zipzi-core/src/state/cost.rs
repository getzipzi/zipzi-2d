//! Per-step estimates from touched records/roots only. Shared block identities
//! are deduplicated for this step, never registered in a lifetime accounting ledger.
//! Unchanged blocks at both endpoints cost zero; estimates are a soft policy input.
use super::*;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    mem::size_of,
};
#[derive(Default)]
struct Footprint {
    owned: usize,
    shared: HashMap<usize, usize>,
}
impl Footprint {
    fn add(&mut self, bytes: usize) {
        self.owned = self.owned.saturating_add(bytes);
    }
    fn shared<T: Retained>(&mut self, id: usize, value: &T) {
        if self.shared.contains_key(&id) {
            return;
        }
        let mut nested = Footprint::default();
        value.retained(&mut nested);
        self.shared.insert(
            id,
            size_of::<T>()
                .saturating_add(2 * size_of::<usize>())
                .saturating_add(nested.owned),
        );
        for (id, bytes) in nested.shared {
            self.shared.entry(id).or_insert(bytes);
        }
    }
}
trait Retained {
    const HEAP_FREE: bool = false;
    fn retained(&self, cost: &mut Footprint);
}
impl<T: Retained> Retained for Arc<T> {
    fn retained(&self, cost: &mut Footprint) {
        cost.shared(Arc::as_ptr(self) as usize, self.as_ref());
    }
}
impl<T: Retained> Retained for crate::SharedVec<T> {
    fn retained(&self, cost: &mut Footprint) {
        cost.shared(&**self as *const Vec<T> as usize, &**self);
    }
}
impl Retained for String {
    fn retained(&self, cost: &mut Footprint) {
        cost.add(self.capacity());
    }
}
impl<T: Retained> Retained for Vec<T> {
    fn retained(&self, cost: &mut Footprint) {
        cost.add(self.capacity().saturating_mul(size_of::<T>()));
        if !T::HEAP_FREE {
            for value in self {
                value.retained(cost);
            }
        }
    }
}
impl<T: Retained> Retained for Option<T> {
    fn retained(&self, cost: &mut Footprint) {
        if let Some(value) = self {
            value.retained(cost);
        }
    }
}
impl<T: Retained> Retained for HashMap<String, T> {
    fn retained(&self, cost: &mut Footprint) {
        cost.add(self.capacity().saturating_mul(size_of::<(String, T)>() + 1));
        for (key, value) in self {
            key.retained(cost);
            value.retained(cost);
        }
    }
}
impl<T: Retained> Retained for BTreeMap<String, T> {
    fn retained(&self, cost: &mut Footprint) {
        cost.add(
            self.len()
                .saturating_mul(11 * size_of::<(String, T)>() + 12 * size_of::<usize>() + 64),
        );
        for (key, value) in self {
            key.retained(cost);
            value.retained(cost);
        }
    }
}
impl Retained for Value {
    fn retained(&self, cost: &mut Footprint) {
        match self {
            Self::String(v) => v.retained(cost),
            Self::Array(v) => v.retained(cost),
            Self::Object(v) => {
                cost.add(v.len().saturating_mul(
                    11 * size_of::<(String, Value)>() + 12 * size_of::<usize>() + 64,
                ));
                for (k, v) in v {
                    k.retained(cost);
                    v.retained(cost);
                }
            }
            _ => {}
        }
    }
}
macro_rules! no_extra {($($ty:ty),*$(,)?)=>{$(impl Retained for $ty{const HEAP_FREE:bool=true;fn retained(&self,_:&mut Footprint){}})*};}
no_extra!(
    f32,
    f64,
    i32,
    u32,
    u8,
    [u32; 3],
    Vec2,
    OffscreenKeyform,
    MotionPoint,
    ObjectEditorState,
    CursorPreviewBinding,
    crate::WarpBezierNode,
    crate::MeshConstraint
);
macro_rules! fields {($($ty:ty=>[$($($field:ident).+),*$(,)?];)*)=>{$(impl Retained for $ty{fn retained(&self,cost:&mut Footprint){$(self.$($field).+.retained(cost);)*}})*};}
fields! {

    MotionKey => [id];
    Part => [id, runtime_id, name, parent_id];
    Parameter => [id, runtime_id, name];
    CdiParameterGroup => [id, runtime_id, name, parent_id, extensions];
    CdiCombinedSet => [id, members];
    CdiNamespaceIds => [parameters, parts, groups];
    DisplayInfo => [
        parameters,
        parameter_groups,
        parts,
        combined_parameters,
        extensions,
        opaque_source_ids,
    ];
    ExpressionEntry => [id, target, extensions];
    ExpressionAsset => [
        id,
        name,
        file_type,
        entries,
        extensions,
        opaque_source_ids,
        opaque_source_content_hash,
    ];
    MotionTrack => [id, target, initial, segments, extensions];
    MotionEvent => [id, value, extensions];
    MotionClip => [
        id,
        name,
        track_order,
        tracks,
        events,
        extensions,
        meta_extensions,
        opaque_source_ids,
        opaque_source_content_hash,
    ];
    MotionRegistration => [id, clip_id, sound, extensions];
    MotionGroup => [id, name, entries];
    PoseGroup => [id, name, entries];
    PoseLink => [id, target];
    PoseEntry => [id, part, links, extensions];
    PoseAsset => [id, file_type, groups, extensions, opaque_source_ids, opaque_source_content_hash];
    PhysicsAsset => [id, data, opaque_source_ids, opaque_source_content_hash];
    crate::document::PhysicsDefinition => [
        settings,
        extensions,
        meta.extensions,
        meta.forces.extensions,
        meta.dictionary,
    ];
    crate::physics::PhysicsDictionaryEntry => [id, name, extensions];
    crate::document::PhysicsSetting => [
        id,
        external_id,
        inputs,
        outputs,
        vertices,
        normalization.extensions,
        extensions,
    ];
    crate::document::PhysicsInput => [id, source, reference_extensions, extensions];
    crate::document::PhysicsOutput => [id, destination, reference_extensions, node_id, extensions];
    crate::document::PhysicsNode => [id, extensions];
    ModelParameterGroup => [id, name, parameters, extensions];
    ModelHitArea => [id, name, mesh, extensions];
    Model3Settings => [
        groups,
        layout,
        hit_areas,
        user_data,
        extensions,
        source_runtime_ids,
        source_content,
    ];
    BindingAxis => [id, samples, parameter_id, keys];
    MeshKeyform => [samples, keys, positions];
    MeshBinding => [id, mesh_id, axes, keyforms];
    RotationKeyform => [samples, keys];
    PartKeyform => [samples, keys];
    SceneBinding => [id, axes, track];
    DrawOrderGroup => [owner, items];
    BlendShapeKeyTable => [samples, base_sample_id, id, parameter_id, keys];
    BlendShapeConstraint => [samples, id, parameter_id, keys, weights];
    DeltaMeshKeyform => [sample_id, positions];
    DeltaWarpKeyform => [sample_id, points];
    BlendShapeBinding => [id, target_id, key_table_id, constraint_ids, keyforms];
    GlueBinding => [axes, keyforms];
    GlueVertexPair => [id];
    Glue => [id, runtime_id, name, mesh_a_id, mesh_b_id, pairs, binding];
    Offscreen => [id, runtime_id, name, part_id, masks, part_keyform_indices, keyforms];
    crate::BindingSample => [id];
    crate::ConstraintSample => [id];
    GlueKeyform => [samples];
    DeltaRotationKeyform => [sample_id];
    DeltaPartKeyform => [sample_id];
    DeltaGlueKeyform => [sample_id];
    DeltaOffscreenKeyform => [sample_id];

 AssetRecord=>[id,name,content];
 EditorState=>[objects,rotation_bone_lengths,rotation_bone_reference_scales,rotation_bone_angle_offsets,cursor_preview_bindings];
 HierarchyOrder=>[organization,deformation,glues];
 Mesh=>[id,name,texture_asset_id,vertex_ids,base_positions,uvs,triangles,meshing,runtime_id,part_id,deformer_id,masks];
 crate::MeshMeshing=>[boundary,constraints];
 Transform=>[id,runtime_id,name,part_id,parent_id,data,original_shape];
 crate::WarpTransform=>[points,bezier];
 crate::WarpBezier=>[nodes];
 crate::DeformerOriginal=>[capture_error,snapshot];
 crate::DeformerOriginalSnapshot=>[shape];
 WarpKeyform=>[samples,keys,positions,bezier];
 PackageAttachment=>[id,path,bytes];
 OwnedDocument=>[
     id, assets, parameters, parts, transforms, meshes, bindings, scene_bindings,
     blend_key_tables, blend_constraints, blend_bindings, glues, offscreens,
     expressions, motions, draw_order_groups, display_info, hierarchy_order,
     editor_state, motion_groups, motion_roots, pose, physics, missing_attachments,
     model3_settings, package_attachments,
 ];
}

impl OwnedDocument {
    /// Approximate storage of the decoded records, including shared payloads once.
    pub fn estimated_bytes(&self) -> usize {
        let mut cost = Footprint::default();
        self.retained(&mut cost);
        cost.shared.values().fold(
            cost.owned.saturating_add(size_of::<Self>()),
            |total, bytes| total.saturating_add(*bytes),
        )
    }
}
impl<T: Retained> Retained for ReferenceEntry<T> {
    fn retained(&self, cost: &mut Footprint) {
        self.id.retained(cost);
        self.target.retained(cost);
    }
}
impl Retained for crate::SceneReference {
    fn retained(&self, cost: &mut Footprint) {
        cost.add(self.capacity());
    }
}
impl Retained for crate::TransformId {
    fn retained(&self, cost: &mut Footprint) {
        cost.add(self.capacity());
    }
}
impl Retained for crate::PartId {
    fn retained(&self, cost: &mut Footprint) {
        cost.add(self.capacity());
    }
}
impl Retained for AssetContent {
    fn retained(&self, cost: &mut Footprint) {
        if let Self::Unresolved { source } = self {
            source.retained(cost);
        }
    }
}
impl Retained for TransformData {
    fn retained(&self, cost: &mut Footprint) {
        if let Self::Warp(w) = self {
            w.retained(cost);
        }
    }
}
impl Retained for crate::DeformerOriginalShape {
    fn retained(&self, cost: &mut Footprint) {
        if let Self::Warp { warp, .. } = self {
            warp.retained(cost);
        }
    }
}
impl Retained for MotionSegment {
    fn retained(&self, cost: &mut Footprint) {
        self.end().retained(cost);
    }
}
impl Retained for SceneTrack {
    fn retained(&self, cost: &mut Footprint) {
        match self {
            Self::Warp {
                target_id,
                keyforms,
            } => {
                target_id.retained(cost);
                keyforms.retained(cost);
            }
            Self::Rotation {
                target_id,
                keyforms,
            } => {
                target_id.retained(cost);
                keyforms.retained(cost);
            }
            Self::Part {
                target_id,
                keyforms,
            } => {
                target_id.retained(cost);
                keyforms.retained(cost);
            }
        }
    }
}
impl Retained for DeltaKeyforms {
    fn retained(&self, cost: &mut Footprint) {
        match self {
            Self::Mesh(v) => v.retained(cost),
            Self::Warp(v) => v.retained(cost),
            Self::Rotation(v) => v.retained(cost),
            Self::Part(v) => v.retained(cost),
            Self::Glue(v) => v.retained(cost),
            Self::Offscreen(v) => v.retained(cost),
        }
    }
}
impl Retained for CdiParameterEntry {
    fn retained(&self, cost: &mut Footprint) {
        match self {
            Self::Resolved {
                parameter_id,
                group_id,
                extensions,
            } => {
                parameter_id.retained(cost);
                group_id.retained(cost);
                extensions.retained(cost);
            }
            Self::Unresolved {
                runtime_id,
                name,
                group_id,
                extensions,
            } => {
                runtime_id.retained(cost);
                name.retained(cost);
                group_id.retained(cost);
                extensions.retained(cost);
            }
        }
    }
}
impl Retained for CdiPartEntry {
    fn retained(&self, cost: &mut Footprint) {
        match self {
            Self::Resolved {
                part_id,
                extensions,
            } => {
                part_id.retained(cost);
                extensions.retained(cost);
            }
            Self::Unresolved {
                runtime_id,
                name,
                extensions,
            } => {
                runtime_id.retained(cost);
                name.retained(cost);
                extensions.retained(cost);
            }
        }
    }
}
impl Retained for CdiParameterRef {
    fn retained(&self, cost: &mut Footprint) {
        match self {
            Self::Unspecified => {}
            Self::Resolved { parameter_id } => {
                parameter_id.retained(cost);
            }
            Self::Unresolved { runtime_id } => {
                runtime_id.retained(cost);
            }
        }
    }
}
impl Retained for ExpressionTarget {
    fn retained(&self, cost: &mut Footprint) {
        match self {
            Self::Unspecified => {}
            Self::Resolved { parameter_id } => {
                parameter_id.retained(cost);
            }
            Self::Unresolved { runtime_id } => {
                runtime_id.retained(cost);
            }
        }
    }
}
impl Retained for MotionTrackTarget {
    fn retained(&self, cost: &mut Footprint) {
        match self {
            Self::Unspecified => {}
            Self::Model { runtime_id } => {
                runtime_id.retained(cost);
            }
            Self::Unresolved {
                category,
                runtime_id,
            } => {
                category.retained(cost);
                runtime_id.retained(cost);
            }
            Self::Parameter { parameter_id } => {
                parameter_id.retained(cost);
            }
            Self::PartOpacity { part_id } => {
                part_id.retained(cost);
            }
        }
    }
}
impl Retained for PosePartRef {
    fn retained(&self, cost: &mut Footprint) {
        match self {
            Self::Unspecified => {}
            Self::Resolved { part_id } => {
                part_id.retained(cost);
            }
            Self::Unresolved { runtime_id } => {
                runtime_id.retained(cost);
            }
        }
    }
}
impl Retained for ModelTargetRef {
    fn retained(&self, cost: &mut Footprint) {
        match self {
            Self::Unspecified => {}
            Self::Resolved { object_id } => {
                object_id.retained(cost);
            }
            Self::Unresolved { runtime_id } => {
                runtime_id.retained(cost);
            }
        }
    }
}
impl Retained for AttachmentRef {
    fn retained(&self, cost: &mut Footprint) {
        match self {
            Self::Resolved { attachment_id } => {
                attachment_id.retained(cost);
            }
            Self::Unresolved { path } => {
                path.retained(cost);
            }
        }
    }
}
impl Retained for PhysicsTarget {
    fn retained(&self, cost: &mut Footprint) {
        match self {
            Self::Unspecified => {}
            Self::Resolved { parameter_id } => {
                parameter_id.retained(cost);
            }
            Self::Unresolved { runtime_id } => {
                runtime_id.retained(cost);
            }
        }
    }
}
fn records<T: Retained>(records: &Records<T>, ids: &[&str], cost: &mut Footprint) {
    for id in ids {
        if let Some(value) = records.shared_map().get(*id) {
            value.retained(cost);
        }
    }
    // COW directory metadata includes all copied keys, but never traverses unrelated payloads.
    cost.add(
        records
            .shared_map()
            .capacity()
            .saturating_mul(size_of::<(String, Arc<T>)>() + 1),
    );
    for id in records.shared_map().keys() {
        cost.add(id.capacity());
    }
}
pub(super) fn estimate(base: &Content, after: &Content, writes: &WriteSet) -> usize {
    if writes.is_empty() {
        return 0;
    }
    let mut before = Footprint::default();
    let mut next = Footprint::default();
    let ids: Vec<_> = writes
        .records
        .iter()
        .filter(|w| w.domain == RecordDomain::Asset)
        .map(|w| w.id.as_str())
        .collect();
    if !ids.is_empty() {
        records(&base.assets, &ids, &mut before);
        records(&after.assets, &ids, &mut next);
    }
    if writes.order_changed(RecordDomain::Asset) {
        base.assets.order_vec().retained(&mut before);
        after.assets.order_vec().retained(&mut next);
    }
    let ids: Vec<_> = writes
        .records
        .iter()
        .filter(|w| w.domain == RecordDomain::Parameter)
        .map(|w| w.id.as_str())
        .collect();
    if !ids.is_empty() {
        records(&base.parameters, &ids, &mut before);
        records(&after.parameters, &ids, &mut next);
    }
    if writes.order_changed(RecordDomain::Parameter) {
        base.parameters.order_vec().retained(&mut before);
        after.parameters.order_vec().retained(&mut next);
    }
    let ids: Vec<_> = writes
        .records
        .iter()
        .filter(|w| w.domain == RecordDomain::Part)
        .map(|w| w.id.as_str())
        .collect();
    if !ids.is_empty() {
        records(&base.parts, &ids, &mut before);
        records(&after.parts, &ids, &mut next);
    }
    if writes.order_changed(RecordDomain::Part) {
        base.parts.order_vec().retained(&mut before);
        after.parts.order_vec().retained(&mut next);
    }
    let ids: Vec<_> = writes
        .records
        .iter()
        .filter(|w| w.domain == RecordDomain::Transform)
        .map(|w| w.id.as_str())
        .collect();
    if !ids.is_empty() {
        records(&base.transforms, &ids, &mut before);
        records(&after.transforms, &ids, &mut next);
    }
    if writes.order_changed(RecordDomain::Transform) {
        base.transforms.order_vec().retained(&mut before);
        after.transforms.order_vec().retained(&mut next);
    }
    let ids: Vec<_> = writes
        .records
        .iter()
        .filter(|w| w.domain == RecordDomain::Mesh)
        .map(|w| w.id.as_str())
        .collect();
    if !ids.is_empty() {
        records(&base.meshes, &ids, &mut before);
        records(&after.meshes, &ids, &mut next);
    }
    if writes.order_changed(RecordDomain::Mesh) {
        base.meshes.order_vec().retained(&mut before);
        after.meshes.order_vec().retained(&mut next);
    }
    let ids: Vec<_> = writes
        .records
        .iter()
        .filter(|w| w.domain == RecordDomain::Binding)
        .map(|w| w.id.as_str())
        .collect();
    if !ids.is_empty() {
        records(&base.bindings, &ids, &mut before);
        records(&after.bindings, &ids, &mut next);
    }
    if writes.order_changed(RecordDomain::Binding) {
        base.bindings.order_vec().retained(&mut before);
        after.bindings.order_vec().retained(&mut next);
    }
    let ids: Vec<_> = writes
        .records
        .iter()
        .filter(|w| w.domain == RecordDomain::SceneBinding)
        .map(|w| w.id.as_str())
        .collect();
    if !ids.is_empty() {
        records(&base.scene_bindings, &ids, &mut before);
        records(&after.scene_bindings, &ids, &mut next);
    }
    if writes.order_changed(RecordDomain::SceneBinding) {
        base.scene_bindings.order_vec().retained(&mut before);
        after.scene_bindings.order_vec().retained(&mut next);
    }
    let ids: Vec<_> = writes
        .records
        .iter()
        .filter(|w| w.domain == RecordDomain::BlendKeyTable)
        .map(|w| w.id.as_str())
        .collect();
    if !ids.is_empty() {
        records(&base.blend_key_tables, &ids, &mut before);
        records(&after.blend_key_tables, &ids, &mut next);
    }
    if writes.order_changed(RecordDomain::BlendKeyTable) {
        base.blend_key_tables.order_vec().retained(&mut before);
        after.blend_key_tables.order_vec().retained(&mut next);
    }
    let ids: Vec<_> = writes
        .records
        .iter()
        .filter(|w| w.domain == RecordDomain::BlendConstraint)
        .map(|w| w.id.as_str())
        .collect();
    if !ids.is_empty() {
        records(&base.blend_constraints, &ids, &mut before);
        records(&after.blend_constraints, &ids, &mut next);
    }
    if writes.order_changed(RecordDomain::BlendConstraint) {
        base.blend_constraints.order_vec().retained(&mut before);
        after.blend_constraints.order_vec().retained(&mut next);
    }
    let ids: Vec<_> = writes
        .records
        .iter()
        .filter(|w| w.domain == RecordDomain::BlendBinding)
        .map(|w| w.id.as_str())
        .collect();
    if !ids.is_empty() {
        records(&base.blend_bindings, &ids, &mut before);
        records(&after.blend_bindings, &ids, &mut next);
    }
    if writes.order_changed(RecordDomain::BlendBinding) {
        base.blend_bindings.order_vec().retained(&mut before);
        after.blend_bindings.order_vec().retained(&mut next);
    }
    let ids: Vec<_> = writes
        .records
        .iter()
        .filter(|w| w.domain == RecordDomain::Glue)
        .map(|w| w.id.as_str())
        .collect();
    if !ids.is_empty() {
        records(&base.glues, &ids, &mut before);
        records(&after.glues, &ids, &mut next);
    }
    if writes.order_changed(RecordDomain::Glue) {
        base.glues.order_vec().retained(&mut before);
        after.glues.order_vec().retained(&mut next);
    }
    let ids: Vec<_> = writes
        .records
        .iter()
        .filter(|w| w.domain == RecordDomain::Offscreen)
        .map(|w| w.id.as_str())
        .collect();
    if !ids.is_empty() {
        records(&base.offscreens, &ids, &mut before);
        records(&after.offscreens, &ids, &mut next);
    }
    if writes.order_changed(RecordDomain::Offscreen) {
        base.offscreens.order_vec().retained(&mut before);
        after.offscreens.order_vec().retained(&mut next);
    }
    let ids: Vec<_> = writes
        .records
        .iter()
        .filter(|w| w.domain == RecordDomain::Expression)
        .map(|w| w.id.as_str())
        .collect();
    if !ids.is_empty() {
        records(&base.expressions, &ids, &mut before);
        records(&after.expressions, &ids, &mut next);
    }
    if writes.order_changed(RecordDomain::Expression) {
        base.expressions.order_vec().retained(&mut before);
        after.expressions.order_vec().retained(&mut next);
    }
    let ids: Vec<_> = writes
        .records
        .iter()
        .filter(|w| w.domain == RecordDomain::Motion)
        .map(|w| w.id.as_str())
        .collect();
    if !ids.is_empty() {
        records(&base.motions, &ids, &mut before);
        records(&after.motions, &ids, &mut next);
    }
    if writes.order_changed(RecordDomain::Motion) {
        base.motions.order_vec().retained(&mut before);
        after.motions.order_vec().retained(&mut next);
    }
    if writes.root_changed(RootDomain::DrawOrderGroups) {
        base.draw_order_groups.retained(&mut before);
        after.draw_order_groups.retained(&mut next);
    }
    if writes.root_changed(RootDomain::DisplayInfo) {
        base.display_info.retained(&mut before);
        after.display_info.retained(&mut next);
    }
    if writes.root_changed(RootDomain::HierarchyOrder) {
        base.hierarchy_order.retained(&mut before);
        after.hierarchy_order.retained(&mut next);
    }
    if writes.root_changed(RootDomain::EditorState) {
        base.editor_state.retained(&mut before);
        after.editor_state.retained(&mut next);
    }
    if writes.root_changed(RootDomain::MotionGroups) {
        base.motion_groups.retained(&mut before);
        after.motion_groups.retained(&mut next);
    }
    if writes.root_changed(RootDomain::MotionRoots) {
        base.motion_roots.retained(&mut before);
        after.motion_roots.retained(&mut next);
    }
    if writes.root_changed(RootDomain::Pose) {
        base.pose.retained(&mut before);
        after.pose.retained(&mut next);
    }
    if writes.root_changed(RootDomain::Physics) {
        base.physics.retained(&mut before);
        after.physics.retained(&mut next);
    }
    if writes.root_changed(RootDomain::MissingAttachments) {
        base.missing_attachments.retained(&mut before);
        after.missing_attachments.retained(&mut next);
    }
    if writes.root_changed(RootDomain::Model3Settings) {
        base.model3_settings.retained(&mut before);
        after.model3_settings.retained(&mut next);
    }
    if writes.root_changed(RootDomain::PackageAttachments) {
        base.package_attachments.retained(&mut before);
        after.package_attachments.retained(&mut next);
    }
    if !Arc::ptr_eq(&base.parameter_runtime_ids, &after.parameter_runtime_ids) {
        base.parameter_runtime_ids.retained(&mut before);
        after.parameter_runtime_ids.retained(&mut next);
    }
    let mut bytes = (2 * size_of::<Content>())
        .saturating_add(before.owned)
        .saturating_add(next.owned);
    for (id, cost) in &before.shared {
        if !next.shared.contains_key(id) {
            bytes = bytes.saturating_add(*cost);
        }
    }
    for (id, cost) in &next.shared {
        if !before.shared.contains_key(id) {
            bytes = bytes.saturating_add(*cost);
        }
    }
    for write in &writes.records {
        bytes = bytes
            .saturating_add(size_of::<RecordWrite>())
            .saturating_add(write.id.capacity());
    }
    bytes
}

/// Operation-local scene input payload. Immutable unselected records are excluded.
pub(crate) fn scene_payload(
    model: &crate::model_validation::Model<'_>,
    ids: &std::collections::HashSet<String>,
) -> usize {
    let mut cost = Footprint::default();
    macro_rules! add {
        ($values:expr) => {
            for value in $values {
                cost.add(size_of_val(value));
                value.retained(&mut cost);
            }
        };
    }
    use std::mem::size_of_val;
    add!(model
        .content
        .parameters
        .values()
        .filter(|v| ids.contains(&v.id)));
    add!(model.content.parts.values().filter(|v| ids.contains(&v.id)));
    add!(model
        .content
        .meshes
        .values()
        .filter(|v| ids.contains(&v.id)));
    add!(model
        .content
        .transforms
        .values()
        .filter(|v| ids.contains(&v.id)));
    add!(model
        .content
        .bindings
        .values()
        .filter(|v| ids.contains(&v.id) || ids.contains(&v.mesh_id)));
    add!(model
        .content
        .scene_bindings
        .values()
        .filter(|v| ids.contains(&v.id) || ids.contains(v.target_id())));
    add!(model
        .content
        .blend_bindings
        .values()
        .filter(|v| ids.contains(&v.id) || ids.contains(&v.target_id)));
    add!(model.content.glues.values().filter(|v| ids.contains(&v.id)
        || ids.contains(v.mesh_a_id.as_str())
        || ids.contains(v.mesh_b_id.as_str())));
    add!(model
        .content
        .offscreens
        .values()
        .filter(|v| ids.contains(&v.id) || ids.contains(&v.part_id)));
    cost.shared
        .values()
        .fold(cost.owned, |n, bytes| n.saturating_add(*bytes))
}

/// Only records and metadata that dependency deletion can rewrite are charged.
pub(super) fn deletion_payload(
    model: &crate::model_validation::Model<'_>,
    report: &DeletionReport,
) -> usize {
    let ids: std::collections::HashSet<_> = report
        .removed
        .iter()
        .map(|r| r.object.id.clone())
        .chain(report.rewritten.iter().map(|r| r.owner.id.clone()))
        .collect();
    let mut cost = Footprint::default();
    cost.add(scene_payload(model, &ids));
    macro_rules! add {
        ($values:expr) => {
            for value in $values {
                cost.add(std::mem::size_of_val(value));
                value.retained(&mut cost);
            }
        };
    }
    add!(model
        .content
        .blend_key_tables
        .values()
        .filter(|v| ids.contains(&v.id)));
    add!(model
        .content
        .blend_constraints
        .values()
        .filter(|v| ids.contains(&v.id)));
    add!(model
        .content
        .motions
        .values()
        .filter(|v| ids.contains(&v.id)
            || v.tracks
                .iter()
                .any(|t| ids.contains(&t.id) || t.keys().any(|k| ids.contains(&k.id)))
            || v.events.iter().any(|e| ids.contains(&e.id))));
    add!(model
        .content
        .expressions
        .values()
        .filter(|v| ids.contains(&v.id)
            || v.entries.iter().any(|e| ids.contains(&e.id))
            || report
                .removed
                .iter()
                .any(|r| r.object.owner_id.as_deref() == Some(&v.id))));
    if let Some(pose) = model
        .pose()
        .filter(|v| v.entity_ids().any(|id| ids.contains(id)))
    {
        pose.retained(&mut cost);
    }
    if let Some(physics) = model
        .physics()
        .filter(|v| v.entity_ids().any(|id| ids.contains(id)))
    {
        physics.retained(&mut cost);
    }
    // These small ordered roots are reconstructed together by the business patch.
    model.display_info().retained(&mut cost);
    model.model3_settings().retained(&mut cost);
    model.editor_state().retained(&mut cost);
    model.content.motion_groups.retained(&mut cost);
    model.content.motion_roots.retained(&mut cost);
    model.content.package_attachments.retained(&mut cost);
    cost.shared
        .values()
        .fold(cost.owned, |n, b| n.saturating_add(*b))
}

pub(crate) fn expression_entries_bytes(entries: &Vec<ExpressionEntry>) -> usize {
    let mut cost = Footprint::default();
    entries.retained(&mut cost);
    cost.shared
        .values()
        .fold(cost.owned, |n, b| n.saturating_add(*b))
}

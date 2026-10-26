//! Capacity-aware estimate of persistent content allocations. Keep this in
//! sync with DocumentContent and the value types stored in its collections.
use std::collections::{BTreeMap, HashMap};
use std::mem::size_of;

use crate::draw_order::DrawOrderGroup;
use crate::types::{
    BindingAxis, BlendShapeBinding, BlendShapeConstraint, BlendShapeKeyTable, DeltaGlueKeyform,
    DeltaKeyforms, DeltaMeshKeyform, DeltaOffscreenKeyform, DeltaPartKeyform, DeltaRotationKeyform,
    DeltaWarpKeyform, Glue, GlueBinding, GlueKeyform, GlueVertexPair, ImageAsset, Mesh,
    MeshBinding, MeshKeyform, Offscreen, OffscreenKeyform, Parameter, Part, PartKeyform,
    RotationKeyform, SceneBinding, SceneTrack, Transform, TransformData, Vec2, WarpKeyform,
};

use super::super::DocumentContent;
use super::super::PackageAttachment;
use super::super::PhysicsAsset;
use super::super::{
    CdiCombinedSet, CdiNamespaceIds, CdiParameterEntry, CdiParameterGroup, CdiParameterRef,
    CdiPartEntry, DisplayInfo,
};
use super::super::{ExpressionAsset, ExpressionEntry, ExpressionTarget};
use super::super::{Model3Settings, ModelHitArea, ModelParameterGroup, ModelTargetRef};
use super::super::{
    MotionClip, MotionEvent, MotionGroup, MotionKey, MotionPoint, MotionRegistration,
    MotionSegment, MotionTrack, MotionTrackTarget,
};
use super::super::{PoseAsset, PoseEntry, PoseGroup, PoseLink, PosePartRef};
use serde_json::Value;

trait ExtraBytes {
    fn extra_bytes(&self) -> usize;
}

impl<T: ExtraBytes> ExtraBytes for std::sync::Arc<T> {
    fn extra_bytes(&self) -> usize {
        // Charge shared allocations in each checkpoint conservatively. This
        // keeps the existing budget a safe upper bound instead of undercounting.
        2 * size_of::<usize>() + size_of::<T>() + self.as_ref().extra_bytes()
    }
}
impl ExtraBytes for String {
    fn extra_bytes(&self) -> usize {
        self.capacity()
    }
}
impl ExtraBytes for crate::SceneReference {
    fn extra_bytes(&self) -> usize {
        self.capacity()
    }
}
impl<T: ExtraBytes> ExtraBytes for Vec<T> {
    fn extra_bytes(&self) -> usize {
        self.capacity() * size_of::<T>() + self.iter().map(ExtraBytes::extra_bytes).sum::<usize>()
    }
}
impl<T: ExtraBytes> ExtraBytes for crate::SharedVec<T> {
    fn extra_bytes(&self) -> usize {
        // Charge each shared vector conservatively in every checkpoint.
        2 * size_of::<usize>() + size_of::<Vec<T>>() + (**self).extra_bytes()
    }
}
impl<T: ExtraBytes> ExtraBytes for Option<T> {
    fn extra_bytes(&self) -> usize {
        self.as_ref().map_or(0, ExtraBytes::extra_bytes)
    }
}
impl<T: ExtraBytes> ExtraBytes for HashMap<String, T> {
    fn extra_bytes(&self) -> usize {
        // Hashbrown's control-byte allocation is included approximately.
        self.capacity() * (size_of::<(String, T)>() + 1)
            + self
                .iter()
                .map(|(key, value)| key.capacity() + value.extra_bytes())
                .sum::<usize>()
    }
}
impl<T: ExtraBytes> ExtraBytes for BTreeMap<String, T> {
    fn extra_bytes(&self) -> usize {
        // Charge a full 11-entry B-tree node for every occupied entry. This
        // deliberately overestimates small and nested maps, including a map
        // with one item, rather than undercounting spare node slots.
        let nodes = self
            .len()
            .saturating_mul(11 * size_of::<(String, T)>() + 12 * size_of::<usize>() + 64);
        nodes
            + self
                .iter()
                .map(|(key, value)| key.capacity() + value.extra_bytes())
                .sum::<usize>()
    }
}
impl ExtraBytes for Value {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::String(value) => value.capacity(),
            Self::Array(items) => items.extra_bytes(),
            Self::Object(items) => {
                let nodes = items.len().saturating_mul(
                    11 * size_of::<(String, Value)>() + 12 * size_of::<usize>() + 64,
                );
                nodes
                    + items
                        .iter()
                        .map(|(key, value)| key.capacity() + value.extra_bytes())
                        .sum::<usize>()
            }
            _ => 0,
        }
    }
}

macro_rules! no_extra {
    ($($ty:ty),+ $(,)?) => {$(
        impl ExtraBytes for $ty { fn extra_bytes(&self) -> usize { 0 } }
    )+};
}
no_extra!(
    super::super::ObjectEditorState,
    f32,
    f64,
    i32,
    u32,
    Vec2,
    OffscreenKeyform,
    MotionPoint
);

// Ordinary records only sum their heap-owning fields. Container allocation
// policies and enum payloads stay explicit below.
macro_rules! extra_fields {
    ($($ty:ty => [$($($field:ident).+),+ $(,)?];)+) => {$(
        impl ExtraBytes for $ty {
            fn extra_bytes(&self) -> usize {
                0 $(+ self.$($field).+.extra_bytes())+
            }
        }
    )+};
}

extra_fields! {
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
    super::super::PhysicsDefinition => [
        settings,
        extensions,
        meta.extensions,
        meta.forces.extensions,
        meta.dictionary,
    ];
    crate::physics::PhysicsDictionaryEntry => [id, name, extensions];
    super::super::PhysicsSetting => [
        id,
        external_id,
        inputs,
        outputs,
        vertices,
        normalization.extensions,
        extensions,
    ];
    super::super::PhysicsInput => [id, source, reference_extensions, extensions];
    super::super::PhysicsOutput => [id, destination, reference_extensions, node_id, extensions];
    super::super::PhysicsNode => [id, extensions];
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
    DocumentContent => [
        id,
        assets,
        asset_order,
        parts,
        part_order,
        transforms,
        transform_order,
        meshes,
        mesh_order,
        parameters,
        parameter_order,
        bindings,
        binding_order,
        scene_bindings,
        scene_binding_order,
        draw_order_groups,
        blend_key_tables,
        blend_key_table_order,
        blend_constraints,
        blend_constraint_order,
        blend_bindings,
        blend_binding_order,
        glues,
        glue_order,
        offscreens,
        offscreen_order,
        display_info,
        editor_state.objects,
        hierarchy_order.organization,
        hierarchy_order.deformation,
        hierarchy_order.glues,
        expressions,
        expression_order,
        motions,
        motion_order,
        motion_groups,
        motion_roots,
        pose,
        physics,
        missing_attachments,
        model3_settings,
        package_attachments,
    ];
}

impl ExtraBytes for MotionSegment {
    fn extra_bytes(&self) -> usize {
        self.end().extra_bytes()
    }
}

impl ExtraBytes for ImageAsset {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.name.capacity()
            + self.unresolved_source.as_ref().map_or(0, String::capacity)
            + self
                .content
                .as_ref()
                .map_or(0, |c| c.source.capacity() + c.sha256.capacity())
    }
}

impl ExtraBytes for Transform {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.runtime_id.capacity()
            + self.name.capacity()
            + self.part_id.as_ref().map_or(0, |id| id.capacity())
            + self.parent_id.as_ref().map_or(0, |id| id.capacity())
            + match &self.data {
                TransformData::Warp(w) => {
                    w.points.extra_bytes()
                        + w.bezier.as_ref().map_or(0, |b| {
                            b.nodes.capacity() * std::mem::size_of::<crate::WarpBezierNode>()
                        })
                }
                TransformData::Rotation(_) => 0,
            }
    }
}
impl ExtraBytes for Mesh {
    fn extra_bytes(&self) -> usize {
        self.id.capacity()
            + self.name.capacity()
            + self.texture_asset_id.capacity()
            + self.vertex_ids.extra_bytes()
            + self.base_positions.extra_bytes()
            + self.uvs.extra_bytes()
            + self.triangles.capacity() * size_of::<[u32; 3]>()
            + self.meshing.as_ref().map_or(0, |m| {
                m.boundary.capacity() * size_of::<u32>()
                    + m.constraints.capacity() * size_of::<crate::MeshConstraint>()
            })
            + self.runtime_id.capacity()
            + self.part_id.capacity()
            + self.deformer_id.capacity()
            + self.masks.extra_bytes()
    }
}

impl ExtraBytes for CdiParameterEntry {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Resolved {
                parameter_id,
                group_id,
                extensions,
            } => parameter_id.capacity() + group_id.extra_bytes() + extensions.extra_bytes(),
            Self::Unresolved {
                runtime_id,
                name,
                group_id,
                extensions,
            } => {
                runtime_id.capacity()
                    + name.capacity()
                    + group_id.extra_bytes()
                    + extensions.extra_bytes()
            }
        }
    }
}
impl ExtraBytes for CdiPartEntry {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Resolved {
                part_id,
                extensions,
            } => part_id.capacity() + extensions.extra_bytes(),
            Self::Unresolved {
                runtime_id,
                name,
                extensions,
            } => runtime_id.capacity() + name.capacity() + extensions.extra_bytes(),
        }
    }
}
impl ExtraBytes for CdiParameterRef {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Unspecified => 0,
            Self::Resolved { parameter_id } => parameter_id.capacity(),
            Self::Unresolved { runtime_id } => runtime_id.capacity(),
        }
    }
}

impl ExtraBytes for ExpressionTarget {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Unspecified => 0,
            Self::Resolved { parameter_id } => parameter_id.capacity(),
            Self::Unresolved { runtime_id } => runtime_id.capacity(),
        }
    }
}

impl ExtraBytes for MotionTrackTarget {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Unspecified => 0,
            Self::Model { runtime_id } | Self::Unresolved { runtime_id, .. } => {
                runtime_id.capacity()
                    + if let Self::Unresolved { category, .. } = self {
                        category.capacity()
                    } else {
                        0
                    }
            }
            Self::Parameter { parameter_id } => parameter_id.capacity(),
            Self::PartOpacity { part_id } => part_id.capacity(),
        }
    }
}

impl ExtraBytes for PosePartRef {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Unspecified => 0,
            Self::Resolved { part_id } => part_id.capacity(),
            Self::Unresolved { runtime_id } => runtime_id.capacity(),
        }
    }
}

impl ExtraBytes for ModelTargetRef {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Unspecified => 0,
            Self::Resolved { object_id } => object_id.capacity(),
            Self::Unresolved { runtime_id } => runtime_id.capacity(),
        }
    }
}
impl<T: ExtraBytes> ExtraBytes for super::super::ReferenceEntry<T> {
    fn extra_bytes(&self) -> usize {
        self.id.capacity() + self.target.extra_bytes()
    }
}

impl ExtraBytes for PackageAttachment {
    fn extra_bytes(&self) -> usize {
        self.id.capacity() + self.path.capacity() + self.bytes.as_ref().map_or(0, |b| b.capacity())
    }
}

impl ExtraBytes for WarpKeyform {
    fn extra_bytes(&self) -> usize {
        self.samples.extra_bytes()
            + self.keys.extra_bytes()
            + self.positions.extra_bytes()
            + self.bezier.as_ref().map_or(0, |b| {
                b.nodes.capacity() * std::mem::size_of::<crate::WarpBezierNode>()
            })
    }
}

impl ExtraBytes for SceneTrack {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Warp {
                target_id,
                keyforms,
            } => target_id.capacity() + keyforms.extra_bytes(),
            Self::Rotation {
                target_id,
                keyforms,
            } => target_id.capacity() + keyforms.extra_bytes(),
            Self::Part {
                target_id,
                keyforms,
            } => target_id.capacity() + keyforms.extra_bytes(),
        }
    }
}

impl ExtraBytes for DeltaKeyforms {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Mesh(v) => v.extra_bytes(),
            Self::Warp(v) => v.extra_bytes(),
            Self::Rotation(v) => v.extra_bytes(),
            Self::Part(v) => v.extra_bytes(),
            Self::Glue(v) => v.extra_bytes(),
            Self::Offscreen(v) => v.extra_bytes(),
        }
    }
}

pub(super) fn estimated_content_bytes(content: &DocumentContent) -> usize {
    #[cfg(feature = "bench-instrumentation")]
    let _benchmark_span = crate::benchmark::span("core.estimate_content_size");
    size_of::<DocumentContent>() + content.extra_bytes()
}

pub(super) fn expression_entries_bytes(entries: &Vec<ExpressionEntry>) -> usize {
    entries.extra_bytes()
}

impl ExtraBytes for super::super::AttachmentRef {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Resolved { attachment_id } => attachment_id.capacity(),
            Self::Unresolved { path } => path.capacity(),
        }
    }
}

impl ExtraBytes for super::super::PhysicsTarget {
    fn extra_bytes(&self) -> usize {
        match self {
            Self::Unspecified => 0,
            Self::Resolved { parameter_id } => parameter_id.capacity(),
            Self::Unresolved { runtime_id } => runtime_id.capacity(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn manual_mesh_history_charges_boundary_and_constraint_spare_capacity() {
        let mut mesh = Mesh {
            meshing: Some(crate::MeshMeshing {
                boundary: vec![0, 1, 2],
                constraints: vec![],
            }),
            ..Default::default()
        };
        let before = mesh.extra_bytes();
        let meshing = mesh.meshing.as_mut().unwrap();
        let old_boundary_capacity = meshing.boundary.capacity();
        meshing.boundary.reserve_exact(4096);
        meshing.constraints.reserve_exact(8192);
        let growth = (meshing.boundary.capacity() - old_boundary_capacity) * size_of::<u32>()
            + meshing.constraints.capacity() * size_of::<crate::MeshConstraint>();
        assert_eq!(mesh.extra_bytes(), before + growth);
        // Snapshot clones own only the used elements; each retained allocation is measured
        // independently, rather than borrowing the source's capacity or ignoring its metadata.
        let snapshot = mesh.clone();
        assert!(snapshot.extra_bytes() < mesh.extra_bytes());
        assert!(snapshot.extra_bytes() >= 3 * size_of::<u32>());
    }

    #[test]
    fn nested_spare_capacity_is_charged_even_when_rigs_are_shared() {
        let mut asset = PhysicsAsset::empty("physics".into());
        let mut rig = PhysicsAsset::default_group("rig".into(), "external".into());
        rig.normalization
            .extensions
            .insert("vendor".into(), Value::String("payload".into()));
        asset.data.settings = vec![Arc::new(rig)];
        let before = asset.extra_bytes();

        let rig = Arc::get_mut(&mut asset.data.settings[0]).unwrap();
        let Value::String(payload) = rig.normalization.extensions.get_mut("vendor").unwrap() else {
            unreachable!()
        };
        let old_capacity = payload.capacity();
        payload.reserve_exact(4096);
        let added_capacity = payload.capacity() - old_capacity;
        assert_eq!(asset.extra_bytes(), before + added_capacity);

        let checkpoint = asset.clone();
        assert!(Arc::ptr_eq(
            &asset.data.settings[0],
            &checkpoint.data.settings[0]
        ));
        assert_eq!(checkpoint.extra_bytes(), asset.extra_bytes());
    }
}

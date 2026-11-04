//! Shared typed source relations. Ownership is traversed forward; dependencies
//! are traversed backward. Scope graphs remain separate from dependency edges.
use super::*;
use crate::BindingAxis;
mod business;
use std::sync::Mutex;

#[derive(Debug, Default)]
pub(in crate::document) struct DeletionIndexCache(Mutex<Option<(Arc<()>, Arc<DeletionIndex>)>>);
impl Clone for DeletionIndexCache {
    fn clone(&self) -> Self {
        Self(Mutex::new(self.0.lock().unwrap().clone()))
    }
}
#[derive(Debug, Default)]
pub(super) struct DeletionIndex {
    pub objects: BTreeSet<DeletionObject>,
    pub folders: BTreeMap<String, Vec<DeletionObject>>,
    pub organization: BTreeMap<String, Vec<DeletionObject>>,
    pub deformation: BTreeMap<String, Vec<DeletionObject>>,
    pub owned: BTreeMap<DeletionObject, Vec<DeletionObject>>,
    pub incoming: BTreeMap<DeletionObject, Vec<DeletionRelation>>,
}
impl DeletionIndex {
    fn build(d: &Document, control: &ConversionControl) -> Result<Self, Status> {
        let mut builder = IndexBuilder {
            index: Self::default(),
            count_only: false,
            limit: usize::MAX,
            bytes: 0,
        };
        builder.visit(d, control)?;
        Ok(builder.index)
    }
    pub(super) fn admission_bytes(
        d: &Document,
        control: &ConversionControl,
        limit: usize,
    ) -> Result<usize, Status> {
        // One root allowance for each of the six trees. Other nodes have at
        // least five keys, so charge() can amortize their eleven slots.
        let roots = 6
            * (11 * std::mem::size_of::<(DeletionObject, Vec<DeletionRelation>)>()
                + 12 * std::mem::size_of::<usize>()
                + 64);
        let mut builder = IndexBuilder {
            index: Self::default(),
            count_only: true,
            limit,
            bytes: roots,
        };
        builder.visit(d, control)?;
        builder.checkpoint(control)?;
        Ok(builder.bytes)
    }
}

// Count and build with the same traversal. Admission keeps unique map keys,
// without relationship vectors or geometry, so shared owners count only once.
struct IndexBuilder {
    index: DeletionIndex,
    count_only: bool,
    limit: usize,
    bytes: usize,
}
impl IndexBuilder {
    fn checkpoint(&self, control: &ConversionControl) -> Result<(), Status> {
        control.checkpoint()?;
        if self.bytes > self.limit {
            return Err(Status::error(
                "AUTHORING_MEMORY_LIMIT",
                format!("{} deletion index bytes exceed {}", self.bytes, self.limit),
            ));
        }
        Ok(())
    }
    fn charge<K, V>(&mut self, strings: usize) {
        // Non-root B-tree nodes have at least five occupied keys. Three slots
        // and three pointers per insertion cover their eleven slots and twelve
        // edges. Root spare slots are charged separately by admission_bytes().
        self.bytes = self.bytes.saturating_add(
            3 * std::mem::size_of::<(K, V)>() + 3 * std::mem::size_of::<usize>() + 8 + strings,
        );
    }
    fn strings(key: &DeletionObject) -> usize {
        key.id.capacity() + key.owner_id.as_ref().map_or(0, String::capacity)
    }
    fn record(&mut self, key: DeletionObject) {
        let strings = Self::strings(&key);
        if self.index.objects.insert(key) {
            self.charge::<DeletionObject, ()>(strings);
        }
    }
    fn group(
        &mut self,
        select: fn(&mut DeletionIndex) -> &mut BTreeMap<String, Vec<DeletionObject>>,
        owner: String,
        child: DeletionObject,
    ) {
        if !select(&mut self.index).contains_key(&owner) {
            self.charge::<String, Vec<DeletionObject>>(
                owner.capacity() + 4 * std::mem::size_of::<DeletionObject>(),
            );
        }
        self.bytes = self
            .bytes
            .saturating_add(Self::strings(&child) + 2 * std::mem::size_of::<DeletionObject>());
        let children = select(&mut self.index).entry(owner).or_default();
        if !self.count_only {
            children.push(child);
        }
    }

    fn own(&mut self, owner: DeletionObject, child: DeletionObject) {
        self.record(owner.clone());
        self.record(child.clone());
        if !self.index.owned.contains_key(&owner) {
            self.charge::<DeletionObject, Vec<DeletionObject>>(
                Self::strings(&owner) + 4 * std::mem::size_of::<DeletionObject>(),
            );
        }
        self.bytes = self
            .bytes
            .saturating_add(Self::strings(&child) + 2 * std::mem::size_of::<DeletionObject>());
        let children = self.index.owned.entry(owner).or_default();
        if !self.count_only {
            children.push(child);
        }
    }
    fn edge(&mut self, owner: DeletionObject, field: DeletionField, target: DeletionObject) {
        self.record(owner.clone());
        self.record(target.clone());
        if !self.index.incoming.contains_key(&target) {
            self.charge::<DeletionObject, Vec<DeletionRelation>>(
                Self::strings(&target) + 4 * std::mem::size_of::<DeletionRelation>(),
            );
        }
        self.bytes = self.bytes.saturating_add(
            Self::strings(&owner)
                + Self::strings(&target)
                + 2 * std::mem::size_of::<DeletionRelation>(),
        );
        let incoming = self.index.incoming.entry(target.clone()).or_default();
        if !self.count_only {
            incoming.push(DeletionRelation {
                owner,
                field,
                target,
            });
        }
    }
    fn axes(
        &mut self,
        owner: DeletionObject,
        axes: &[BindingAxis],
        control: &ConversionControl,
    ) -> Result<(), Status> {
        for axis in axes {
            self.checkpoint(control)?;
            let key = object(DeletionKind::Axis, &axis.id);
            self.own(owner.clone(), key.clone());
            if !axis.parameter_id.is_empty() {
                self.edge(
                    key.clone(),
                    DeletionField::ParameterAxis,
                    object(DeletionKind::Parameter, &axis.parameter_id),
                );
            }
            for sample in &axis.samples {
                self.checkpoint(control)?;
                self.own(key.clone(), object(DeletionKind::Sample, &sample.id));
            }
        }
        Ok(())
    }
    fn track(
        &mut self,
        track: &MotionTrack,
        expression: Option<&str>,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        let identity = |kind, id: &str| DeletionObject {
            kind,
            id: id.into(),
            owner_id: expression.map(str::to_owned),
        };
        let owner = identity(
            if expression.is_some() {
                DeletionKind::ExpressionTrack
            } else {
                DeletionKind::MotionTrack
            },
            &track.id,
        );
        if let MotionTrackTarget::PartOpacity { part_id } = &track.target {
            self.edge(
                owner.clone(),
                if expression.is_some() {
                    DeletionField::ExpressionSourcePart
                } else {
                    DeletionField::MotionPart
                },
                object(DeletionKind::Part, part_id),
            );
        }
        if let MotionTrackTarget::Parameter { parameter_id } = &track.target {
            self.edge(
                owner.clone(),
                if expression.is_some() {
                    DeletionField::ExpressionSourceParameter
                } else {
                    DeletionField::MotionParameter
                },
                object(DeletionKind::Parameter, parameter_id),
            );
        }
        for key in track.keys() {
            self.checkpoint(control)?;
            self.own(
                owner.clone(),
                identity(
                    if expression.is_some() {
                        DeletionKind::ExpressionKey
                    } else {
                        DeletionKind::MotionKey
                    },
                    &key.id,
                ),
            );
        }
        Ok(())
    }
    fn visit(&mut self, d: &Document, control: &ConversionControl) -> Result<(), Status> {
        let index = self;
        for id in d.scene_organization_order() {
            index.checkpoint(control)?;
            let Some(key) = d.deletion_scene_object(&id) else {
                continue;
            };
            index.record(key.clone());
            if let Some(parent) = d.scene_organization_owner(&id) {
                index.group(|i| &mut i.organization, parent.into(), key.clone());
            }
            if let Some(parent) = d.scene_deformer_parent(&id) {
                index.group(|i| &mut i.deformation, parent.into(), key);
            }
        }
        for id in d.binding_order() {
            index.checkpoint(control)?;
            let b = d.get_binding(id).unwrap();
            let owner = object(DeletionKind::MeshBinding, id);
            index.own(object(DeletionKind::Mesh, &b.mesh_id), owner.clone());
            index.axes(owner, &b.axes, control)?;
        }
        for id in d.scene_binding_order() {
            index.checkpoint(control)?;
            let b = d.get_scene_binding(id).unwrap();
            let owner = object(DeletionKind::SceneBinding, id);
            index.own(
                d.deletion_scene_object(b.target_id()).unwrap(),
                owner.clone(),
            );
            index.axes(owner, &b.axes, control)?;
        }
        for id in d.blend_binding_order() {
            index.checkpoint(control)?;
            let b = d.get_blend_binding(id).unwrap();
            index.own(
                d.deletion_scene_object(&b.target_id).unwrap(),
                object(DeletionKind::BlendBinding, id),
            );
        }
        for id in d.glue_order() {
            index.checkpoint(control)?;
            let g = d.get_glue(id).unwrap();
            let owner = object(DeletionKind::Glue, id);
            for (endpoint, field) in [
                (&g.mesh_a_id, DeletionField::GlueMeshA),
                (&g.mesh_b_id, DeletionField::GlueMeshB),
            ] {
                if let Some(mesh) = endpoint.resolved_id() {
                    index.edge(owner.clone(), field, object(DeletionKind::Mesh, mesh));
                }
            }
            for pair in &g.pairs {
                index.own(owner.clone(), object(DeletionKind::GluePair, &pair.id));
            }
            if let Some(b) = &g.binding {
                let binding = object(DeletionKind::GlueBinding, id);
                index.own(owner, binding.clone());
                index.axes(binding, &b.axes, control)?;
            }
        }
        for id in d.mesh_order() {
            index.checkpoint(control)?;
            let m = d.get_mesh(id).unwrap();
            for (slot, mask) in m.masks.iter().enumerate() {
                index.edge(
                    object(DeletionKind::Mesh, id),
                    DeletionField::MeshMask { slot },
                    object(DeletionKind::Mesh, mask),
                );
            }
        }
        for id in d.offscreen_order() {
            index.checkpoint(control)?;
            for (slot, mask) in d.get_offscreen(id).unwrap().masks.iter().enumerate() {
                index.edge(
                    object(DeletionKind::Offscreen, id),
                    DeletionField::OffscreenMask { slot },
                    object(DeletionKind::Mesh, mask),
                );
            }
        }
        for area in d.model3_settings().hit_areas.iter().flatten() {
            index.checkpoint(control)?;
            index.record(object(DeletionKind::HitArea, &area.id));
            if let Some(mesh) = area.mesh.object_id() {
                index.edge(
                    object(DeletionKind::HitArea, &area.id),
                    DeletionField::HitAreaMesh,
                    object(DeletionKind::Mesh, mesh),
                );
            }
        }
        for g in d.draw_order_groups().into_iter().flatten() {
            index.checkpoint(control)?;
            let owner = object(
                DeletionKind::DrawingGroup,
                if g.owner.is_empty() { d.id() } else { &g.owner },
            );
            if !g.owner.is_empty() {
                index.own(object(DeletionKind::Part, &g.owner), owner.clone());
            }
            for (slot, id) in g.items.iter().enumerate() {
                if let Some(target) = d.deletion_scene_object(id) {
                    index.edge(owner.clone(), DeletionField::DrawingItem { slot }, target);
                }
            }
        }
        if let Some(pose) = d.pose() {
            index.record(object(DeletionKind::Pose, &pose.id));
            for group in &pose.groups {
                let key = object(DeletionKind::PoseGroup, &group.id);
                index.own(object(DeletionKind::Pose, &pose.id), key.clone());
                for entry in &group.entries {
                    index.own(key.clone(), object(DeletionKind::PoseEntry, &entry.id));
                }
            }
            for entry in pose.groups.iter().flat_map(|g| &g.entries) {
                index.checkpoint(control)?;
                let owner = object(DeletionKind::PoseEntry, &entry.id);
                if let PosePartRef::Resolved { part_id } = &entry.part {
                    index.edge(
                        owner.clone(),
                        DeletionField::PosePart,
                        object(DeletionKind::Part, part_id),
                    );
                }
                for link in entry.links.iter().flatten() {
                    let child = object(DeletionKind::PoseLink, &link.id);
                    index.own(owner.clone(), child.clone());
                    if let PosePartRef::Resolved { part_id } = &link.target {
                        index.edge(
                            child,
                            DeletionField::PoseLinkPart,
                            object(DeletionKind::Part, part_id),
                        );
                    }
                }
            }
        }
        index.business(d, control)?;
        for (slot, entry) in d.display_info().parts.iter().flatten().enumerate() {
            index.checkpoint(control)?;
            if let CdiPartEntry::Resolved { part_id, .. } = entry {
                index.edge(
                    object(DeletionKind::DisplayInfo, d.id()),
                    DeletionField::CdiPart { slot },
                    object(DeletionKind::Part, part_id),
                );
            }
        }
        Ok(())
    }
}
impl Document {
    pub(super) fn deletion_index(
        &self,
        control: &ConversionControl,
    ) -> Result<Arc<DeletionIndex>, Status> {
        control.checkpoint()?;
        let mut cache = self.deletion_index.0.lock().unwrap();
        if let Some((source, index)) = cache
            .as_ref()
            .filter(|(source, _)| Arc::ptr_eq(source, &self.snapshot_identity))
        {
            let _ = source;
            return Ok(index.clone());
        }
        let index = Arc::new(DeletionIndex::build(self, control)?);
        *cache = Some((self.snapshot_identity.clone(), index.clone()));
        Ok(index)
    }
    pub(super) fn indexed_deletion_dependencies(
        &self,
        request: &DeletionRequest,
        removed: &mut BTreeMap<DeletionObject, DeletionReason>,
        report: &mut DeletionReport,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        let index = self.deletion_index(control)?;
        let mut pending: Vec<_> = removed.keys().cloned().collect();
        let mut seen = BTreeSet::new();
        while let Some(key) = pending.pop() {
            control.checkpoint()?;
            if !seen.insert(key.clone()) {
                continue;
            }
            for child in index.owned.get(&key).into_iter().flatten() {
                if !removed.contains_key(child) {
                    pending.push(child.clone());
                }
                add_owned(removed, report, key.clone(), child.clone());
            }
            for relation in index.incoming.get(&key).into_iter().flatten() {
                match relation.field {
                    DeletionField::GlueMeshA
                    | DeletionField::GlueMeshB
                    | DeletionField::HitAreaMesh
                    | DeletionField::PosePart
                    | DeletionField::PoseLinkPart
                    | DeletionField::MotionPart
                    | DeletionField::ExpressionSourcePart
                    | DeletionField::ParameterAxis
                    | DeletionField::BlendTableParameter
                    | DeletionField::BlendConstraintParameter
                    | DeletionField::BlendTable
                    | DeletionField::MotionParameter
                    | DeletionField::ExpressionParameter
                    | DeletionField::ExpressionSourceParameter
                    | DeletionField::PhysicsParameter
                    | DeletionField::ModelParameter
                    | DeletionField::CdiCombinedParameter
                    | DeletionField::MotionClip => {
                        if !removed.contains_key(&relation.owner) {
                            removed.insert(relation.owner.clone(), DeletionReason::Dependency);
                            pending.push(relation.owner.clone());
                        }
                        report.relations.push(relation.clone());
                    }
                    DeletionField::PhysicsNode
                        if !request
                            .physics_node_replacements
                            .contains_key(&relation.target.id) =>
                    {
                        if !removed.contains_key(&relation.owner) {
                            removed.insert(relation.owner.clone(), DeletionReason::Dependency);
                            pending.push(relation.owner.clone());
                        }
                        report.relations.push(relation.clone());
                    }
                    _ => report.rewritten.push(relation.clone()),
                }
            }
        }
        report.rewritten.retain(|r| !removed.contains_key(&r.owner));
        let masks: BTreeSet<_> = report
            .rewritten
            .iter()
            .filter(|r| matches!(r.field, DeletionField::MeshMask { .. }))
            .map(|r| r.owner.id.clone())
            .collect();
        for id in masks {
            let m = self.get_mesh(&id).unwrap();
            if m.inverted_mask
                && !m.masks.is_empty()
                && m.masks
                    .iter()
                    .all(|mask| removed.contains_key(&object(DeletionKind::Mesh, mask)))
            {
                report.rewritten.push(DeletionRelation {
                    owner: object(DeletionKind::Mesh, &id),
                    field: DeletionField::MeshInvertedMask,
                    target: object(DeletionKind::Mesh, &m.masks[0]),
                });
            }
        }
        let part_records: BTreeSet<_> = report
            .relations
            .iter()
            .filter(|r| r.target.kind == DeletionKind::Part)
            .map(|r| r.owner.clone())
            .collect();
        for key in removed.iter().filter(|(k, reason)| {
            **reason != DeletionReason::Owned
                && part_records.contains(*k)
                && matches!(
                    k.kind,
                    DeletionKind::PoseEntry
                        | DeletionKind::PoseLink
                        | DeletionKind::MotionTrack
                        | DeletionKind::ExpressionTrack
                )
        }) {
            report
                .effects
                .push(DeletionEffect::PartAnimationRemoved(key.0.clone()));
        }
        // Traversal order is independent of the explicit root order.
        report
            .rewritten
            .sort_by(|a, b| (&a.owner, &a.target).cmp(&(&b.owner, &b.target)));
        Ok(())
    }
}

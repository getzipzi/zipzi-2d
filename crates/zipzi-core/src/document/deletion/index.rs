//! Shared typed source relations. Ownership is traversed forward; dependencies
//! are traversed backward. Scope graphs remain separate from dependency edges.
use super::*;
use crate::BindingAxis;
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
    pub organization: BTreeMap<String, Vec<DeletionObject>>,
    pub deformation: BTreeMap<String, Vec<DeletionObject>>,
    pub owned: BTreeMap<DeletionObject, Vec<DeletionObject>>,
    pub incoming: BTreeMap<DeletionObject, Vec<DeletionRelation>>,
}
impl DeletionIndex {
    fn own(&mut self, owner: DeletionObject, child: DeletionObject) {
        self.owned.entry(owner).or_default().push(child);
    }
    fn edge(&mut self, owner: DeletionObject, field: DeletionField, target: DeletionObject) {
        self.incoming
            .entry(target.clone())
            .or_default()
            .push(DeletionRelation {
                owner,
                field,
                target,
            });
    }
    fn axes(
        &mut self,
        owner: DeletionObject,
        axes: &[BindingAxis],
        control: &ConversionControl,
    ) -> Result<(), Status> {
        for axis in axes {
            control.checkpoint()?;
            let key = object(DeletionKind::Axis, &axis.id);
            self.own(owner.clone(), key.clone());
            for sample in &axis.samples {
                control.checkpoint()?;
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
        for key in track.keys() {
            control.checkpoint()?;
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
    fn build(d: &Document, control: &ConversionControl) -> Result<Self, Status> {
        let mut index = Self::default();
        for id in d.scene_organization_order() {
            control.checkpoint()?;
            let Some(key) = d.deletion_scene_object(&id) else {
                continue;
            };
            if let Some(parent) = d.scene_organization_owner(&id) {
                index
                    .organization
                    .entry(parent.into())
                    .or_default()
                    .push(key.clone());
            }
            if let Some(parent) = d.scene_deformer_parent(&id) {
                index
                    .deformation
                    .entry(parent.into())
                    .or_default()
                    .push(key);
            }
        }
        for id in d.binding_order() {
            control.checkpoint()?;
            let b = d.get_binding(id).unwrap();
            let owner = object(DeletionKind::MeshBinding, id);
            index.own(object(DeletionKind::Mesh, &b.mesh_id), owner.clone());
            index.axes(owner, &b.axes, control)?;
        }
        for id in d.scene_binding_order() {
            control.checkpoint()?;
            let b = d.get_scene_binding(id).unwrap();
            let owner = object(DeletionKind::SceneBinding, id);
            index.own(
                d.deletion_scene_object(b.target_id()).unwrap(),
                owner.clone(),
            );
            index.axes(owner, &b.axes, control)?;
        }
        for id in d.blend_binding_order() {
            control.checkpoint()?;
            let b = d.get_blend_binding(id).unwrap();
            index.own(
                d.deletion_scene_object(&b.target_id).unwrap(),
                object(DeletionKind::BlendBinding, id),
            );
        }
        for id in d.glue_order() {
            control.checkpoint()?;
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
                index.axes(owner, &b.axes, control)?;
            }
        }
        for id in d.mesh_order() {
            control.checkpoint()?;
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
            control.checkpoint()?;
            for (slot, mask) in d.get_offscreen(id).unwrap().masks.iter().enumerate() {
                index.edge(
                    object(DeletionKind::Offscreen, id),
                    DeletionField::OffscreenMask { slot },
                    object(DeletionKind::Mesh, mask),
                );
            }
        }
        for area in d.model3_settings().hit_areas.iter().flatten() {
            control.checkpoint()?;
            if let Some(mesh) = area.mesh.object_id() {
                index.edge(
                    object(DeletionKind::HitArea, &area.id),
                    DeletionField::HitAreaMesh,
                    object(DeletionKind::Mesh, mesh),
                );
            }
        }
        for g in d.draw_order_groups().into_iter().flatten() {
            control.checkpoint()?;
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
            for entry in pose.groups.iter().flat_map(|g| &g.entries) {
                control.checkpoint()?;
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
        for id in d.motion_order() {
            control.checkpoint()?;
            for track in &d.get_motion(id).unwrap().tracks {
                index.track(track, None, control)?;
            }
        }
        for id in d.expression_order() {
            control.checkpoint()?;
            if let Some(scene) = d.get_expression(id).unwrap().source_scene() {
                for track in &scene.tracks {
                    index.track(track, Some(id), control)?;
                }
            }
        }
        for (slot, entry) in d.display_info().parts.iter().flatten().enumerate() {
            control.checkpoint()?;
            if let CdiPartEntry::Resolved { part_id, .. } = entry {
                index.edge(
                    object(DeletionKind::DisplayInfo, d.id()),
                    DeletionField::CdiPart { slot },
                    object(DeletionKind::Part, part_id),
                );
            }
        }
        Ok(index)
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
                    | DeletionField::ExpressionSourcePart => {
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
        for key in removed.iter().filter(|(k, reason)| {
            **reason != DeletionReason::Owned
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

use super::*;
impl Model<'_> {
    pub(super) fn content_edit_payload(&self, edit: &SceneContentEdit) -> usize {
        use SceneContentEdit as E;
        let owner = match edit {
            E::DeformerOriginal { transform_id, .. } => transform_id,
            E::MeshVertices { mesh_id, .. } => mesh_id,
            E::InsertMeshVertices { mesh_id, .. } => mesh_id,
            E::DeleteMeshVertices { mesh_id, .. } => mesh_id,
            E::ReorderMeshVertices { mesh_id, .. } => mesh_id,
            E::InsertTriangle { mesh_id, .. } => mesh_id,
            E::PatchTriangle { mesh_id, .. } => mesh_id,
            E::DeleteTriangles { mesh_id, .. } => mesh_id,
            E::ReplaceTriangles { mesh_id, .. } => mesh_id,
            E::WarpPoints { transform_id, .. } => transform_id,
            E::WarpBezier { transform_id, .. } => transform_id,
            E::WarpBezierControls { transform_id, .. } => transform_id,
            E::WarpAuthoring { transform_id, .. } => transform_id,
            E::WarpShape { transform_id, .. } => transform_id,
            E::WarpKeepChildren { transform_id, .. } => transform_id,
            E::RotationKeepChildren { transform_id, .. } => transform_id,
            E::ResizeWarp { transform_id, .. } => transform_id,
            E::InsertGluePair { glue_id, .. } => glue_id,
            E::PatchGluePair { glue_id, .. } => glue_id,
            E::MoveGluePairs { glue_id, .. } => glue_id,
            E::TransferGluePairs { glue_id, .. } => glue_id,
            E::DuplicateGluePairs { glue_id, .. } => glue_id,
            E::DeleteGluePairs { glue_id, .. } => glue_id,
            E::InsertOffscreenKeyform { offscreen_id, .. } => offscreen_id,
            E::PatchOffscreenKeyform { offscreen_id, .. } => offscreen_id,
            E::DuplicateOffscreenKeyforms { offscreen_id, .. } => offscreen_id,
            E::ReorderOffscreenKeyforms { offscreen_id, .. } => offscreen_id,
            E::DeleteOffscreenKeyforms { offscreen_id, .. } => offscreen_id,
            E::SetOffscreenMapping { offscreen_id, .. } => offscreen_id,
        };
        let mut ids = HashSet::from([owner.clone()]);
        if matches!(
            edit,
            E::WarpKeepChildren { .. } | E::RotationKeepChildren { .. }
        ) {
            loop {
                let before = ids.len();
                for id in self.transform_order().iter().chain(self.mesh_order()) {
                    if self
                        .scene_deformer_parent(id)
                        .is_some_and(|p| ids.contains(p))
                    {
                        ids.insert(id.clone());
                    }
                }
                if before == ids.len() {
                    break;
                }
            }
        }
        let mut bytes = crate::state::scene_payload(self, &ids);
        if let E::TransferGluePairs { destination, .. } = edit {
            bytes = bytes.saturating_add(crate::state::scene_payload(
                self,
                &HashSet::from([destination.clone()]),
            ));
        }
        if matches!(
            edit,
            E::WarpKeepChildren { .. }
                | E::RotationKeepChildren { .. }
                | E::DeformerOriginal { .. }
        ) {
            // These operations actually evaluate a complete frame for their geometry proof.
            bytes = self.content.meshes.values().fold(bytes, |n, m| {
                n.saturating_add(m.vertex_ids.len().saturating_mul(64))
            });
            bytes = self.content.transforms.values().fold(bytes, |n, t| {
                n.saturating_add(t.warp().map_or(64, |w| w.points.len().saturating_mul(64)))
            });
        }
        bytes
    }
}

//! Physics operations preserve semantic order and jointly validate node references.
use super::*;
use std::sync::Arc;
pub use zipzi_contract::physics_document::PhysicsEdit;

#[derive(Debug, Clone)]
pub struct PhysicsPlan {
    baseline: Arc<()>,
    change: PhysicsPlanChange,
    affected: Vec<String>,
}
#[derive(Debug, Clone)]
enum PhysicsPlanChange {
    Patch(PhysicsPatch),
    Replace(Option<PhysicsAsset>),
}
impl PhysicsPlan {
    /// Field patches never create or retire object identities.
    pub fn preserves_identities(&self) -> bool {
        matches!(self.change, PhysicsPlanChange::Patch(_))
    }
    pub fn affected_ids(&self) -> &[String] {
        &self.affected
    }
}
fn missing(id: &str) -> Status {
    Status::error("MISSING_PHYSICS_OBJECT", id)
}
fn unique(ids: &[String]) -> Result<HashSet<String>, Status> {
    let set: HashSet<_> = ids.iter().cloned().collect();
    if ids.is_empty() || set.len() != ids.len() {
        return Err(Status::error(
            "INVALID_PHYSICS_SELECTION",
            "select distinct objects",
        ));
    }
    Ok(set)
}
fn insert<T>(
    items: &mut Vec<T>,
    mut moved: Vec<T>,
    before: &Option<String>,
    id: impl Fn(&T) -> &str,
) -> Result<(), Status> {
    let index = match before {
        Some(v) => items
            .iter()
            .position(|x| id(x) == v)
            .ok_or_else(|| missing(v))?,
        None => items.len(),
    };
    items.splice(index..index, moved.drain(..));
    Ok(())
}
pub trait PhysicsAssetEditing: Sized {
    fn default_group(id: String, external_id: String) -> PhysicsSetting;
}
trait PhysicsAssetRig {
    fn rig_mut(&mut self, id: &str) -> Result<&mut PhysicsSetting, Status>;
}
impl PhysicsAssetEditing for PhysicsAsset {
    fn default_group(id: String, external_id: String) -> PhysicsSetting {
        let node = |radius| PhysicsNode {
            id: uuid::Uuid::new_v4().to_string(),
            position: crate::physics::PhysicsVector { x: 0., y: radius },
            mobility: 0.95,
            delay: 0.8,
            acceleration: 1.,
            radius,
            extensions: BTreeMap::new(),
        };
        PhysicsSetting {
            id,
            external_id,
            inputs: vec![],
            outputs: vec![],
            vertices: vec![node(0.), node(10.)],
            normalization: crate::physics::PhysicsNormalization {
                position: crate::physics::PhysicsRange {
                    minimum: -10.,
                    default: 0.,
                    maximum: 10.,
                },
                angle: crate::physics::PhysicsRange {
                    minimum: -10.,
                    default: 0.,
                    maximum: 10.,
                },
                extensions: BTreeMap::new(),
            },
            extensions: BTreeMap::new(),
        }
    }
}
impl PhysicsAssetRig for PhysicsAsset {
    fn rig_mut(&mut self, id: &str) -> Result<&mut PhysicsSetting, Status> {
        self.data
            .settings
            .iter_mut()
            .find(|g| g.id == id)
            .map(Arc::make_mut)
            .ok_or_else(|| missing(id))
    }
}
impl Document {
    pub fn physics_unlocked(&self, id: &str) -> Result<(), Status> {
        let mut current = Some(id);
        while let Some(id) = current {
            if self.editor_state().get(id).locked {
                return Err(Status::error("OBJECT_LOCKED", id));
            }
            current = self.physics_owner(id);
        }
        Ok(())
    }
    pub(in crate::document) fn validate_physics_replacement_unlocked(
        &self,
        asset: &PhysicsAsset,
    ) -> Result<(), Status> {
        let Some(old) = self.physics() else {
            return self.physics_unlocked(&asset.id);
        };
        if old == asset {
            return Ok(());
        }
        self.physics_unlocked(&old.id)?;
        for (i, g) in old.data.settings.iter().enumerate() {
            let new = asset
                .data
                .settings
                .iter()
                .enumerate()
                .find(|(_, n)| n.id == g.id);
            if old.id != asset.id
                || new.is_none_or(|(j, n)| i != j || n.as_ref() != g.as_ref())
                || old.group_name(&g.id) != asset.group_name(&g.id)
            {
                self.physics_unlocked(&g.id)?;
                for id in g.entity_ids().skip(1) {
                    if !self.editor_state().get(id).locked {
                        continue;
                    }
                    if old.id != asset.id || new.is_none_or(|(j, _)| i != j) {
                        return Err(Status::error("OBJECT_LOCKED", id));
                    }
                    let unchanged = if let Some((_, n)) = new {
                        g.inputs
                            .iter()
                            .enumerate()
                            .find(|(_, v)| v.id == id)
                            .is_some_and(|(i, v)| n.inputs.get(i) == Some(v))
                            || g.outputs
                                .iter()
                                .enumerate()
                                .find(|(_, v)| v.id == id)
                                .is_some_and(|(i, v)| n.outputs.get(i) == Some(v))
                            || g.vertices
                                .iter()
                                .enumerate()
                                .find(|(_, v)| v.id == id)
                                .is_some_and(|(i, v)| n.vertices.get(i) == Some(v))
                    } else {
                        false
                    };
                    if !unchanged {
                        return Err(Status::error("OBJECT_LOCKED", id));
                    }
                }
            }
        }
        Ok(())
    }
    pub fn plan_physics_edit(&self, intent: PhysicsEdit) -> Result<PhysicsPlan, Status> {
        if self.mutation_blocked() {
            return Err(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return Err(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if let PhysicsEdit::Patch { patch } = intent {
            let status = self.validate_physics_patch(&patch);
            if !status.is_ok() {
                return Err(status);
            }
            let affected = std::iter::once(patch.asset_id.clone())
                .chain(patch.rig.iter().flat_map(|r| {
                    std::iter::once(r.group_id.clone()).chain(
                        r.changes
                            .iter()
                            .filter_map(|p| p.entity_id().map(str::to_owned)),
                    )
                }))
                .collect();
            return Ok(PhysicsPlan {
                baseline: self.snapshot_identity.clone(),
                change: PhysicsPlanChange::Patch(patch),
                affected,
            });
        }
        if let (
            PhysicsEdit::Delete {
                ids,
                node_replacements,
            },
            Some(asset),
        ) = (&intent, self.physics())
        {
            if ids.contains(&asset.id) {
                let selection = unique(ids)?;
                let known: HashSet<_> = asset.entity_ids().map(str::to_owned).collect();
                if !selection.is_subset(&known) || !node_replacements.is_empty() {
                    return Err(Status::error(
                        "INVALID_PHYSICS_SELECTION",
                        "whole asset deletion cannot replace nodes",
                    ));
                }
                for id in asset.entity_ids() {
                    if self.editor_state().get(id).locked {
                        return Err(Status::error("OBJECT_LOCKED", id));
                    }
                }
                return Ok(PhysicsPlan {
                    baseline: self.snapshot_identity.clone(),
                    change: PhysicsPlanChange::Replace(None),
                    affected: known.into_iter().collect(),
                });
            }
        }
        let mut asset = match (&intent, self.physics()) {
            (_, Some(a)) => a.clone(),
            (PhysicsEdit::CreateGroup { asset_id, .. }, None) => {
                PhysicsAsset::empty(asset_id.clone())
            }
            _ => return Err(Status::error("MISSING_PHYSICS", "Create a group first")),
        };
        let duplicate_node = matches!(&intent, PhysicsEdit::DuplicateNode { .. });
        match intent {
            PhysicsEdit::CreateGroup {
                asset_id,
                id,
                name,
                before,
            } => {
                if asset.id != asset_id {
                    return Err(Status::error("PHYSICS_ASSET_CHANGED", asset_id));
                }
                let mut suffix = 1;
                let external = loop {
                    let v = format!("PhysicsSetting{suffix}");
                    if asset.data.settings.iter().all(|g| g.external_id != v) {
                        break v;
                    }
                    suffix += 1;
                };
                let group = Arc::new(PhysicsAsset::default_group(id.clone(), external));
                insert(&mut asset.data.settings, vec![group], &before, |g| &g.id)?;
                asset
                    .data
                    .meta
                    .dictionary
                    .push(crate::physics::PhysicsDictionaryEntry {
                        id,
                        name,
                        extensions: BTreeMap::new(),
                    });
            }
            PhysicsEdit::CreateInput {
                group_id,
                id,
                before,
            } => {
                let g = asset.rig_mut(&group_id)?;
                insert(
                    &mut g.inputs,
                    vec![PhysicsInput {
                        id,
                        source: PhysicsTarget::Unspecified,
                        reference_extensions: BTreeMap::new(),
                        weight: 100.,
                        kind: crate::physics::PhysicsAxis::X,
                        reflect: false,
                        extensions: BTreeMap::new(),
                    }],
                    &before,
                    |v| &v.id,
                )?;
            }
            PhysicsEdit::CreateOutput {
                group_id,
                id,
                node_id,
                before,
            } => {
                let g = asset.rig_mut(&group_id)?;
                insert(
                    &mut g.outputs,
                    vec![PhysicsOutput {
                        id,
                        destination: PhysicsTarget::Unspecified,
                        reference_extensions: BTreeMap::new(),
                        node_id,
                        scale: 1.,
                        weight: 100.,
                        kind: crate::physics::PhysicsAxis::Angle,
                        reflect: false,
                        extensions: BTreeMap::new(),
                    }],
                    &before,
                    |v| &v.id,
                )?;
            }
            PhysicsEdit::InsertNode {
                group_id,
                node,
                before,
            } => {
                let g = asset.rig_mut(&group_id)?;
                if before.as_ref() == g.vertices.first().map(|v| &v.id) {
                    return Err(Status::error(
                        "PHYSICS_ROOT_NODE",
                        "cannot replace chain root",
                    ));
                }
                insert(&mut g.vertices, vec![node], &before, |v| &v.id)?;
            }
            PhysicsEdit::MoveNode { id, before } | PhysicsEdit::DuplicateNode { id, before } => {
                let (source, node) = self.get_physics_node(&id).ok_or_else(|| missing(&id))?;
                self.physics_unlocked(&id)?;
                let duplicate = duplicate_node;
                let group = source.id.clone();
                let mut copy = node.clone();
                if !duplicate && source.vertices[0].id == id {
                    return Err(Status::error("PHYSICS_ROOT_NODE", "root cannot move"));
                }
                if before.as_ref() == source.vertices.first().map(|v| &v.id) {
                    return Err(Status::error(
                        "PHYSICS_ROOT_NODE",
                        "cannot insert before root",
                    ));
                }
                if !duplicate && before.as_ref() == Some(&id) {
                    return Err(Status::error(
                        "INVALID_PLACEMENT",
                        "anchor belongs to selection",
                    ));
                }
                if duplicate {
                    copy.id = uuid::Uuid::new_v4().to_string();
                }
                let g = asset.rig_mut(&group)?;
                if !duplicate {
                    g.vertices.retain(|v| v.id != id);
                }
                insert(&mut g.vertices, vec![copy], &before, |v| &v.id)?;
            }
            PhysicsEdit::MoveInputs {
                ids,
                group_id,
                before,
            } => {
                let ids = unique(&ids)?;
                if before.as_ref().is_some_and(|v| ids.contains(v)) {
                    return Err(Status::error(
                        "INVALID_PLACEMENT",
                        "anchor belongs to selection",
                    ));
                }
                let mut moved = vec![];
                for g in &mut asset.data.settings {
                    if g.inputs.iter().any(|v| ids.contains(&v.id)) {
                        Arc::make_mut(g).inputs.retain(|v| {
                            if ids.contains(&v.id) {
                                moved.push(v.clone());
                                false
                            } else {
                                true
                            }
                        });
                    }
                }
                if moved.len() != ids.len() {
                    return Err(Status::error("MISSING_PHYSICS_INPUT", "selection changed"));
                }
                insert(&mut asset.rig_mut(&group_id)?.inputs, moved, &before, |v| {
                    &v.id
                })?;
            }
            PhysicsEdit::MoveOutputs {
                ids,
                group_id,
                before,
                node_replacements,
            } => {
                let ids = unique(&ids)?;
                if before.as_ref().is_some_and(|v| ids.contains(v)) {
                    return Err(Status::error(
                        "INVALID_PLACEMENT",
                        "anchor belongs to selection",
                    ));
                }
                if node_replacements.keys().any(|id| !ids.contains(id)) {
                    return Err(Status::error(
                        "INVALID_NODE_REPLACEMENT",
                        "replacement must address a selected output",
                    ));
                }
                let mut moved = vec![];
                for g in &mut asset.data.settings {
                    if g.outputs.iter().any(|v| ids.contains(&v.id)) {
                        Arc::make_mut(g).outputs.retain(|v| {
                            if ids.contains(&v.id) {
                                let mut v = v.clone();
                                if let Some(node) = node_replacements.get(&v.id) {
                                    v.node_id = node.clone();
                                }
                                moved.push(v);
                                false
                            } else {
                                true
                            }
                        });
                    }
                }
                if moved.len() != ids.len() {
                    return Err(Status::error("MISSING_PHYSICS_OUTPUT", "selection changed"));
                }
                insert(
                    &mut asset.rig_mut(&group_id)?.outputs,
                    moved,
                    &before,
                    |v| &v.id,
                )?;
            }
            PhysicsEdit::ReorderGroups { ids, before } => {
                let ids = unique(&ids)?;
                if before.as_ref().is_some_and(|v| ids.contains(v)) {
                    return Err(Status::error(
                        "INVALID_PLACEMENT",
                        "anchor belongs to selection",
                    ));
                }
                let mut moved = vec![];
                asset.data.settings.retain(|v| {
                    if ids.contains(&v.id) {
                        moved.push(v.clone());
                        false
                    } else {
                        true
                    }
                });
                if moved.len() != ids.len() {
                    return Err(Status::error("MISSING_PHYSICS_GROUP", "selection changed"));
                }
                insert(&mut asset.data.settings, moved, &before, |v| &v.id)?;
            }
            PhysicsEdit::Duplicate {
                ids,
                group_id,
                before,
            } => {
                let ids = unique(&ids)?;
                // Group copies own their channels and chain; external parameter targets stay shared.
                let groups: Vec<_> = asset
                    .data
                    .settings
                    .iter()
                    .filter(|g| ids.contains(&g.id))
                    .cloned()
                    .collect();
                let mut found = HashSet::new();
                if !groups.is_empty() {
                    if group_id.is_some() {
                        return Err(Status::error(
                            "PHYSICS_GROUPS_ARE_FLAT",
                            "groups cannot nest",
                        ));
                    }
                    let mut copies = vec![];
                    for source in groups {
                        self.physics_unlocked(&source.id)?;
                        for id in source.entity_ids() {
                            self.physics_unlocked(id)?;
                            if ids.contains(id) {
                                found.insert(id.to_owned());
                            }
                        }
                        let mut g = source.as_ref().clone();
                        g.id = uuid::Uuid::new_v4().to_string();
                        let mut suffix = 1;
                        g.external_id = loop {
                            let v = format!("{}_copy{suffix}", source.external_id);
                            if asset.data.settings.iter().all(|g| g.external_id != v) {
                                break v;
                            }
                            suffix += 1;
                        };
                        let mut nodes = BTreeMap::new();
                        for v in &mut g.vertices {
                            let id = uuid::Uuid::new_v4().to_string();
                            nodes.insert(v.id.clone(), id.clone());
                            v.id = id;
                        }
                        for v in &mut g.inputs {
                            v.id = uuid::Uuid::new_v4().to_string();
                        }
                        for v in &mut g.outputs {
                            v.id = uuid::Uuid::new_v4().to_string();
                            v.node_id = nodes[&v.node_id].clone();
                        }
                        let mut d = asset
                            .data
                            .meta
                            .dictionary
                            .iter()
                            .find(|d| d.id == source.id)
                            .unwrap()
                            .clone();
                        d.id = g.id.clone();
                        asset.data.meta.dictionary.push(d);
                        copies.push(Arc::new(g));
                    }
                    insert(&mut asset.data.settings, copies, &before, |g| &g.id)?;
                } else {
                    let target = group_id.ok_or_else(|| {
                        Status::error("MISSING_PHYSICS_GROUP", "copy destination required")
                    })?;
                    let mut inputs = vec![];
                    let mut outputs = vec![];
                    for g in &asset.data.settings {
                        for v in &g.inputs {
                            if ids.contains(&v.id) {
                                self.physics_unlocked(&v.id)?;
                                found.insert(v.id.clone());
                                let mut v = v.clone();
                                v.id = uuid::Uuid::new_v4().to_string();
                                inputs.push(v);
                            }
                        }
                        for v in &g.outputs {
                            if ids.contains(&v.id) {
                                self.physics_unlocked(&v.id)?;
                                found.insert(v.id.clone());
                                let mut v = v.clone();
                                v.id = uuid::Uuid::new_v4().to_string();
                                outputs.push(v);
                            }
                        }
                    }
                    if !inputs.is_empty() && !outputs.is_empty() {
                        return Err(Status::error(
                            "MIXED_PHYSICS_COLLECTION",
                            "copy one channel type at a time",
                        ));
                    }
                    let g = asset.rig_mut(&target)?;
                    if !inputs.is_empty() {
                        insert(&mut g.inputs, inputs, &before, |v| &v.id)?;
                    } else {
                        insert(&mut g.outputs, outputs, &before, |v| &v.id)?;
                    }
                }
                if found != ids {
                    return Err(Status::error(
                        "UNSUPPORTED_PHYSICS_COPY",
                        "nodes require chain operations",
                    ));
                }
            }
            PhysicsEdit::Delete {
                ids,
                node_replacements,
            } => {
                let ids = unique(&ids)?;
                let known: HashSet<_> = asset.entity_ids().map(str::to_owned).collect();
                if !ids.is_subset(&known) {
                    return Err(Status::error("MISSING_PHYSICS_OBJECT", "selection changed"));
                }
                if ids.contains(&asset.id) {
                    return Err(Status::error(
                        "PHYSICS_SINGLETON",
                        "delete groups and channels explicitly",
                    ));
                }
                for (id, replacement) in &node_replacements {
                    if !ids.contains(id)
                        || ids.contains(replacement)
                        || self.get_physics_node(id).is_none()
                    {
                        return Err(Status::error("INVALID_NODE_REPLACEMENT", id));
                    }
                }
                asset.data.settings.retain(|g| !ids.contains(&g.id));
                let retained: HashSet<_> =
                    asset.data.settings.iter().map(|g| g.id.clone()).collect();
                asset
                    .data
                    .meta
                    .dictionary
                    .retain(|d| retained.contains(&d.id));
                for g in &mut asset.data.settings {
                    if !g.entity_ids().any(|id| ids.contains(id)) {
                        continue;
                    }
                    let g = Arc::make_mut(g);
                    if g.vertices.first().is_some_and(|v| ids.contains(&v.id)) {
                        return Err(Status::error("PHYSICS_ROOT_NODE", "root cannot be deleted"));
                    }
                    g.inputs.retain(|v| !ids.contains(&v.id));
                    g.outputs.retain(|v| !ids.contains(&v.id));
                    for v in &mut g.outputs {
                        if ids.contains(&v.node_id) {
                            v.node_id = node_replacements
                                .get(&v.node_id)
                                .cloned()
                                .ok_or_else(|| Status::error("PHYSICS_NODE_IN_USE", &v.node_id))?;
                        }
                    }
                    let previous_node_count = g.vertices.len();
                    g.vertices.retain(|v| !ids.contains(&v.id));
                    if g.vertices.len() < previous_node_count && g.vertices.len() < 2 {
                        return Err(Status::error(
                            "PHYSICS_CHAIN_TOO_SHORT",
                            "keep root and one child",
                        ));
                    }
                }
            }
            PhysicsEdit::Patch { .. } => unreachable!(),
        }
        asset.reconcile_counts();
        self.validate_physics_replacement_unlocked(&asset)?;
        let status = self.validate_physics(&asset, true);
        if !status.is_ok() {
            return Err(status);
        }
        let affected = asset
            .entity_ids()
            .chain(self.physics().into_iter().flat_map(|a| a.entity_ids()))
            .map(str::to_owned)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok(PhysicsPlan {
            baseline: self.snapshot_identity.clone(),
            change: PhysicsPlanChange::Replace(Some(asset)),
            affected,
        })
    }
    pub fn apply_physics_plan(&mut self, plan: &PhysicsPlan) -> EditResult {
        if !Arc::ptr_eq(&self.snapshot_identity, &plan.baseline) {
            return self.failed(Status::error("STALE_PLAN", "physics baseline changed"));
        }
        match &plan.change {
            PhysicsPlanChange::Patch(patch) => return self.patch_physics(patch),
            PhysicsPlanChange::Replace(Some(asset)) => return self.set_physics(asset.clone()),
            PhysicsPlanChange::Replace(None) => {}
        }
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        let Some(asset) = self.physics() else {
            return self.failed(Status::ok());
        };
        for id in asset.entity_ids() {
            if self.editor_state().get(id).locked {
                return self.failed(Status::error("OBJECT_LOCKED", id));
            }
        }
        let ids: Vec<_> = asset.entity_ids().map(str::to_owned).collect();
        for id in &ids {
            self.content.editor_state.objects.remove(id);
        }
        self.content.physics = None;
        let result = self.changed(ChangeKind::Metadata, vec![], ids);
        self.evaluation_revision = self.revision;
        result
    }
}

//! Texture records are authoring objects; content is immutable and optional.
use super::*;
use crate::{ImageAsset, ImageContent, SceneReference};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum ResourceEdit {
    Create {
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    PatchName {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Exact current field value used as an optimistic precondition; stale values reject
        /// the edit.
        expected: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
    },
    SetContent {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Exact current image-content metadata precondition; None requires the asset to have
        /// no loaded content.
        expected: Option<ImageContent>,
        /// Image content metadata; None explicitly represents an asset with no loaded image
        /// content.
        content: Option<ImageContent>,
    },
    Reorder {
        /// Ordered stable object IDs selected for this operation; visible names and row numbers
        /// are not identities.
        ids: Vec<String>,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    Duplicate {
        /// Stable ID of the existing source object to duplicate, not a filesystem path.
        source: String,
        /// New stable UUID for this object; it must not conflict with an existing identity.
        id: String,
        /// Human-readable display name; use the stable ID for identity because names can change.
        name: String,
        /// Optional sibling ID before which to insert; None appends. The anchor must belong to
        /// the selected destination.
        before: Option<String>,
    },
    /// None rejects references; Some explicitly redirects or detaches every user.
    Delete {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// None rejects deletion while referenced; Some explicitly redirects users to a
        /// resolved target or an unresolved/detached reference.
        replacement: Option<SceneReference>,
    },
}
#[derive(Debug, Clone)]
pub struct ResourcePlan {
    snapshot: Arc<()>,
    revision: u64,
    assets: HashMap<String, ImageAsset>,
    order: Vec<String>,
    meshes: Vec<Mesh>,
    affected: Vec<String>,
    copied: BTreeMap<String, String>,
    runtime: bool,
    noop: bool,
}
impl ResourcePlan {
    pub fn is_noop(&self) -> bool {
        self.noop
    }
    pub fn affected_ids(&self) -> &[String] {
        &self.affected
    }
    pub fn copied_ids(&self) -> &BTreeMap<String, String> {
        &self.copied
    }
    pub fn drawing_changed(&self) -> bool {
        self.runtime
    }
}
impl Document {
    pub fn resource_unlocked(&self, id: &str) -> Result<(), Status> {
        if self.get_asset(id).is_none() {
            return Err(Status::error("MISSING_ASSET", id));
        }
        self.authoring_unlocked(id)
    }
    pub fn plan_resource_edit(&self, edit: ResourceEdit) -> Result<ResourcePlan, Status> {
        if !self.initialized() {
            return Err(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        let mut p = ResourcePlan {
            snapshot: self.snapshot_identity.clone(),
            revision: self.revision(),
            assets: self.content.assets.clone(),
            order: self.asset_order().to_vec(),
            meshes: vec![],
            affected: vec![],
            copied: BTreeMap::new(),
            runtime: false,
            noop: true,
        };
        let fresh = |id: &str| {
            if !valid_uuid(id) {
                Err(Status::error("INVALID_ID", id))
            } else if self.contains_id(id) {
                Err(Status::error("DUPLICATE_ID", id))
            } else {
                Ok(())
            }
        };
        let name = |value: String| -> Result<String, Status> {
            if value.contains('\0') {
                return Err(Status::error("INVALID_NAME", value));
            }
            let base = if value.is_empty() {
                "Texture".into()
            } else {
                value
            };
            let mut out = base.clone();
            let mut n = 2;
            while self.content.assets.values().any(|a| a.name == out) {
                out = format!("{base} ({n})");
                n += 1;
            }
            Ok(out)
        };
        let insert =
            |order: &mut Vec<String>, id: String, before: Option<String>| -> Result<(), Status> {
                let at = match before {
                    Some(anchor) => order
                        .iter()
                        .position(|s| s == &anchor)
                        .ok_or_else(|| Status::error("INVALID_PLACEMENT", anchor))?,
                    None => order.len(),
                };
                order.insert(at, id);
                Ok(())
            };
        match edit {
            ResourceEdit::Create {
                id,
                name: label,
                before,
            } => {
                fresh(&id)?;
                p.assets.insert(
                    id.clone(),
                    ImageAsset {
                        unresolved_source: None,
                        id: id.clone(),
                        name: name(label)?,
                        content: None,
                    },
                );
                insert(&mut p.order, id.clone(), before)?;
                p.affected.push(id);
            }
            ResourceEdit::PatchName { id, expected, name } => {
                self.resource_unlocked(&id)?;
                if self.get_asset(&id).unwrap().name != expected {
                    return Err(Status::error("STALE_FIELD", id));
                }
                if name.is_empty() || name.contains('\0') {
                    return Err(Status::error("INVALID_NAME", id));
                }
                p.assets.get_mut(&id).unwrap().name = name;
                p.affected.push(id);
            }
            ResourceEdit::SetContent {
                id,
                expected,
                content,
            } => {
                self.resource_unlocked(&id)?;
                if self.get_asset(&id).unwrap().content != expected {
                    return Err(Status::error("STALE_CONTENT", id));
                }
                if content.as_ref().is_some_and(|c| !c.is_valid()) {
                    return Err(Status::error("INVALID_ASSET", id));
                }
                p.runtime = expected != content;
                let asset = p.assets.get_mut(&id).unwrap();
                asset.content = content;
                asset.unresolved_source = None;
                p.affected.push(id);
            }
            ResourceEdit::Duplicate {
                source,
                id,
                name: label,
                before,
            } => {
                fresh(&id)?;
                self.resource_unlocked(&source)?;
                let mut asset = self.get_asset(&source).unwrap().clone();
                asset.id = id.clone();
                asset.name = name(if label.is_empty() {
                    asset.name.clone()
                } else {
                    label
                })?;
                p.assets.insert(id.clone(), asset);
                insert(&mut p.order, id.clone(), before)?;
                p.copied.insert(source.clone(), id.clone());
                p.affected.extend([source, id]);
            }
            ResourceEdit::Reorder { ids, before } => {
                if ids.iter().collect::<HashSet<_>>().len() != ids.len() {
                    return Err(Status::error("DUPLICATE_SELECTION", "Repeated texture"));
                }
                for id in &ids {
                    self.resource_unlocked(id)?;
                }
                if let Some(anchor) = &before {
                    if !p.order.contains(anchor) {
                        return Err(Status::error("INVALID_PLACEMENT", anchor));
                    }
                    self.resource_unlocked(anchor)?;
                }
                if !before.as_ref().is_some_and(|id| ids.contains(id)) {
                    let selected: Vec<_> = p
                        .order
                        .iter()
                        .filter(|id| ids.contains(id))
                        .cloned()
                        .collect();
                    p.order.retain(|id| !ids.contains(id));
                    for id in selected {
                        insert(&mut p.order, id, before.clone())?;
                    }
                }
                p.affected = ids;
            }
            ResourceEdit::Delete { id, replacement } => {
                self.resource_unlocked(&id)?;
                let users = self.references_to(&id);
                if !users.is_empty() && replacement.is_none() {
                    return Err(Status::error("OBJECT_REFERENCED", users.join(", ")));
                }
                if let Some(target) = &replacement {
                    if target.resolved_id() == Some(id.as_str()) {
                        return Err(Status::error("SELF_REFERENCE", &id));
                    }
                    if let Some(target) = target.resolved_id() {
                        if self.get_asset(target).is_none() {
                            return Err(Status::error("MISSING_ASSET", target));
                        }
                    }
                    for mesh_id in users {
                        self.authoring_unlocked(&mesh_id)?;
                        let mut mesh = self
                            .get_mesh(&mesh_id)
                            .ok_or_else(|| Status::error("UNSUPPORTED_REFERENCE", &mesh_id))?
                            .clone();
                        mesh.texture_asset_id = target.clone();
                        p.meshes.push(mesh);
                        p.affected.push(mesh_id);
                    }
                }
                p.assets.remove(&id);
                p.order.retain(|v| v != &id);
                p.affected.push(id);
                p.runtime = !p.meshes.is_empty();
            }
        }
        p.noop = p.assets == self.content.assets
            && p.order == self.content.asset_order
            && p.meshes.is_empty();
        if !p.noop {
            let mut candidate = self.fork_candidate();
            p.install(&mut candidate);
            if let Some(issue) = candidate.validate_structure().into_iter().next() {
                return Err(issue.status);
            }
            if p.runtime {
                let old = self.scene_projection();
                let next = candidate.scene_projection();
                for id in self.mesh_order() {
                    if old.drawing_slots.contains_key(id) != next.drawing_slots.contains_key(id) {
                        p.affected.push(id.clone());
                    }
                }
                for id in self.offscreen_order() {
                    if old.composite_slots.contains_key(id) != next.composite_slots.contains_key(id)
                    {
                        p.affected.push(id.clone());
                    }
                }
            }
        }
        Ok(p)
    }
    pub fn apply_resource_plan(&mut self, p: &ResourcePlan) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Finish the active edit",
            ));
        }
        if self.revision() != p.revision || !Arc::ptr_eq(&self.snapshot_identity, &p.snapshot) {
            return self.failed(Status::error("STALE_PLAN", "Replan resources"));
        }
        if p.noop {
            return self.failed(Status::ok());
        }
        p.install(self);
        self.changed(
            if p.runtime {
                ChangeKind::Resources
            } else {
                ChangeKind::Metadata
            },
            if p.runtime {
                self.mesh_order().to_vec()
            } else {
                vec![]
            },
            p.affected.clone(),
        )
    }
}
impl ResourcePlan {
    fn install(&self, document: &mut Document) {
        for id in document.asset_order().to_vec() {
            if !self.assets.contains_key(&id) {
                document.content.editor_state.objects.remove(&id);
            }
        }
        document.content.assets = self.assets.clone();
        document.content.asset_order = self.order.clone();
        for mesh in &self.meshes {
            document
                .content
                .meshes
                .insert(mesh.id.clone(), mesh.clone());
        }
        document.scene_projection.take();
    }
}

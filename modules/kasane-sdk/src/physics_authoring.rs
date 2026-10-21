//! Typed Physics intents, field addresses and export plans share SDK history.
use crate::{
    AuthoringSession, CapabilityState, EditReceipt, EditSession, ObjectHandle, ObjectKind,
    SdkError, Version,
};
use kasane_core::document::*;
#[derive(Debug, Clone)]
pub struct PhysicsEditPlan {
    version: Version,
    domain: PhysicsPlan,
}
impl PhysicsEditPlan {
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn affected_ids(&self) -> &[String] {
        self.domain.affected_ids()
    }
}
#[derive(Debug, Clone)]
pub struct PhysicsCapabilities {
    pub inspect: CapabilityState,
    pub patch: CapabilityState,
    pub create_child: CapabilityState,
    pub duplicate: CapabilityState,
    pub delete: CapabilityState,
    pub move_object: CapabilityState,
    pub reorder: CapabilityState,
    pub repair: CapabilityState,
    pub preview: CapabilityState,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhysicsCollection {
    Groups,
    Inputs { group_id: String },
    Outputs { group_id: String },
    Nodes { group_id: String },
}
#[derive(Debug, Clone, PartialEq)]
enum FieldBaseline {
    Input(PhysicsInput),
    Output(PhysicsOutput),
    Node(PhysicsNode),
}
#[derive(Debug, Clone)]
pub struct PhysicsFieldAddress {
    entity: ObjectHandle,
    owner: ObjectHandle,
    epoch: u64,
    expected: FieldBaseline,
}
impl PhysicsFieldAddress {
    pub fn id(&self) -> &str {
        self.entity.id()
    }
    pub fn owner_id(&self) -> &str {
        self.owner.id()
    }
}
impl AuthoringSession {
    pub fn plan_physics_edit(&self, intent: PhysicsEdit) -> Result<PhysicsEditPlan, SdkError> {
        let selected = match &intent {
            PhysicsEdit::Delete { ids, .. } => ids.clone(),
            _ => vec![],
        };
        let domain = self
            .document()
            .plan_physics_edit(intent)
            .map_err(|s| physics_error(self.document(), s, "plan_physics_edit", &selected))?;
        Ok(PhysicsEditPlan {
            version: self.version(),
            domain,
        })
    }
    pub fn execute_physics_plan(
        &mut self,
        plan: &PhysicsEditPlan,
    ) -> Result<EditReceipt, SdkError> {
        let (_, receipt) = self.edit("Edit physics", Some(plan.version), |e| {
            e.apply_physics_plan(plan)
        })?;
        Ok(receipt)
    }
    pub fn physics_collection(
        &self,
        collection: PhysicsCollection,
    ) -> Result<Vec<ObjectHandle>, SdkError> {
        let doc = self.document();
        let asset = doc.physics().ok_or_else(|| {
            SdkError::new(
                "MISSING_PHYSICS",
                "create a group first",
                "physics_collection",
            )
        })?;
        let (kind, ids) = match collection {
            PhysicsCollection::Groups => (
                ObjectKind::PhysicsGroup,
                asset
                    .data
                    .settings
                    .iter()
                    .map(|g| g.id.as_str())
                    .collect::<Vec<_>>(),
            ),
            PhysicsCollection::Inputs { ref group_id }
            | PhysicsCollection::Outputs { ref group_id }
            | PhysicsCollection::Nodes { ref group_id } => {
                let g = doc.get_physics_group(group_id).ok_or_else(|| {
                    SdkError::new("MISSING_PHYSICS_GROUP", group_id, "physics_collection")
                })?;
                match collection {
                    PhysicsCollection::Inputs { .. } => (
                        ObjectKind::PhysicsInput,
                        g.inputs.iter().map(|v| v.id.as_str()).collect(),
                    ),
                    PhysicsCollection::Outputs { .. } => (
                        ObjectKind::PhysicsOutput,
                        g.outputs.iter().map(|v| v.id.as_str()).collect(),
                    ),
                    _ => (
                        ObjectKind::PhysicsNode,
                        g.vertices.iter().map(|v| v.id.as_str()).collect(),
                    ),
                }
            }
        };
        ids.into_iter().map(|id| self.handle(kind, id)).collect()
    }
    pub fn physics_capabilities(
        &self,
        handle: &ObjectHandle,
    ) -> Result<PhysicsCapabilities, SdkError> {
        self.resolve_handle(handle)?;
        let doc = self.document();
        let kind = handle.kind();
        let supported = matches!(
            kind,
            ObjectKind::Physics
                | ObjectKind::PhysicsGroup
                | ObjectKind::PhysicsInput
                | ObjectKind::PhysicsOutput
                | ObjectKind::PhysicsNode
        );
        let unsupported = |code| CapabilityState::Unsupported { code };
        let state = if !supported {
            unsupported("WRONG_OBJECT_KIND")
        } else if doc.physics_unlocked(handle.id()).is_err() {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        let descendants_locked = doc.physics().is_some_and(|a| {
            a.data
                .settings
                .iter()
                .filter(|g| kind == ObjectKind::Physics || g.id == handle.id())
                .flat_map(|g| g.entity_ids())
                .any(|id| doc.editor_state().get(id).locked)
        });
        let subtree = if supported && descendants_locked {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            state.clone()
        };
        let node = kind == ObjectKind::PhysicsNode;
        let channel = matches!(kind, ObjectKind::PhysicsInput | ObjectKind::PhysicsOutput);
        let root_node = doc
            .get_physics_node(handle.id())
            .is_some_and(|(g, _)| g.vertices[0].id == handle.id());
        let chain_too_short = doc
            .get_physics_node(handle.id())
            .is_some_and(|(g, _)| g.vertices.len() <= 2);
        let node_in_use = doc
            .get_physics_node(handle.id())
            .is_some_and(|(g, _)| g.outputs.iter().any(|v| v.node_id == handle.id()));
        Ok(PhysicsCapabilities {
            inspect: CapabilityState::Available,
            patch: state.clone(),
            create_child: if matches!(kind, ObjectKind::Physics | ObjectKind::PhysicsGroup) {
                state.clone()
            } else {
                unsupported("NO_CHILD_COLLECTION")
            },
            duplicate: if kind == ObjectKind::Physics {
                unsupported("USE_CHAIN_OR_GROUP_COPY")
            } else {
                subtree.clone()
            },
            delete: if root_node {
                unsupported("PHYSICS_ROOT_NODE")
            } else if chain_too_short {
                CapabilityState::Unavailable {
                    code: "PHYSICS_CHAIN_TOO_SHORT",
                }
            } else if node_in_use {
                CapabilityState::Unavailable {
                    code: "PHYSICS_NODE_IN_USE",
                }
            } else {
                subtree.clone()
            },
            move_object: if channel {
                state.clone()
            } else {
                unsupported("USE_SEMANTIC_CHAIN_OR_EXECUTION_ORDER")
            },
            reorder: if root_node {
                unsupported("PHYSICS_ROOT_NODE")
            } else if kind == ObjectKind::PhysicsGroup {
                subtree
            } else if channel || node {
                state.clone()
            } else {
                unsupported("USE_CHAIN_OPERATION")
            },
            repair: if channel {
                state.clone()
            } else {
                unsupported("NO_REFERENCE_SLOT")
            },
            preview: if kind == ObjectKind::Physics || kind == ObjectKind::PhysicsGroup {
                CapabilityState::Available
            } else {
                unsupported("PREVIEW_PHYSICS_OWNER")
            },
        })
    }
    fn physics_baseline(&self, id: &str) -> Option<(ObjectKind, &str, FieldBaseline)> {
        let d = self.document();
        d.get_physics_input(id)
            .map(|(g, v)| {
                (
                    ObjectKind::PhysicsInput,
                    g.id.as_str(),
                    FieldBaseline::Input(v.clone()),
                )
            })
            .or_else(|| {
                d.get_physics_output(id).map(|(g, v)| {
                    (
                        ObjectKind::PhysicsOutput,
                        g.id.as_str(),
                        FieldBaseline::Output(v.clone()),
                    )
                })
            })
            .or_else(|| {
                d.get_physics_node(id).map(|(g, v)| {
                    (
                        ObjectKind::PhysicsNode,
                        g.id.as_str(),
                        FieldBaseline::Node(v.clone()),
                    )
                })
            })
    }
    pub fn physics_field_address(&self, id: &str) -> Result<PhysicsFieldAddress, SdkError> {
        let (kind, owner, expected) = self
            .physics_baseline(id)
            .ok_or_else(|| SdkError::new("MISSING_PHYSICS_OBJECT", id, "physics_field_address"))?;
        Ok(PhysicsFieldAddress {
            entity: self.handle(kind, id)?,
            owner: self.handle(ObjectKind::PhysicsGroup, owner)?,
            epoch: self.physics_contexts.get(id).map_or(0, |v| v.1),
            expected,
        })
    }
    pub fn plan_physics_field_patch(
        &self,
        address: &PhysicsFieldAddress,
        changes: Vec<PhysicsProperty>,
    ) -> Result<PhysicsEditPlan, SdkError> {
        self.resolve_handle(&address.entity)?;
        self.resolve_handle(&address.owner)?;
        let current = self.physics_baseline(address.id());
        if self.physics_contexts.get(address.id()).map_or(0, |v| v.1) != address.epoch
            || current.is_none_or(|(_, owner, value)| {
                owner != address.owner.id() || value != address.expected
            })
        {
            return Err(SdkError::new(
                "STALE_FIELD_ADDRESS",
                "capture a new physics address",
                "plan_physics_field_patch",
            ));
        }
        if changes.iter().any(|p| p.entity_id() != Some(address.id())) {
            return Err(SdkError::new(
                "WRONG_FIELD_ADDRESS",
                "patch targets another entity",
                "plan_physics_field_patch",
            ));
        }
        self.plan_physics_edit(PhysicsEdit::Patch {
            patch: PhysicsPatch {
                asset_id: self.document().physics().unwrap().id.clone(),
                asset_changes: vec![],
                rig: Some(PhysicsRigPatch {
                    group_id: address.owner.id().into(),
                    changes,
                }),
            },
        })
    }
}
impl EditSession<'_> {
    pub fn apply_physics_plan(&mut self, plan: &PhysicsEditPlan) -> Result<(), SdkError> {
        self.ensure_active("apply_physics_plan")?;
        if self.base_version() != plan.version {
            return Err(self.abort_with(SdkError::new(
                "STALE_PLAN",
                "replan against current session",
                "apply_physics_plan",
            )));
        }
        let before = (!plan.domain.preserves_identities())
            .then(|| crate::session::keys_in(self.candidate_document()));
        let result = self.document().apply_physics_plan(&plan.domain);
        self.record(result, "apply_physics_plan", "")?;
        if let Some(before) = before {
            self.track_removed_objects(before);
        }
        Ok(())
    }
    pub fn edit_physics(&mut self, intent: PhysicsEdit) -> Result<(), SdkError> {
        self.ensure_active("edit_physics")?;
        if let PhysicsEdit::Patch { patch } = intent {
            return self.patch_physics(&patch);
        }
        let selected = match &intent {
            PhysicsEdit::Delete { ids, .. } => ids.clone(),
            _ => vec![],
        };
        let plan = self
            .candidate_document()
            .plan_physics_edit(intent)
            .map_err(|s| {
                let error = physics_error(self.candidate_document(), s, "edit_physics", &selected);
                self.abort_with(error)
            })?;
        let before = crate::session::keys_in(self.candidate_document());
        let result = self.document().apply_physics_plan(&plan);
        self.record(result, "edit_physics", "")?;
        self.track_removed_objects(before);
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct PhysicsExportPlan {
    version: Version,
    pub omitted: Vec<DisabledPhysicsChannel>,
}
impl AuthoringSession {
    pub fn plan_available_physics_export(&self) -> Result<PhysicsExportPlan, SdkError> {
        let omitted = self
            .document()
            .physics_projection()
            .map_err(|s| SdkError::from_status(s, "plan_available_physics_export", vec![]))?
            .map_or_else(Vec::new, |p| p.disabled);
        Ok(PhysicsExportPlan {
            version: self.version(),
            omitted,
        })
    }
    pub fn export_physics_plan(
        &self,
        plan: &PhysicsExportPlan,
    ) -> Result<Option<String>, SdkError> {
        if self.version() != plan.version {
            return Err(SdkError::new(
                "STALE_PLAN",
                "replan export omissions",
                "export_physics_plan",
            ));
        }
        kasane_project::export_available_physics3(self.document())
            .map_err(|e| SdkError::new(&e.code, &e.message, "export_physics_plan"))
    }
}

fn physics_error(
    document: &kasane_core::Document,
    status: kasane_core::Status,
    operation: &'static str,
    selected: &[String],
) -> SdkError {
    let users = if status.code == "PHYSICS_NODE_IN_USE" {
        document
            .physics_node_users(&status.message)
            .into_iter()
            .filter(|id| {
                !selected.contains(id)
                    && document
                        .get_physics_output(id)
                        .is_none_or(|(g, _)| !selected.contains(&g.id))
            })
            .collect()
    } else {
        vec![]
    };
    let mut error = SdkError::from_status(status, operation, vec![]);
    error.referrers = users.into();
    error
}

//! Immutable capability queries use stable IDs in this captured content. They
//! retain no handles, epochs or mutable Session authority. Commands revalidate
//! domain rules inside the Draft that actually publishes.
use super::*;
use zipzi_contract::authoring::*;
impl DocumentState {
    pub fn expression_capabilities(&self, id: &str) -> Result<ExpressionCapabilities, Status> {
        let doc = self.content.model();
        let kind = if doc.get_expression(id).is_some() {
            ObjectKind::Expression
        } else if doc.get_expression_entry(id).is_some() {
            ObjectKind::ExpressionEntry
        } else {
            return Err(Status::error("MISSING_OBJECT", id));
        };

        let supported = matches!(kind, ObjectKind::Expression | ObjectKind::ExpressionEntry);
        let owner = doc
            .get_expression_entry(id)
            .map(|(owner, _)| owner.id.as_str());
        let locked = doc.editor_state().get(id).locked
            || owner.is_some_and(|id| doc.editor_state().get(id).locked);
        let state = if !supported {
            CapabilityState::Unsupported {
                code: "WRONG_OBJECT_KIND",
            }
        } else if locked {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        let owned_locked = doc.get_expression(id).is_some_and(|owner| {
            owner
                .entries
                .iter()
                .any(|e| doc.editor_state().get(&e.id).locked)
        });
        let subtree_state = if owned_locked {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            state.clone()
        };
        let entry = kind == ObjectKind::ExpressionEntry;
        Ok(ExpressionCapabilities {
            inspect: CapabilityState::Available,
            create_entry: if !entry {
                state.clone()
            } else {
                CapabilityState::Unsupported {
                    code: "ENTRY_CANNOT_OWN_ENTRIES",
                }
            },
            patch: state.clone(),
            move_entry: if entry {
                state.clone()
            } else {
                CapabilityState::Unsupported {
                    code: "EXPRESSION_HAS_NO_OWNER",
                }
            },
            duplicate: subtree_state.clone(),
            delete: subtree_state,
            repair: if entry {
                state.clone()
            } else {
                CapabilityState::Unsupported {
                    code: "NO_REFERENCE_SLOT",
                }
            },
            preview: if entry {
                CapabilityState::Unsupported {
                    code: "PREVIEW_OWNER_EXPRESSION",
                }
            } else if supported {
                CapabilityState::Available
            } else {
                state
            },
        })
    }
    pub fn pose_capabilities(&self, id: &str) -> Result<PoseCapabilities, Status> {
        let doc = self.content.model();
        let kind = if doc.pose().is_some_and(|a| a.id == id) {
            ObjectKind::Pose
        } else if doc.get_pose_group(id).is_some() {
            ObjectKind::PoseGroup
        } else if doc.get_pose_entry(id).is_some() {
            ObjectKind::PoseEntry
        } else if doc.get_pose_link(id).is_some() {
            ObjectKind::PoseLink
        } else {
            return Err(Status::error("MISSING_OBJECT", id));
        };

        let supported = matches!(
            kind,
            ObjectKind::Pose | ObjectKind::PoseGroup | ObjectKind::PoseEntry | ObjectKind::PoseLink
        );
        let unsupported = |code| CapabilityState::Unsupported { code };
        let state = if !supported {
            unsupported("WRONG_OBJECT_KIND")
        } else if doc.pose_unlocked(id).is_err() {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        let subtree = if supported && doc.pose_subtree_unlocked(id).is_err() {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            state.clone()
        };
        Ok(PoseCapabilities {
            inspect: CapabilityState::Available,
            patch: state.clone(),
            create_child: if matches!(
                kind,
                ObjectKind::Pose | ObjectKind::PoseGroup | ObjectKind::PoseEntry
            ) {
                state.clone()
            } else {
                unsupported("NO_CHILD_COLLECTION")
            },
            duplicate: if kind == ObjectKind::Pose {
                unsupported("POSE_SINGLETON")
            } else {
                subtree.clone()
            },
            delete: subtree.clone(),
            move_object: if kind == ObjectKind::Pose {
                unsupported("POSE_SINGLETON")
            } else {
                subtree.clone()
            },
            reorder: if kind == ObjectKind::Pose {
                unsupported("POSE_SINGLETON")
            } else {
                subtree.clone()
            },
            repair: if matches!(kind, ObjectKind::PoseEntry | ObjectKind::PoseLink) {
                state
            } else {
                unsupported("NO_REFERENCE_SLOT")
            },
            dissolve: if kind == ObjectKind::PoseGroup
                && doc.pose().is_some_and(|p| p.groups.len() < 2)
            {
                CapabilityState::Unavailable {
                    code: "POSE_RECEIVING_GROUP_REQUIRED",
                }
            } else if kind == ObjectKind::PoseGroup {
                subtree
            } else {
                unsupported("REQUIRES_POSE_GROUP")
            },
            preview: if kind == ObjectKind::Pose {
                CapabilityState::Available
            } else {
                unsupported("PREVIEW_POSE_OWNER")
            },
        })
    }
    pub fn physics_capabilities(&self, id: &str) -> Result<PhysicsCapabilities, Status> {
        let doc = self.content.model();
        let kind = if doc.physics().is_some_and(|a| a.id == id) {
            ObjectKind::Physics
        } else if doc.get_physics_group(id).is_some() {
            ObjectKind::PhysicsGroup
        } else if doc.get_physics_input(id).is_some() {
            ObjectKind::PhysicsInput
        } else if doc.get_physics_output(id).is_some() {
            ObjectKind::PhysicsOutput
        } else if doc.get_physics_node(id).is_some() {
            ObjectKind::PhysicsNode
        } else {
            return Err(Status::error("MISSING_OBJECT", id));
        };

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
        } else if doc.physics_unlocked(id).is_err() {
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
                .filter(|g| kind == ObjectKind::Physics || g.id == id)
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
            .get_physics_node(id)
            .is_some_and(|(g, _)| g.vertices[0].id == id);
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
            delete: subtree.clone(),
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
    pub fn motion_capabilities(&self, id: &str) -> Result<MotionCapabilities, Status> {
        let doc = self.content.model();
        let kind = if doc.get_motion(id).is_some() {
            ObjectKind::Motion
        } else if doc.get_motion_group(id).is_some() {
            ObjectKind::MotionGroup
        } else if doc.get_motion_registration(id).is_some() {
            ObjectKind::MotionRegistration
        } else if doc.get_motion_track(id).is_some() {
            ObjectKind::MotionTrack
        } else if doc.get_motion_key(id).is_some() {
            ObjectKind::MotionKey
        } else if doc.get_motion_event(id).is_some() {
            ObjectKind::MotionEvent
        } else {
            return Err(Status::error("MISSING_OBJECT", id));
        };

        let supported = matches!(
            kind,
            ObjectKind::Motion
                | ObjectKind::MotionGroup
                | ObjectKind::MotionRegistration
                | ObjectKind::MotionTrack
                | ObjectKind::MotionKey
                | ObjectKind::MotionEvent
        );
        let state = if !supported {
            CapabilityState::Unsupported {
                code: "WRONG_OBJECT_KIND",
            }
        } else if doc.motion_unlocked(id).is_err() {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        let preview = if matches!(kind, ObjectKind::Motion | ObjectKind::MotionRegistration) {
            CapabilityState::Available
        } else {
            CapabilityState::Unsupported {
                code: "PREVIEW_CLIP_OR_REGISTRATION",
            }
        };
        let movement = if matches!(
            kind,
            ObjectKind::MotionRegistration
                | ObjectKind::MotionTrack
                | ObjectKind::MotionKey
                | ObjectKind::MotionEvent
        ) {
            state.clone()
        } else {
            CapabilityState::Unsupported {
                code: "USE_DOMAIN_ORDER",
            }
        };

        let descendant_locked = doc
            .editor_state()
            .objects
            .iter()
            .filter(|(_, flags)| flags.locked)
            .any(|(locked_id, _)| match kind {
                ObjectKind::Motion => {
                    doc.get_motion_track(locked_id)
                        .is_some_and(|(c, _)| c.id == id)
                        || doc
                            .get_motion_key(locked_id)
                            .is_some_and(|(c, _, _)| c.id == id)
                        || doc
                            .get_motion_event(locked_id)
                            .is_some_and(|(c, _)| c.id == id)
                }
                ObjectKind::MotionTrack => doc
                    .get_motion_key(locked_id)
                    .is_some_and(|(_, t, _)| t.id == id),
                ObjectKind::MotionGroup => doc
                    .get_motion_registration(locked_id)
                    .is_some_and(|(c, _)| c == MotionContainer::Group(id.into())),
                _ => false,
            });
        let subtree = if descendant_locked {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            state.clone()
        };
        let child = if matches!(kind, ObjectKind::Motion | ObjectKind::MotionGroup) {
            state.clone()
        } else {
            CapabilityState::Unsupported {
                code: "NO_CHILD_COLLECTION",
            }
        };
        let repair = if matches!(
            kind,
            ObjectKind::MotionRegistration | ObjectKind::MotionTrack
        ) {
            state.clone()
        } else {
            CapabilityState::Unsupported {
                code: "NO_REFERENCE_SLOT",
            }
        };
        let content = if doc
            .get_motion_track(id)
            .is_some_and(|(_, t)| doc.motion_track_disabled_reason(&t.target).is_some())
        {
            CapabilityState::Unavailable {
                code: "CONFIGURE_TRACK_TARGET",
            }
        } else {
            state.clone()
        };
        Ok(MotionCapabilities {
            inspect: CapabilityState::Available,
            patch: state.clone(),
            duplicate: subtree.clone(),
            delete: subtree,
            move_object: movement,
            preview,
            create_child: child,
            reorder: if matches!(
                kind,
                ObjectKind::MotionGroup | ObjectKind::MotionTrack | ObjectKind::MotionRegistration
            ) {
                state.clone()
            } else {
                CapabilityState::Unsupported {
                    code: "USE_SEMANTIC_TIME_OPERATION",
                }
            },
            repair,
            edit_content: content,
        })
    }
}

impl DocumentState {
    /// Check a typed stable selector in this captured state; no runtime handle
    /// allocation or Session lifetime is involved.
    pub fn contains_object(&self, kind: ObjectKind, id: &str) -> bool {
        let doc = self.content.model();
        match kind {
            ObjectKind::Asset => self.asset(id).is_some(),
            ObjectKind::Mesh => doc.get_mesh(id).is_some(),
            ObjectKind::Parameter => doc.get_parameter(id).is_some(),
            ObjectKind::MeshBinding => doc.get_binding(id).is_some(),
            ObjectKind::Part => doc.get_part(id).is_some(),
            ObjectKind::Transform => doc.get_transform(id).is_some(),
            ObjectKind::SceneBinding => doc.get_scene_binding(id).is_some(),
            ObjectKind::BlendKeyTable => doc.get_blend_key_table(id).is_some(),
            ObjectKind::BlendConstraint => doc.get_blend_constraint(id).is_some(),
            ObjectKind::BlendBinding => doc.get_blend_binding(id).is_some(),
            ObjectKind::Glue => doc.get_glue(id).is_some(),
            ObjectKind::Offscreen => doc.get_offscreen(id).is_some(),
            ObjectKind::Expression => doc.get_expression(id).is_some(),
            ObjectKind::Motion => doc.get_motion(id).is_some(),

            ObjectKind::BindingAxis => doc
                .binding_identity(id)
                .is_some_and(|i| i.kind == crate::document::BindingIdentityKind::Axis),
            ObjectKind::BindingSample => doc
                .binding_identity(id)
                .is_some_and(|i| i.kind == crate::document::BindingIdentityKind::Sample),
            ObjectKind::GlueVertexPair => doc.get_glue_pair(id).is_some(),
            ObjectKind::DrawOrderGroup => doc.drawing_group_owners().iter().any(|owner| {
                if owner.is_empty() {
                    doc.id() == id
                } else {
                    owner == id
                }
            }),
            ObjectKind::CdiParameterGroup => doc
                .display_info()
                .parameter_groups
                .as_ref()
                .is_some_and(|groups| groups.iter().any(|group| group.id == id)),
            ObjectKind::ModelParameterGroup
            | ObjectKind::ModelParameterMember
            | ObjectKind::ModelHitArea
            | ObjectKind::CdiCombinedMember => reference_kind(&doc, id) == Some(kind),
            ObjectKind::CdiCombinedSet => doc
                .display_info()
                .combined_parameters
                .as_ref()
                .is_some_and(|sets| sets.iter().any(|set| set.id == id)),
            ObjectKind::ExpressionEntry => doc.get_expression_entry(id).is_some(),
            ObjectKind::MotionGroup => doc.get_motion_group(id).is_some(),
            ObjectKind::MotionRegistration => doc.get_motion_registration(id).is_some(),
            ObjectKind::MotionTrack => doc.get_motion_track(id).is_some(),
            ObjectKind::MotionKey => doc.get_motion_key(id).is_some(),
            ObjectKind::MotionEvent => doc.get_motion_event(id).is_some(),
            ObjectKind::Attachment => doc.get_attachment(id).is_some(),
            ObjectKind::Pose => doc.pose().is_some_and(|pose| pose.id == id),
            ObjectKind::PoseGroup => doc.get_pose_group(id).is_some(),
            ObjectKind::PoseEntry => doc.get_pose_entry(id).is_some(),
            ObjectKind::PoseLink => doc.get_pose_link(id).is_some(),
            ObjectKind::PhysicsGroup => doc.get_physics_group(id).is_some(),
            ObjectKind::PhysicsInput => doc.get_physics_input(id).is_some(),
            ObjectKind::PhysicsOutput => doc.get_physics_output(id).is_some(),
            ObjectKind::PhysicsNode => doc.get_physics_node(id).is_some(),
            ObjectKind::Physics => doc.physics().is_some_and(|physics| physics.id == id),
        }
    }
    pub fn require_object(&self, kind: ObjectKind, id: &str) -> Result<(), Status> {
        if self.contains_object(kind, id) {
            Ok(())
        } else {
            Err(Status::error("MISSING_OBJECT", id))
        }
    }
    pub fn binding_capabilities(
        &self,
        kind: ObjectKind,
        id: &str,
    ) -> Result<BindingCapabilities, Status> {
        use zipzi_contract::authoring::{CapabilityState as C, ObjectKind as K};
        self.require_object(kind, id)?;
        let d = self.content.model();
        if !matches!(
            kind,
            K::MeshBinding
                | K::SceneBinding
                | K::Glue
                | K::BlendKeyTable
                | K::BlendConstraint
                | K::BlendBinding
                | K::BindingAxis
                | K::BindingSample
        ) {
            return Err(Status::error(
                "WRONG_OBJECT_KIND",
                "Expected a binding, axis, or sample",
            ));
        }
        let editable = if d.authoring_unlocked(id).is_ok() {
            C::Available
        } else {
            C::Unavailable {
                code: "OBJECT_LOCKED",
            }
        };
        let identity = d.binding_identity(id);
        let owner = identity.as_ref().map_or(id, |i| i.owner.as_str());
        let axes = d
            .get_binding(owner)
            .map(|b| b.axes.as_slice())
            .or_else(|| d.get_scene_binding(owner).map(|b| b.axes.as_slice()))
            .or_else(|| {
                d.get_glue(owner)
                    .and_then(|g| g.binding.as_ref())
                    .map(|b| b.axes.as_slice())
            });
        let pending_axis = identity.as_ref().is_some_and(|identity| {
            let axis = identity.axis.as_deref().unwrap_or(&identity.id);
            axes.is_some_and(|axes| {
                axes.iter()
                    .any(|a| a.id == axis && a.parameter_id.is_empty())
            })
        });
        let pending = pending_axis
            || d.get_blend_binding(owner)
                .is_some_and(|b| !d.blend_binding_active(b))
            || d.get_binding(owner)
                .is_some_and(|b| b.axes.iter().all(|a| a.parameter_id.is_empty()))
            || d.get_scene_binding(owner)
                .is_some_and(|b| b.axes.iter().all(|a| a.parameter_id.is_empty()))
            || d.get_glue(owner)
                .and_then(|g| g.binding.as_ref())
                .is_some_and(|b| b.axes.iter().all(|a| a.parameter_id.is_empty()))
            || d.get_blend_key_table(owner)
                .is_some_and(|t| t.parameter_id.is_empty())
            || d.get_blend_constraint(owner)
                .is_some_and(|c| c.parameter_id.is_empty());
        let preview = if pending {
            C::Unavailable {
                code: "NEEDS_CONFIGURATION",
            }
        } else {
            C::Available
        };
        Ok(BindingCapabilities {
            inspect: C::Available,
            patch: editable.clone(),
            create_child: if matches!(kind, K::BindingSample | K::BlendBinding) {
                C::Unsupported {
                    code: "SAMPLES_OWNED_BY_GRID_OR_TABLE",
                }
            } else {
                editable.clone()
            },
            reorder: if kind == K::BindingSample {
                C::Unsupported {
                    code: "SEMANTIC_KEY_ORDER",
                }
            } else {
                editable.clone()
            },
            delete: editable.clone(),
            repair: editable,
            preview,
        })
    }
    pub fn scene_capabilities(
        &self,
        kind: ObjectKind,
        id: &str,
    ) -> Result<SceneCapabilities, Status> {
        self.require_object(kind, id)?;
        let d = self.content.model();
        let role = kind == ObjectKind::DrawOrderGroup;
        let supported = role || scene_kind(&d, id) == Some(kind);
        let root = role && id == d.id();
        let state = if !supported {
            CapabilityState::Unsupported {
                code: "WRONG_OBJECT_KIND",
            }
        } else if if role {
            d.drawing_group_unlocked(if root { "" } else { id })
                .is_err()
        } else {
            d.scene_lifecycle_unlocked(&[id.into()], SceneCopyScope::Selected)
                .is_err()
        } {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        let container = matches!(kind, ObjectKind::Mesh | ObjectKind::Transform);
        let delete = if root {
            CapabilityState::Unsupported {
                code: "SYSTEM_DRAWING_ROOT",
            }
        } else if role && d.offscreen_for_part(id).is_some() {
            CapabilityState::Unavailable {
                code: "DRAWING_GROUP_HAS_OFFSCREEN",
            }
        } else {
            state.clone()
        };
        Ok(SceneCapabilities {
            inspect: CapabilityState::Available,
            patch: state.clone(),
            move_object: state.clone(),
            reparent: if container {
                state.clone()
            } else {
                CapabilityState::Unsupported {
                    code: "NO_DEFORMATION_PARENT",
                }
            },
            reorder: state.clone(),
            duplicate: state.clone(),
            delete: delete.clone(),
            dissolve: if role {
                delete
            } else {
                CapabilityState::Unsupported {
                    code: "NOT_DRAWING_GROUP",
                }
            },
        })
    }
    pub fn organization_capabilities(
        &self,
        kind: ObjectKind,
        id: &str,
    ) -> Result<OrganizationCapabilities, Status> {
        self.require_object(kind, id)?;
        let d = self.content.model();
        let supported = organization_kind(&d, id) == Some(kind);
        let unavailable = |code| CapabilityState::Unavailable { code };
        let unsupported = |code| CapabilityState::Unsupported { code };
        let state = if !supported {
            unsupported("WRONG_OBJECT_KIND")
        } else if d.organization_unlocked(id).is_err() {
            unavailable("OBJECT_LOCKED")
        } else {
            CapabilityState::Available
        };
        let subtree = if supported && d.organization_subtree_unlocked(&[id.into()]).is_err() {
            unavailable("OBJECT_LOCKED")
        } else {
            state.clone()
        };
        let container = matches!(kind, ObjectKind::Part | ObjectKind::CdiParameterGroup);
        let unresolved_group_copy = kind == ObjectKind::CdiParameterGroup && d.organization_subtree(&[id.into()]).is_ok_and(|ids| {
            d.display_info().parameters.iter().flatten().any(|entry| matches!(entry,
                crate::document::CdiParameterEntry::Unresolved { group_id: Some(group), .. } if ids.contains(group)))
        });
        Ok(OrganizationCapabilities {
            inspect: CapabilityState::Available,
            patch: state.clone(),
            create_child: if container {
                state.clone()
            } else {
                unsupported("NO_CHILD_COLLECTION")
            },
            move_object: subtree.clone(),
            reorder: subtree.clone(),
            duplicate: if unresolved_group_copy {
                unavailable("UNRESOLVED_PARAMETER_COPY_CONFLICT")
            } else {
                subtree.clone()
            },
            delete: subtree.clone(),
            dissolve: if kind == ObjectKind::Part && !d.part_dissolve_state_representable(id) {
                unavailable("PART_DISSOLVE_STATE_UNREPRESENTABLE")
            } else if container {
                subtree
            } else {
                unsupported("DISSOLVE_REQUIRES_CONTAINER")
            },
        })
    }
    pub fn reference_capabilities(
        &self,
        kind: ObjectKind,
        id: &str,
    ) -> Result<ReferenceCapabilities, Status> {
        self.require_object(kind, id)?;
        let d = self.content.model();
        let i = d
            .reference_identity(id)
            .filter(|_| reference_kind(&d, id) == Some(kind))
            .ok_or_else(|| Status::error("WRONG_REFERENCE_OBJECT", id))?;
        let state = if d.reference_unlocked(id).is_ok() {
            CapabilityState::Available
        } else {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        };
        let root = matches!(
            i.kind,
            ReferenceKind::ModelParameterGroup | ReferenceKind::CdiCombinedSet
        );
        Ok(ReferenceCapabilities {
            inspect: CapabilityState::Available,
            patch: if i.kind == ReferenceKind::CdiCombinedSet {
                CapabilityState::Unsupported {
                    code: "NO_REFERENCE_FIELDS",
                }
            } else {
                state.clone()
            },
            create_child: if root {
                state.clone()
            } else {
                CapabilityState::Unsupported {
                    code: "REFERENCE_HAS_NO_CHILDREN",
                }
            },
            move_object: state.clone(),
            duplicate: state.clone(),
            delete: state.clone(),
            dissolve: if root {
                state
            } else {
                CapabilityState::Unsupported {
                    code: "REFERENCE_CANNOT_DISSOLVE",
                }
            },
        })
    }
    pub fn scene_content_capabilities(
        &self,
        kind: ObjectKind,
        id: &str,
    ) -> Result<SceneContentCapabilities, Status> {
        self.require_object(kind, id)?;
        let d = self.content.model();
        let supported = matches!(
            kind,
            ObjectKind::Mesh
                | ObjectKind::Glue
                | ObjectKind::GlueVertexPair
                | ObjectKind::Offscreen
        ) || (kind == ObjectKind::Transform
            && d.get_transform(id).and_then(|t| t.warp()).is_some());
        let state = if !supported {
            CapabilityState::Unsupported {
                code: "WRONG_CONTENT_OWNER",
            }
        } else if d
            .scene_lifecycle_unlocked(
                &[d.glue_pair_owner(id).unwrap_or(id).into()],
                crate::document::SceneCopyScope::Selected,
            )
            .is_err()
        {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        Ok(SceneContentCapabilities {
            inspect: if supported {
                CapabilityState::Available
            } else {
                state.clone()
            },
            edit: state.clone(),
            topology: state,
        })
    }
    pub fn resource_capabilities(
        &self,
        kind: ObjectKind,
        id: &str,
    ) -> Result<ResourceCapabilities, Status> {
        self.require_object(kind, id)?;
        if kind != ObjectKind::Asset {
            return Err(Status::error("WRONG_OBJECT_KIND", id));
        }
        let state = match self.content.model().authoring_unlocked(id) {
            Ok(()) => CapabilityState::Available,
            Err(_) => CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            },
        };
        let delete = state.clone();
        Ok(ResourceCapabilities {
            inspect: CapabilityState::Available,
            patch: state.clone(),
            load_content: state.clone(),
            duplicate: state.clone(),
            reorder: state,
            delete,
        })
    }
    pub fn attachment_capabilities(
        &self,
        kind: ObjectKind,
        id: &str,
    ) -> Result<AttachmentCapabilities, Status> {
        self.require_object(kind, id)?;
        let state = if kind != ObjectKind::Attachment {
            CapabilityState::Unsupported {
                code: "WRONG_OBJECT_KIND",
            }
        } else if self.content.model().editor_state().get(id).locked {
            CapabilityState::Unavailable {
                code: "OBJECT_LOCKED",
            }
        } else {
            CapabilityState::Available
        };
        let delete = state.clone();
        Ok(AttachmentCapabilities {
            inspect: CapabilityState::Available,
            patch: state.clone(),
            duplicate: state.clone(),
            delete,
            load_content: state,
        })
    }
}
fn scene_kind(d: &crate::model_validation::Model<'_>, id: &str) -> Option<ObjectKind> {
    if d.get_mesh(id).is_some() {
        Some(ObjectKind::Mesh)
    } else if d.get_transform(id).is_some() {
        Some(ObjectKind::Transform)
    } else if d.get_offscreen(id).is_some() {
        Some(ObjectKind::Offscreen)
    } else if d.get_glue(id).is_some() {
        Some(ObjectKind::Glue)
    } else {
        None
    }
}
fn organization_kind(d: &crate::model_validation::Model<'_>, id: &str) -> Option<ObjectKind> {
    if d.get_parameter(id).is_some() {
        Some(ObjectKind::Parameter)
    } else if d.get_parameter_group(id).is_some() {
        Some(ObjectKind::CdiParameterGroup)
    } else if d.get_part(id).is_some() {
        Some(ObjectKind::Part)
    } else if d.get_mesh(id).is_some() {
        Some(ObjectKind::Mesh)
    } else if d.get_transform(id).is_some() {
        Some(ObjectKind::Transform)
    } else if d.get_offscreen(id).is_some() {
        Some(ObjectKind::Offscreen)
    } else {
        None
    }
}
fn reference_kind(d: &crate::model_validation::Model<'_>, id: &str) -> Option<ObjectKind> {
    d.reference_identity(id).map(|i| match i.kind {
        ReferenceKind::ModelParameterGroup => ObjectKind::ModelParameterGroup,
        ReferenceKind::ModelParameterMember => ObjectKind::ModelParameterMember,
        ReferenceKind::ModelHitArea => ObjectKind::ModelHitArea,
        ReferenceKind::CdiCombinedSet => ObjectKind::CdiCombinedSet,
        ReferenceKind::CdiCombinedMember => ObjectKind::CdiCombinedMember,
    })
}
impl DocumentState {
    pub fn collection_capabilities(
        &self,
        target: &zipzi_contract::kernel_wire::CollectionTarget,
    ) -> Result<CollectionCapabilities, Status> {
        use zipzi_contract::kernel_wire::CollectionTarget as T;
        use CapabilityState as C;
        let model = self.content.model();
        let unavailable = || C::Unavailable {
            code: "OBJECT_LOCKED",
        };
        let specialized = || C::Unsupported {
            code: "USE_SPECIALIZED_EDIT",
        };
        let all = |state: C| CollectionCapabilities {
            create: state.clone(),
            move_into: state.clone(),
            reorder: state,
        };
        Ok(match target {
            T::Organization(collection) => {
                let owner = match collection {
                    OrganizationCollection::Parameters { group_id } => group_id.as_deref(),
                    OrganizationCollection::ParameterGroups { parent_id } => parent_id.as_deref(),
                    OrganizationCollection::PartMembers { part_id } => part_id.as_deref(),
                };
                if let Some(id) = owner {
                    self.require_object(
                        if matches!(collection, OrganizationCollection::PartMembers { .. }) {
                            ObjectKind::Part
                        } else {
                            ObjectKind::CdiParameterGroup
                        },
                        id,
                    )?;
                }
                all(
                    if owner.is_some_and(|id| model.organization_unlocked(id).is_err()) {
                        unavailable()
                    } else {
                        C::Available
                    },
                )
            }
            T::Scene(collection) => {
                model.validate_scene_collection(collection)?;
                let owner = match collection {
                    SceneCollection::Organization { part_id } => part_id.as_deref(),
                    SceneCollection::Deformation { parent_id } => parent_id.as_deref(),
                    SceneCollection::Drawing { owner } => Some(owner.as_str()),
                    _ => None,
                };
                let mut cap = all(
                    if owner.is_some_and(|id| model.organization_unlocked(id).is_err()) {
                        unavailable()
                    } else {
                        C::Available
                    },
                );
                if matches!(collection, SceneCollection::DrawingGroups) {
                    cap.reorder = C::Unsupported {
                        code: "DRAWING_GROUP_ORDER_DERIVED",
                    };
                }
                cap
            }
            T::References(collection) => {
                model.reference_collection(collection)?;
                all(
                    if model.reference_collection_unlocked(collection).is_err() {
                        unavailable()
                    } else {
                        C::Available
                    },
                )
            }
            T::Pose(collection) => {
                let (owner, kind) = match collection {
                    PoseCollection::Groups => {
                        (model.pose().map(|p| p.id.as_str()), ObjectKind::Pose)
                    }
                    PoseCollection::Entries { group_id } => {
                        (Some(group_id.as_str()), ObjectKind::PoseGroup)
                    }
                    PoseCollection::Links { entry_id } => {
                        (Some(entry_id.as_str()), ObjectKind::PoseEntry)
                    }
                };
                if let Some(owner) = owner {
                    self.require_object(kind, owner)?;
                    all(if model.pose_unlocked(owner).is_err() {
                        unavailable()
                    } else {
                        C::Available
                    })
                } else {
                    CollectionCapabilities {
                        create: C::Available,
                        move_into: C::Unavailable {
                            code: "MISSING_POSE",
                        },
                        reorder: C::Unavailable {
                            code: "MISSING_POSE",
                        },
                    }
                }
            }
            T::Physics(collection) => {
                let (owner, kind) = match collection {
                    PhysicsCollection::Groups => {
                        (model.physics().map(|p| p.id.as_str()), ObjectKind::Physics)
                    }
                    PhysicsCollection::Inputs { group_id }
                    | PhysicsCollection::Outputs { group_id }
                    | PhysicsCollection::Nodes { group_id } => {
                        (Some(group_id.as_str()), ObjectKind::PhysicsGroup)
                    }
                };
                if let Some(owner) = owner {
                    self.require_object(kind, owner)?;
                    let cap = self.physics_capabilities(owner)?;
                    CollectionCapabilities {
                        create: cap.create_child,
                        move_into: if matches!(
                            collection,
                            PhysicsCollection::Inputs { .. } | PhysicsCollection::Outputs { .. }
                        ) {
                            cap.patch.clone()
                        } else {
                            specialized()
                        },
                        reorder: if matches!(collection, PhysicsCollection::Nodes { .. }) {
                            specialized()
                        } else {
                            cap.patch
                        },
                    }
                } else {
                    CollectionCapabilities {
                        create: C::Available,
                        move_into: specialized(),
                        reorder: C::Available,
                    }
                }
            }
            T::ExpressionEntries { expression_id } => {
                self.require_object(ObjectKind::Expression, expression_id)?;
                let cap = self.expression_capabilities(expression_id)?;
                CollectionCapabilities {
                    create: cap.create_entry,
                    move_into: cap.patch.clone(),
                    reorder: cap.patch,
                }
            }
            T::Motion(MotionContainer::Group(id)) => {
                self.require_object(ObjectKind::MotionGroup, id)?;
                let cap = self.motion_capabilities(id)?;
                CollectionCapabilities {
                    create: cap.create_child,
                    move_into: cap.patch,
                    reorder: cap.reorder,
                }
            }
            T::Motion(MotionContainer::Root) => all(C::Available),
            T::MotionGroups | T::Expressions | T::Resources | T::Attachments => {
                CollectionCapabilities {
                    create: C::Available,
                    move_into: specialized(),
                    reorder: C::Available,
                }
            }
            T::Clips => CollectionCapabilities {
                create: C::Available,
                move_into: specialized(),
                reorder: specialized(),
            },
        })
    }
}

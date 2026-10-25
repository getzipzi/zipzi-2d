//! Cheap collection queries shared by all hosts. No identities, edit candidates,
//! keyform products or geometry conversion plans are allocated for availability.
use crate::{
    AuthoringSession, CapabilityState, ObjectKind, OrganizationCollection, PhysicsCollection,
    PoseCollection, ReferenceCollection, SceneCollection, SdkError,
};
use zipzi_core::document::MotionContainer;
#[derive(Clone, Debug)]
pub enum AuthoringCollection {
    Organization(OrganizationCollection),
    Scene(SceneCollection),
    References(ReferenceCollection),
    Pose(PoseCollection),
    Physics(PhysicsCollection),
    Motion(MotionContainer),
    MotionGroups,
    Clips,
    Expressions,
    ExpressionEntries { expression_id: String },
    Resources,
    Attachments,
}
#[derive(Clone, Debug, PartialEq, Eq, zipzi_api_derive::ApiShape, serde::Serialize)]
pub struct CollectionCapabilities {
    pub create: CapabilityState,
    pub move_into: CapabilityState,
    pub reorder: CapabilityState,
}
impl AuthoringSession {
    pub fn authoring_collection_capabilities(
        &self,
        collection: &AuthoringCollection,
    ) -> Result<CollectionCapabilities, SdkError> {
        use CapabilityState as C;
        let specialized = || C::Unsupported {
            code: "USE_SPECIALIZED_EDIT",
        };
        let root = || CollectionCapabilities {
            create: C::Available,
            move_into: specialized(),
            reorder: specialized(),
        };
        macro_rules! collection {
            ($query:expr) => {{
                let c = $query?;
                CollectionCapabilities {
                    create: c.create,
                    move_into: c.move_into,
                    reorder: c.reorder,
                }
            }};
        }
        Ok(match collection {
            AuthoringCollection::Organization(c) => {
                collection!(self.organization_collection_capabilities(c))
            }
            AuthoringCollection::Scene(c) => collection!(self.scene_collection_capabilities(c)),
            AuthoringCollection::References(c) => {
                collection!(self.reference_collection_capabilities(c))
            }
            AuthoringCollection::Pose(c) => collection!(self.pose_collection_capabilities(c)),
            AuthoringCollection::Resources | AuthoringCollection::Attachments => {
                let c = if matches!(collection, AuthoringCollection::Resources) {
                    self.resource_collection_capabilities()
                } else {
                    self.attachment_collection_capabilities()
                };
                CollectionCapabilities {
                    create: c.create,
                    reorder: c.reorder,
                    move_into: specialized(),
                }
            }
            AuthoringCollection::ExpressionEntries { expression_id } => {
                let c = self.expression_capabilities(
                    &self.handle(ObjectKind::Expression, expression_id)?,
                )?;
                CollectionCapabilities {
                    create: c.create_entry,
                    move_into: c.patch.clone(),
                    reorder: c.patch,
                }
            }
            AuthoringCollection::Motion(MotionContainer::Group(id)) => {
                let c = self.motion_capabilities(&self.handle(ObjectKind::MotionGroup, id)?)?;
                CollectionCapabilities {
                    create: c.create_child,
                    move_into: c.patch,
                    reorder: c.reorder,
                }
            }
            AuthoringCollection::Physics(c) => {
                let (owner, kind) = match c {
                    PhysicsCollection::Groups => (
                        self.document().physics().map(|a| a.id.as_str()),
                        ObjectKind::Physics,
                    ),
                    PhysicsCollection::Inputs { group_id }
                    | PhysicsCollection::Outputs { group_id }
                    | PhysicsCollection::Nodes { group_id } => {
                        (Some(group_id.as_str()), ObjectKind::PhysicsGroup)
                    }
                };
                if let Some(owner) = owner {
                    let c = self.physics_capabilities(&self.handle(kind, owner)?)?;
                    // Node chains have dedicated topology edits; collection availability
                    // never promises an arbitrary node reorder or cross-group move.
                    CollectionCapabilities {
                        create: c.create_child,
                        move_into: if matches!(
                            collection,
                            AuthoringCollection::Physics(
                                PhysicsCollection::Inputs { .. }
                                    | PhysicsCollection::Outputs { .. }
                            )
                        ) {
                            c.patch.clone()
                        } else {
                            specialized()
                        },
                        reorder: if matches!(
                            collection,
                            AuthoringCollection::Physics(PhysicsCollection::Nodes { .. })
                        ) {
                            specialized()
                        } else {
                            c.patch
                        },
                    }
                } else {
                    CollectionCapabilities {
                        reorder: C::Available,
                        ..root()
                    }
                }
            }
            AuthoringCollection::Motion(MotionContainer::Root) => CollectionCapabilities {
                create: C::Available,
                move_into: C::Available,
                reorder: C::Available,
            },
            AuthoringCollection::MotionGroups | AuthoringCollection::Expressions => {
                CollectionCapabilities {
                    create: C::Available,
                    move_into: specialized(),
                    reorder: C::Available,
                }
            }
            AuthoringCollection::Clips => root(),
        })
    }
}

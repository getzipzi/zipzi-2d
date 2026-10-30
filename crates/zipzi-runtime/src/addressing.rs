//! Finite selectors for the SDK's stable addresses. Tokens retain the actual SDK
//! address; later patches must pass SDK incarnation/context/field epoch checks.
pub use zipzi_contract::runtime::{AddressPatch, AddressSelector};
use zipzi_sdk::*;
pub(crate) enum CapturedAddress {
    Scene(SceneFieldAddress),
    DrawingGroup(DrawingGroupAddress),
    Binding(BindingFieldAddress),
    Organization(OrganizationAddress),
    Content(SceneContentAddress),
    Reference(ReferenceFieldAddress),
    Selection(ReferenceSelectionAddress),
    ResourceName(ResourceNameAddress),
    AttachmentPath(AttachmentPathAddress),
    Document(DocumentFieldAddress),
    MotionRegistration(MotionRegistrationAddress),
    ExpressionEntry(ExpressionEntryAddress),
    Physics(PhysicsFieldAddress),
    Pose(PoseFieldAddress),
}

pub use zipzi_contract::runtime::CapabilityDomain;

pub use zipzi_contract::runtime::Capabilities;
pub use zipzi_contract::runtime::CollectionQuery;

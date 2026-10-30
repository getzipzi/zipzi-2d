//! Client-scoped leases for SDK identities and scene label addresses.
//! Only SDK handles/epochs determine validity; persistent IDs never revive leases.
use super::*;
use crate::authoring::PlanSnapshot;
use serde::Serialize;
use zipzi_sdk::SceneSummaryRead;

pub(super) struct RefEntry {
    pub client: ClientId,
    pub session: SessionId,
    pub handle: zipzi_sdk::ObjectHandle,
    pub bytes: usize,
}
pub(super) struct AddressEntry {
    pub client: ClientId,
    pub session: SessionId,
    pub address: crate::addressing::CapturedAddress,
    pub bytes: usize,
}
#[derive(Debug, Serialize)]
pub struct ObjectRefInfo {
    pub reference: ObjectRefId,
    pub id: String,
    pub object_kind: zipzi_sdk::ObjectKind,
}
#[derive(Debug, Serialize)]
pub struct LabelAddressInfo {
    pub address: FieldAddressId,
    pub id: String,
    pub expected: String,
}
impl Runtime {
    pub(super) fn reference_bytes(&self) -> usize {
        self.object_refs
            .values()
            .map(|r| r.bytes)
            .chain(self.field_addresses.values().map(|r| r.bytes))
            .fold(0, usize::saturating_add)
    }
    pub fn reference_counts(&self) -> (usize, usize) {
        (self.object_refs.len(), self.field_addresses.len())
    }
    pub(super) fn admit_reference(&self, client: ClientId, bytes: usize) -> Result<()> {
        self.client(client)?;
        let count = self
            .object_refs
            .values()
            .filter(|r| r.client == client)
            .count()
            + self
                .field_addresses
                .values()
                .filter(|r| r.client == client)
                .count();
        if count >= self.limits.references_per_client {
            return Err(RuntimeError::Budget("object references"));
        }
        self.check_bytes(bytes, 0, 0)
    }
    pub fn capture_object_ref(
        &mut self,
        client: ClientId,
        session: SessionId,
        kind: zipzi_sdk::ObjectKind,
        id: &str,
    ) -> Result<ObjectRefInfo> {
        let bytes = id.len().saturating_mul(8).saturating_add(1024);
        self.admit_reference(client, bytes)?;
        let handle = self.ready(session)?.handle(kind, id)?;
        let reference = ObjectRefId::from_parts(self.instance, self.serial());
        self.object_refs.insert(
            reference,
            RefEntry {
                client,
                session,
                handle,
                bytes,
            },
        );
        Ok(ObjectRefInfo {
            reference,
            id: id.into(),
            object_kind: kind,
        })
    }
    pub(super) fn object_ref(&self, client: ClientId, reference: ObjectRefId) -> Result<&RefEntry> {
        self.client(client)?;
        let entry = self
            .object_refs
            .get(&reference)
            .ok_or(RuntimeError::InvalidReference)?;
        if entry.client != client {
            return Err(RuntimeError::WrongOwner);
        }
        self.ready(entry.session)?.resolve_handle(&entry.handle)?;
        Ok(entry)
    }
    pub fn resolve_object_ref(
        &self,
        client: ClientId,
        reference: ObjectRefId,
    ) -> Result<SourceRef> {
        let entry = self.object_ref(client, reference)?;
        Ok(SourceRef::Formal {
            session: entry.session,
            version: self.ready(entry.session)?.version(),
        })
    }
    pub fn object_scene_capabilities(
        &self,
        client: ClientId,
        reference: ObjectRefId,
    ) -> Result<zipzi_sdk::SceneCapabilities> {
        let entry = self.object_ref(client, reference)?;
        Ok(self
            .ready(entry.session)?
            .scene_capabilities(&entry.handle)?)
    }
    pub fn capture_label_address(
        &mut self,
        client: ClientId,
        reference: ObjectRefId,
    ) -> Result<LabelAddressInfo> {
        let entry = self.object_ref(client, reference)?;
        let session = entry.session;
        let sdk = self.ready(session)?;
        let bytes = zipzi_sdk::SceneSummary::response_bytes(sdk.document(), entry.handle.id())
            .ok_or(RuntimeError::InvalidReference)?
            .saturating_add(1024);
        self.admit_reference(client, bytes)?;
        let address = sdk.scene_field_address(entry.handle.id(), zipzi_sdk::SceneField::Label)?;
        let expected = match address.expected() {
            zipzi_sdk::SceneFieldValue::Label(label) => label.clone(),
            _ => unreachable!(),
        };
        let id = address.id().to_owned();
        let token = FieldAddressId::from_parts(self.instance, self.serial());
        self.field_addresses.insert(
            token,
            AddressEntry {
                client,
                session,
                address: crate::addressing::CapturedAddress::Scene(address),
                bytes,
            },
        );
        Ok(LabelAddressInfo {
            address: token,
            id,
            expected,
        })
    }
    pub fn plan_label_patch(
        &mut self,
        client: ClientId,
        address: FieldAddressId,
        name: String,
    ) -> Result<PlanSnapshot> {
        self.client(client)?;
        let entry = self
            .field_addresses
            .get(&address)
            .ok_or(RuntimeError::InvalidReference)?;
        if entry.client != client {
            return Err(RuntimeError::WrongOwner);
        }
        self.check_bytes(
            name.len().saturating_mul(8).saturating_add(entry.bytes),
            0,
            0,
        )?;
        let sdk = self.ready(entry.session)?;
        // This checks SDK incarnations, owner/context/field epochs, including
        // moves away and back and changes subsequently undone.
        let crate::addressing::CapturedAddress::Scene(address) = &entry.address else {
            return Err(RuntimeError::InvalidReference);
        };
        sdk.plan_scene_field_patch(
            address,
            zipzi_sdk::SceneFieldValue::Label(name.clone()),
            false,
        )?;
        let session = entry.session;
        let expected = match address.expected() {
            zipzi_sdk::SceneFieldValue::Label(label) => label.clone(),
            _ => unreachable!(),
        };
        let operation = zipzi_sdk::SceneOperation::SetSceneLabel {
            id: address.id().into(),
            expected,
            name,
        };
        self.plan_scene_authoring(client, session, None, operation)
    }
    pub fn release_object_ref(&mut self, client: ClientId, reference: ObjectRefId) -> Result<()> {
        self.client(client)?;
        if let Some(entry) = self.object_refs.get(&reference) {
            if entry.client != client {
                return Err(RuntimeError::WrongOwner);
            }
            self.object_refs.remove(&reference);
        } else if reference.runtime_instance() != self.instance {
            return Err(RuntimeError::InvalidReference);
        }
        Ok(())
    }
    pub fn release_field_address(
        &mut self,
        client: ClientId,
        address: FieldAddressId,
    ) -> Result<()> {
        self.client(client)?;
        if let Some(entry) = self.field_addresses.get(&address) {
            if entry.client != client {
                return Err(RuntimeError::WrongOwner);
            }
            self.field_addresses.remove(&address);
        } else if address.runtime_instance() != self.instance {
            return Err(RuntimeError::InvalidReference);
        }
        Ok(())
    }
}

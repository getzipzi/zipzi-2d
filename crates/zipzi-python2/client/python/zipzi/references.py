"""Runtime-scoped SDK handles and explicit scene label addresses."""
from __future__ import annotations
from ._generated import modeling as _modeling_codec
from typing import TYPE_CHECKING
from ._owned import Owned
from . import modeling
from ._results import Snapshot
from . import _queries as queries
from ._values import Source, record, string
if TYPE_CHECKING:
    from .backends import Backend
else:
    Backend = object
from ._plans import Plan

from ._generated.reference_values import (
    ObjectKind as ObjectKind, OBJECT_KINDS as OBJECT_KINDS,
    Capability as Capability, SceneCapabilities as SceneCapabilities,
)

class ObjectRef(Owned):
    def __init__(self, backend: Backend, kind: ObjectKind, id: str):
        super().__init__(backend, "reference")
        self._object_kind: ObjectKind = kind
        self._id = id

    @property
    def id(self) -> str:
        'Read the cached persistent UUID without IO. It is distinct from the reference incarnation and Runtime lease.'
        return self._id

    @property
    def kind(self) -> ObjectKind:
        'Read the cached SDK object-kind tag without IO.'
        return self._object_kind

    @classmethod
    def _create(cls, backend: Backend, session: object, kind: ObjectKind, id: str) -> ObjectRef:
        if kind not in OBJECT_KINDS or not isinstance(id, str):
            raise TypeError("Expected modeling object kind and ID")
        ref = cls(backend, kind, id)
        def load(raw: object) -> ObjectRef:
            ref._token = record(raw)["reference"]
            return ref
        return ref._request("capture", lambda: backend.object_ref(session, kind.value if isinstance(kind, modeling.ObjectKind) else kind, id, ref), load)

    def resolve(self) -> Source:
        'Validate the SDK reference against current incarnation/context and return exact formal Source. Deletion or recreation does not silently rebind the reference.'
        self._check(self._backend)
        return self._request("resolve", lambda: self._backend.resolve_ref(self._token, self), Source._read)

    def capabilities(self, domain: modeling.CapabilityDomain) -> Snapshot[modeling.Capabilities]:
        'Query a declared capability domain for this live reference. Returns an owned source-bearing capability result, without reserving future execution.'
        self._check(self._backend)
        _modeling_codec.validate_CapabilityDomain(domain)
        return self._request("capabilities", lambda: self._backend.modeling_capabilities(self._token, domain, self), queries.capabilities)

    def scene_capabilities(self) -> SceneCapabilities:
        'Query SDK scene capability states for this live reference. Unavailable and unsupported states retain their stable reason code.'
        self._check(self._backend)
        return self._request("capabilities", lambda: self._backend.ref_capabilities(self._token, self), SceneCapabilities._read)

    def label(self) -> LabelAddress:
        'Capture a label address bound to this reference incarnation and field epoch. Close the address independently; do not replace its plan with an ID-only rename.'
        self._check(self._backend)
        return LabelAddress._create(self)

    def __enter__(self) -> ObjectRef:
        super().__enter__()
        return self

class LabelAddress(Owned):
    def __init__(self, backend: Backend):
        super().__init__(backend, "address")
        self._id = ""
        self._expected = ""

    @property
    def id(self) -> str:
        'Return the cached target UUID without IO. Field epoch and incarnation remain part of this address; the ID alone cannot replace them.'
        return self._id

    @property
    def expected(self) -> str:
        'Return the captured label text without IO. A later SDK label/context/identity change invalidates the address; capture a new address instead of silently refreshing its precondition.'
        return self._expected

    @classmethod
    def _create(cls, reference: ObjectRef) -> LabelAddress:
        address = cls(reference._backend)
        def load(raw: object) -> LabelAddress:
            value = record(raw)
            address._token = value["address"]
            address._id = string(value["id"])
            address._expected = string(value["expected"])
            return address
        return address._request("capture", lambda: address._backend.label_address(reference._token, address), load)

    def plan(self, name: str) -> Plan:
        'Plan the label change using captured incarnation/context/field epochs. Returns an owned Plan and cached report; apply through Edit and close the Plan after use.'
        from ._plans import Plan
        self._check(self._backend)
        if not isinstance(name, str):
            raise TypeError("Expected label text")
        plan = Plan(self._backend)
        return plan._request("plan", lambda: self._backend.label_plan(self._token, name, plan), plan._load)

    def __enter__(self) -> LabelAddress:
        super().__enter__()
        return self

class FieldAddress(Owned):
    """An SDK address with incarnation and monotonic epoch validation."""
    @classmethod
    def _create(cls, backend: Backend, session: object, selector: modeling.AddressSelector) -> FieldAddress:
        _modeling_codec.validate_AddressSelector(selector)
        address = cls(backend, "address")
        def load(raw: object) -> FieldAddress:
            address._token = record(raw)["address"]
            return address
        return address._request("capture", lambda: backend.capture_address(session, selector, address), load)

    def plan(self, patch: modeling.AddressPatch) -> Plan:
        'Plan a typed address patch under captured SDK epochs and context. Stale fields or recreated objects fail explicitly. Returns the same owned Plan form used by Edit.apply.'
        from ._plans import Plan
        self._check(self._backend)
        _modeling_codec.validate_AddressPatch(patch)
        plan = Plan(self._backend)
        return plan._request("plan", lambda: self._backend.address_plan(self._token, patch, plan), plan._load)

    def __enter__(self) -> FieldAddress:
        super().__enter__()
        return self

__all__ = ['ObjectKind', 'Capability', 'SceneCapabilities', 'ObjectRef', 'LabelAddress', 'FieldAddress']

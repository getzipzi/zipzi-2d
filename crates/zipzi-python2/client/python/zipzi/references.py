"""Runtime-scoped SDK handles and explicit scene label addresses."""
from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal
from ._owned import Owned
from . import modeling
from ._values import Source, record, string
if TYPE_CHECKING:
    from .backends import Backend
    from .plans import Plan

ObjectKind = Literal["asset", "mesh", "parameter", "mesh_binding", "part", "transform", "scene_binding",
    "blend_key_table", "blend_constraint", "blend_binding", "glue", "offscreen", "cdi_parameter_group",
    "binding_axis", "binding_sample", "glue_vertex_pair", "draw_order_group", "cdi_combined_set", "cdi_combined_member",
    "model_parameter_group", "model_parameter_member", "model_hit_area", "attachment"] | modeling.ObjectKind
OBJECT_KINDS = frozenset(member.value for member in modeling.ObjectKind)

@dataclass(frozen=True)
class Capability:
    state: Literal["available", "unavailable", "unsupported"]
    code: str | None = None

    @classmethod
    def read(cls, raw: object) -> Capability:
        if raw == "Available":
            return cls("available")
        value = record(raw)
        for tag, state in (("Unavailable", "unavailable"), ("Unsupported", "unsupported")):
            if tag in value:
                return cls("unavailable" if state == "unavailable" else "unsupported", string(record(value[tag])["code"]))
        raise ValueError("Unknown capability state")

@dataclass(frozen=True)
class SceneCapabilities:
    inspect: Capability
    patch: Capability
    move_object: Capability
    reparent: Capability
    reorder: Capability
    duplicate: Capability
    delete: Capability
    dissolve: Capability

    @classmethod
    def read(cls, raw: object) -> SceneCapabilities:
        value = record(raw)
        return cls(*(Capability.read(value[key]) for key in
            ("inspect", "patch", "move_object", "reparent", "reorder", "duplicate", "delete", "dissolve")))

class ObjectRef(Owned):
    def __init__(self, backend: Backend, kind: ObjectKind, id: str):
        super().__init__(backend, "reference")
        self._object_kind: ObjectKind = kind
        self._id = id

    @property
    def id(self) -> str:
        return self._id

    @property
    def kind(self) -> ObjectKind:
        return self._object_kind

    @classmethod
    def create(cls, backend: Backend, session: object, kind: ObjectKind, id: str) -> ObjectRef:
        if kind not in OBJECT_KINDS or not isinstance(id, str):
            raise TypeError("Expected modeling object kind and ID")
        ref = cls(backend, kind, id)
        def load(raw: object) -> ObjectRef:
            ref._token = record(raw)["reference"]
            return ref
        return ref._request("capture", lambda: backend.object_ref(session, kind.value if isinstance(kind, modeling.ObjectKind) else kind, id, ref), load)

    def resolve(self) -> Source:
        self._check(self._backend)
        return self._request("resolve", lambda: self._backend.resolve_ref(self._token, self), Source.read)

    def capabilities(self, domain: modeling.CapabilityDomain) -> modeling.CapabilitySnapshot:
        self._check(self._backend)
        modeling.validate_CapabilityDomain(domain)
        return self._request("capabilities", lambda: self._backend.modeling_capabilities(self._token, domain, self), modeling.CapabilitySnapshot.read)

    def scene_capabilities(self) -> SceneCapabilities:
        self._check(self._backend)
        return self._request("capabilities", lambda: self._backend.ref_capabilities(self._token, self), SceneCapabilities.read)

    def label(self) -> LabelAddress:
        self._check(self._backend)
        return LabelAddress.create(self)

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
        return self._id

    @property
    def expected(self) -> str:
        return self._expected

    @classmethod
    def create(cls, reference: ObjectRef) -> LabelAddress:
        address = cls(reference._backend)
        def load(raw: object) -> LabelAddress:
            value = record(raw)
            address._token = value["address"]
            address._id = string(value["id"])
            address._expected = string(value["expected"])
            return address
        return address._request("capture", lambda: address._backend.label_address(reference._token, address), load)

    def plan(self, name: str) -> Plan:
        from .plans import Plan
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
    def create(cls, backend: Backend, session: object, selector: modeling.AddressSelector) -> FieldAddress:
        modeling.validate_AddressSelector(selector)
        address = cls(backend, "address")
        def load(raw: object) -> FieldAddress:
            address._token = record(raw)["address"]
            return address
        return address._request("capture", lambda: backend.capture_address(session, selector, address), load)

    def plan(self, patch: modeling.AddressPatch) -> Plan:
        from .plans import Plan
        self._check(self._backend)
        modeling.validate_AddressPatch(patch)
        plan = Plan(self._backend)
        return plan._request("plan", lambda: self._backend.address_plan(self._token, patch, plan), plan._load)

    def __enter__(self) -> FieldAddress:
        super().__enter__()
        return self

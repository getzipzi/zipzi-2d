"""Reviewed P3A destinations for the immutable pre-refactor surface.

This builds a review artefact, not importable SDK stubs or a second wire schema.
Domain field signatures remain the frozen Rust-derived declarations. Rules below
decide public ownership, result projections and lifecycle, with one explicit row
for every original public path. Unknown declarations fail closed.
"""
from __future__ import annotations

import ast
from collections import Counter
import dataclasses
import enum
import hashlib
import importlib
import json
from pathlib import Path
import re
import sys
from collections.abc import Mapping

ROOT = Path(__file__).resolve().parents[2]
DIRECTORY = ROOT / "target/python2/p3a-reports"
INVENTORY = DIRECTORY / "p3a-start-inventory.json"
INVENTORY_SHA256 = "68d2dc29c8075c9ed4e1679b8c54e130a4a0436aee6f210593bc4db8bd515f2e"

ROOT_TYPES = set("Buffer Busy ConnectionInfo ConnectionLimits Edit Job OutcomeUnknown PendingRequest Plan PreparedImage Receipt Runtime SaveReceipt SdkFailure Session SessionId SessionInfo Source Version WriteResult".split())
HANDLES = set("Buffer Edit Job Plan PreparedImage Runtime Session ImageLease ObjectRef LabelAddress FieldAddress SessionSubscription RegistrySubscription".split())
MODULES = set("bindings events meshes modeling ops parameters references resources sampling scene".split())
DOMAIN_TYPES = {
    "meshes": set("Geometry MeshSpec Point".split()),
    "parameters": set("Kind ParameterSpec".split()),
    "bindings": set("Appearance Axis AxisSnapshot Keyform KeyformSnapshot MeshBindingSpec Sample".split()),
    "resources": set("ImageLease ImagePixels Usage".split()),
    "sampling": set("BlendMode Checks Deformer Diagnostic DiagnosticKind Drawable Frame Offscreen Parameter Pose RenderCommand RenderKind".split()),
    "references": set("Capability FieldAddress LabelAddress ObjectKind ObjectRef SceneCapabilities".split()),
    "events": set("Closed Created Cursor Event Origin Published RegistrySubscription Saved SessionSubscription StateChanged".split()),
}
OPS = {"Create": "meshes.Create", "Rename": "meshes.Rename", "SetPositions": "meshes.SetPositions",
       "CreateImage": "resources.CreateImage", "ReplaceImage": "resources.ReplaceImage",
       "CreateParameter": "parameters.Create", "PatchParameter": "parameters.Patch",
       "CreateMeshBinding": "bindings.Create", "SetMeshKeyform": "bindings.SetKeyform"}
MOVED_TYPES = {"parameters.Patch": "parameters.ParameterPatch", "plans.Cost": "PlanCost",
               "plans.Report": "PlanReport", "meshes.RenameIntent": "meshes.Rename",
               "resources.RenameIntent": "resources.RenameImage"}
WRAPPERS = {
    "meshes.Snapshot": ("Snapshot", "meshes.Mesh"),
    "parameters.Snapshot": ("Snapshot", "parameters.Parameter"),
    "bindings.Snapshot": ("Snapshot", "bindings.MeshBinding"),
    "resources.Snapshot": ("Snapshot", "resources.Image"),
    "scene.Snapshot": ("Snapshot", "scene.Summary"),
    "parameters.Batch": ("Batch", "parameters.Parameter"),
    "bindings.Batch": ("Batch", "bindings.MeshBinding"),
    "scene.Batch": ("Batch", "scene.Summary"),
    "sampling.Batch": ("Snapshot", "sampling.Result"),
    "modeling.Records": ("Snapshot", "modeling.OwnedModelingRecords"),
    "modeling.CapabilitySnapshot": ("Snapshot", "modeling.Capabilities"),
    "modeling.EditorSnapshot": ("Snapshot", "modeling.EditorContext | None"),
}
INTERNAL_NAMES = set("annotations dataclass field Enum math Mapping MappingProxyType Sequence ClassVar TypeAlias NamedTuple cast Callable Generic Literal Self TypeVar T TYPE_CHECKING UUID Path Owned RecoveryOwner Resources OPERATION OPERATION_TYPES OBJECT_KINDS MAX_BUFFER_BYTES CHUNK Inputs".split())
CODECS = set("record sequence integer number string scalar points color flag rgba inputs wire validate read batch check_limit keyform_wire validate_keyform mesh_wire resource_wire validate_mesh validate_resource validate_intent".split())
RULES = {
    "core": "Public Runtime identities, receipts, exceptions and factories keep their semantic ownership.",
    "domain": "Existing domain input/value/enum and every Rust-derived field are preserved under their domain; generation replaces mechanical implementation.",
    "r4": "Preserve the already delivered canonical R4 Modeling/IO/address DTO and its exact Rust-derived fields; no new animation/GPU capability is inferred.",
    "operation": "One domain-owned operation serves direct apply and planning; remove parallel ops/intent export and internal wire discriminator.",
    "container": "Source is held once by Snapshot/Batch; payload fields remain owned pure domain values, including empty results.",
    "facade": "Retain the reachable domain entry, with generated query/write wrappers and one complete Call return projection.",
    "lifecycle": "Factory-created handwritten lifecycle wrapper; retain ownership, exact-request recovery and cached state.",
    "factory": "Backend/token constructors are private; the documented Runtime/Session factory creates the wrapper.",
    "codec": "Move conversion, validation and registration mechanics to internal generated modules; no domain capability is deleted.",
    "import": "Remove incidental dependency/re-export from this public namespace; canonical identity and domain capability remain available at the target when stated.",
    "recovery": "Recover through the owning OutcomeUnknown or registered recover_call; do not publish phase-dependent Edit.recover.",
}

# All signatures use public names; this is documentation/acceptance data, never
# executed as code. Explicit overloads are separate entries.
SIGNATURES = {
    "zipzi.local": "() -> Runtime",
    "zipzi.connect": "(*, endpoint: Path | None = None, start: bool = False, host: Path | None = None, timeout: float = 10., startup_timeout: float = 15., sdk_build: str | None = None) -> Runtime",
    "zipzi.recover_call": "(method: Callable[P, T], /, *args: P.args, **kwargs: P.kwargs) -> T",
    "zipzi.describe": "(path: str) -> ApiDescription",
    "zipzi.Runtime.capabilities": "(self) -> RuntimeCapabilities",
    "zipzi.Runtime.limits": "(self) -> RuntimeLimits",
    "zipzi.Runtime.close": "(self, *, abandon_pending: bool = False) -> None",
    "zipzi.Runtime.events_since": "(self, cursor: events.Cursor, *, limit: int = 128) -> tuple[events.Event, ...]",
    "zipzi.Runtime.sessions.create": "(self, document_id: str | None = None) -> Session",
    "zipzi.Runtime.sessions.attach": "(self, id: SessionId) -> Session",
    "zipzi.Runtime.sessions.list": "(self) -> tuple[SessionInfo, ...]",
    "zipzi.Runtime.sessions.open": "(self, path: Path) -> Job[Session]",
    "zipzi.Runtime.sessions.import_": "(self, path: Path, format: modeling.ImportFormat, *, textures: Sequence[modeling.TextureSlot] = ()) -> Job[Session]",
    "zipzi.Runtime.sessions.subscribe": "(self) -> events.RegistrySubscription",
    "zipzi.Runtime.resources.prepare_image": "(self, width: int, height: int, rgba: bytes | bytearray | memoryview | Buffer) -> PreparedImage",
    "zipzi.Runtime.resources.prepare_buffer": "(self, data: bytes | bytearray | memoryview) -> Buffer",
    "zipzi.Runtime.resources.load_png": "(self, path: Path) -> Job[PreparedImage]",
    "zipzi.Runtime.resources.prepare_png": "(self, buffer: Buffer) -> Job[PreparedImage]",
    "zipzi.Runtime.resources.usage": "(self) -> resources.Usage",
    "zipzi.Session.edit": "(self, label: str, *, expected: Source | None = None) -> Edit",
    "zipzi.Session.detach": "(self) -> None",
    "zipzi.Session.close": "(self, *, discard_modified: bool = False) -> SessionInfo",
    "zipzi.Session.info": "(self) -> SessionInfo",
    "zipzi.Session.show_window": "(self) -> None",
    "zipzi.Session.subscribe": "(self) -> events.SessionSubscription",
    "zipzi.Session.undo": "(self) -> Job[Receipt]",
    "zipzi.Session.redo": "(self) -> Job[Receipt]",
    "zipzi.Session.reference": "(self, kind: references.ObjectKind, id: str) -> references.ObjectRef",
    "zipzi.Session.address": "(self, selector: modeling.AddressSelector) -> references.FieldAddress",
    "zipzi.Session.capabilities": "(self, query: modeling.CollectionQuery) -> Snapshot[modeling.Capabilities]",
    "zipzi.Session.editor_context": "(self) -> Snapshot[modeling.EditorContext | None]",
    "zipzi.Session.project.save": "(self, path: Path) -> Job[SaveReceipt]",
    "zipzi.Session.project.run": "(self, request: modeling.ProjectRequest) -> Job[modeling.ProjectReport]",
    "zipzi.Session.project.diagnose": "(self) -> Job[modeling.ProjectReport]",
    "zipzi.Session.project.open": "(self, path: Path) -> Job[modeling.ProjectReport]",
    "zipzi.Session.project.replace": "(self, path: Path, format: modeling.ImportFormat, *, textures: Sequence[modeling.TextureSlot] = ()) -> Job[modeling.ProjectReport]",
    "zipzi.Session.project.export": "(self, path: Path, format: modeling.ExportFormat, *, name: str = 'model', object_id: str | None = None, allow_loss: bool = False, bake_animation: bool = False, auto_frame_rate: bool = False) -> Job[modeling.ProjectReport]",
    "zipzi.Edit.apply": ["(self, operation: ops.Operation) -> WriteResult", "(self, operation: Plan) -> WriteResult"],
    "zipzi.Edit.apply_many": "(self, operations: Sequence[ops.Operation | Plan]) -> WriteResult",
    "zipzi.Edit.commit": "(self) -> Job[Receipt]",
    "zipzi.Edit.cancel": "(self) -> None",
    "zipzi.Job.wait": "(self, *, timeout: float | None = None) -> T",
    "zipzi.Job.state": "(self) -> JobState",
    "zipzi.Job.cancel": "(self) -> None",
    "zipzi.Job.close": "(self) -> None",
    "zipzi.OutcomeUnknown.recover": "(self) -> object",
    "zipzi.Snapshot": "(source: Source, value: T) -> None",
    "zipzi.Batch": "(source: Source, items: Sequence[T]) -> None",
    "zipzi.Page": "(source: Source, items: Sequence[T], next_cursor: PageCursor | None) -> None",
    "zipzi.WriteResult": "(source: Source, ids: Sequence[str]) -> None",
    "zipzi.meshes.Geometry": "(vertex_ids: Sequence[int], positions: Sequence[meshes.Point], uvs: Sequence[meshes.Point], triangles: Sequence[tuple[int, int, int]]) -> None",
    "zipzi.meshes.Rename": "(id: str, name: str, *, expected_name: str | None = None) -> None",
    "zipzi.resources.RenameImage": "(id: str, name: str, *, expected_name: str | None = None) -> None",
}
ANNOTATIONS = {
    "zipzi.Edit.state": "EditState", "zipzi.Edit.receipt": "Receipt | None",
    "zipzi.Edit.pending_request": "PendingRequest | None", "zipzi.Job.pending_request": "PendingRequest | None",
    "zipzi.Runtime.connection": "ConnectionInfo | None", "zipzi.Session.id": "SessionId",
    "zipzi.Snapshot.source": "Source", "zipzi.Snapshot.value": "T", "zipzi.Batch.source": "Source",
    "zipzi.Batch.items": "tuple[T, ...]", "zipzi.Page.source": "Source", "zipzi.Page.items": "tuple[T, ...]",
    "zipzi.Page.next_cursor": "PageCursor | None",
}
for error_type in ("SdkFailure", "Busy", "OutcomeUnknown", "JobTimeout"):
    for name, annotation in {
        "code": "str", "operation": "str", "object_ids": "tuple[str, ...]",
        "expected_version": "Version | None", "actual_version": "Version | None",
        "field_path": "str | None", "referrers": "tuple[str, ...]", "edit_state": "str | None",
        "project_report": "modeling.ProjectReport | None", "cleanup": "OutcomeUnknown | None",
        "recovery_action": "RecoveryAction", "requested": "int | None", "allowed": "int | None", "limit_name": "str | None",
    }.items():
        ANNOTATIONS[f"zipzi.{error_type}.{name}"] = annotation
ANNOTATIONS["zipzi.OutcomeUnknown.pending"] = "PendingRequest"
for wrapper in ("Buffer", "PreparedImage", "Plan", "resources.ImageLease", "references.ObjectRef", "references.LabelAddress", "references.FieldAddress"):
    ANNOTATIONS[f"zipzi.{wrapper}.pending_request"] = "PendingRequest | None"
SIGNATURES.update({
    "zipzi.SdkFailure": "(code: str, message: str, *, operation: str = '', object_ids: tuple[str, ...] = (), expected_version: Version | None = None, actual_version: Version | None = None, field_path: str | None = None, referrers: tuple[str, ...] = (), edit_state: str | None = None, project_report: modeling.ProjectReport | None = None, recovery_action: RecoveryAction | None = None, requested: int | None = None, allowed: int | None = None, limit_name: str | None = None) -> None",
    "zipzi.OutcomeUnknown": "(pending: PendingRequest, message: str) -> None",
    "zipzi.JobTimeout": "(message: str = 'Accepted Job is still pending') -> None",
    "zipzi.Plan.report": "(self) -> PlanReport",
    "zipzi.PreparedImage.pixels": "(self) -> resources.ImagePixels",
})
# Generic and new metadata types are frozen as data here. Runtime limit names
# come directly from the current shared Rust config, not guessed Python limits.
TYPE_DEFINITIONS = {
    "zipzi.EditState": "Enum(editing, committing, committed, cancelled, aborted, failed, unknown)",
    "zipzi.JobState": "Literal['queued', 'running', 'cancelling', 'succeeded', 'failed', 'cancelled']",
    "zipzi.RecoveryAction": "Literal['retry', 'reduce_request', 'release_resources', 'recover_original', 'restart_edit', 'refresh_source', 'resync', 'wait_job', 'stop']",
    "zipzi.ops.Operation": "meshes.Create | meshes.Rename | meshes.SetPositions | parameters.Create | parameters.Patch | bindings.Create | bindings.SetKeyform | resources.CreateImage | resources.ReplaceImage | resources.RenameImage | scene.Operation | modeling.ModelingCommand",
    "zipzi.ops.PlannableOperation": "meshes.Rename | resources.RenameImage | scene.Operation | modeling.ModelingCommandScene | modeling.ModelingCommandOrganization | modeling.ModelingCommandBinding | modeling.ModelingCommandContent | modeling.ModelingCommandReferences | modeling.ModelingCommandResource | modeling.ModelingCommandDocument | modeling.ModelingCommandAttachment",
    "zipzi.RuntimeCapabilities": "implemented: tuple[str, ...]; conditions: tuple[CapabilityCondition, ...]",
    "zipzi.CapabilityCondition": "operation: str; available: bool; reason: str | None",
    "zipzi.RuntimeLimits": "runtime: Mapping[str, int]; protocol: ConnectionLimits | None; query: Mapping[str, int]",
    "zipzi.PageCursor": "opaque immutable value; library-created; exact source/filter/order identity; no public token/constructor",
    "zipzi.ApiDescription": "path: str; signatures: tuple[str, ...]; parameters: tuple[ParameterDescription, ...]; returns: str; effects: str; lifecycle: str; recovery: str; capabilities: tuple[str, ...]; examples: tuple[str, ...]",
    "zipzi.ParameterDescription": "name: str; annotation: str; semantics: str; default: str | None",
    "zipzi.Snapshot": "Generic[T]; source: Source; value: T; immutable",
    "zipzi.Batch": "Generic[T]; source: Source; items: tuple[T, ...]; immutable",
    "zipzi.Page": "Generic[T]; source: Source; items: tuple[T, ...]; next_cursor: PageCursor | None; immutable",
    "zipzi.sampling.Result": "state: str; version: Version; parameters: tuple[sampling.Pose, ...]; frames: tuple[sampling.Frame, ...]; diagnostics: tuple[sampling.Diagnostic, ...]",
}
for owner in ("Session", "Edit"):
    SIGNATURES[f"zipzi.{owner}.plan"] = "(self, operation: ops.PlannableOperation) -> Plan"
    # Async planning remains cancellable and captures the same exact source.
    SIGNATURES[f"zipzi.{owner}.plan_async"] = "(self, operation: ops.PlannableOperation) -> Job[Plan]"
    for domain, payload in (("meshes", "Mesh"), ("parameters", "Parameter"), ("bindings", "MeshBinding"),
                            ("resources", "Image"), ("scene", "Summary")):
        prefix = f"zipzi.{owner}.{domain}"
        SIGNATURES[prefix + ".get"] = f"(self, id: str, *, expected: Source | None = None) -> Snapshot[{domain}.{payload}]"
        SIGNATURES[prefix + ".get_many"] = f"(self, ids: Sequence[str], *, expected: Source | None = None) -> Batch[{domain}.{payload}]"
        SIGNATURES[prefix + ".list"] = f"(self, *, name: str | None = None, limit: int = 128, cursor: PageCursor | None = None) -> Page[{domain}.Summary]"
    SIGNATURES[f"zipzi.{owner}.resources.snapshot"] = "(self, id: str) -> resources.ImageLease"
    SIGNATURES[f"zipzi.{owner}.modeling.records"] = "(self, query: modeling.ModelingQuery) -> Snapshot[modeling.OwnedModelingRecords]"
    for name, result in (("sample", "Snapshot[sampling.Result]"), ("sample_async", "Job[Snapshot[sampling.Result]]")):
        SIGNATURES[f"zipzi.{owner}.modeling.{name}"] = f"(self, poses: Sequence[Mapping[str, float]], *, checks: sampling.Checks = sampling.Checks(), expected: Source | None = None) -> {result}"
for path, signature in {
    "meshes.create": "(self, spec: meshes.MeshSpec) -> str",
    "meshes.rename": "(self, id: str, name: str, *, expected_name: str | None = None) -> WriteResult",
    "meshes.set_positions": "(self, id: str, *, vertex_ids: Sequence[int], positions: Sequence[meshes.Point]) -> WriteResult",
    "parameters.create": "(self, spec: parameters.ParameterSpec) -> str",
    "parameters.patch": "(self, id: str, patch: parameters.ParameterPatch) -> WriteResult",
    "bindings.create": "(self, spec: bindings.MeshBindingSpec) -> str",
    "bindings.set_keyform": "(self, id: str, keyform: bindings.Keyform) -> WriteResult",
    "resources.create_image": "(self, name: str, image: PreparedImage, *, id: str | None = None) -> str",
    "resources.replace_image": "(self, id: str, image: PreparedImage) -> WriteResult",
    "scene.create_part": "(self, spec: scene.PartSpec) -> str",
    "scene.create_parameter_group": "(self, spec: scene.ParameterGroupSpec) -> str",
}.items():
    SIGNATURES[f"zipzi.Edit.{path}"] = signature


def normalize(text: str) -> str:
    text = text.replace("~T", "T").replace("collections.abc.", "")
    text = re.sub(r"ForwardRef\('([^']+)'\)", r"\1", text)
    text = re.sub(r"zipzi\.(?:_lifecycle|_values|_errors)\.", "", text)
    for old, (container, payload) in WRAPPERS.items():
        text = text.replace("zipzi." + old, f"{container}[{payload}]")
    for old, new in {**MOVED_TYPES, **{"ops." + a: b for a, b in OPS.items()}}.items():
        text = text.replace("zipzi." + old, new)
    return text.replace("zipzi.", "")


def default_expression(value: object) -> str:
    if isinstance(value, enum.Enum):
        return normalize(type(value).__module__ + "." + type(value).__name__) + "." + value.name
    if dataclasses.is_dataclass(value):
        return normalize(type(value).__module__ + "." + type(value).__name__) + "(" + ", ".join(
            f"{field.name}={default_expression(getattr(value, field.name))}"
            for field in dataclasses.fields(value) if field.init and not field.name.startswith("_")) + ")"
    if isinstance(value, Mapping):
        return "{" + ", ".join(f"{default_expression(k)}: {default_expression(v)}" for k, v in value.items()) + "}"
    if isinstance(value, (tuple, list)):
        return "(" + ", ".join(default_expression(v) for v in value) + ("," if len(value) == 1 else "") + ")"
    if value is None or isinstance(value, (str, int, float, bytes, bool)):
        return repr(value)
    raise ValueError(f"Unsupported pure SDK default: {type(value)}")


def constructor_signature(original: dict, signature: str) -> str:
    """Turn dataclass factory sentinels into their actual pure Rust defaults.

    Public __module__ rewrites in the old generator exposed ClassVar constructor
    arguments; they are registration details and are removed from the target.
    This only constructs small pure default values, never a lifecycle wrapper.
    """
    if "<factory>" not in signature and "ClassVar" not in signature:
        return signature
    path = original["path"].removesuffix(".__init__")
    module, name = path.rsplit(".", 1)
    sys.path.insert(0, str(ROOT / "crates/zipzi-python2/client/python"))
    cls = getattr(importlib.import_module(module), name)
    defaults = {f.name: f.default_factory for f in dataclasses.fields(cls)
                if f.default_factory is not dataclasses.MISSING}
    tree = ast.parse("def target" + signature.replace("<factory>", "_sdk_default") + ":\n    pass\n")
    function = tree.body[0]
    arguments = function.args
    kept_args, kept_defaults = [], []
    for arg, default in zip(arguments.kwonlyargs, arguments.kw_defaults):
        if arg.arg.startswith("_") or "ClassVar" in ast.unparse(arg.annotation):
            continue
        if isinstance(default, ast.Name) and default.id == "_sdk_default":
            default = ast.parse(default_expression(defaults[arg.arg]()), mode="eval").body
        kept_args.append(arg)
        kept_defaults.append(default)
    arguments.kwonlyargs, arguments.kw_defaults = kept_args, kept_defaults
    return ast.unparse(function).removeprefix("def target").split(":\n", 1)[0]


def destination(row: dict, exports: dict[str, dict], enum_types: set[str]) -> tuple[str | None, str, str]:
    path = row["path"]
    if row["kind"] == "missing_export":
        raise ValueError(f"Missing source export: {path}")
    if path in SIGNATURES or path in ANNOTATIONS:
        return path, "replace", "lifecycle" if path.startswith(("zipzi.Edit", "zipzi.Job", "zipzi.Runtime", "zipzi.Session")) else "core"
    if row["kind"] == "facade":
        return path, "retain", "facade"
    # A facade's methods are public paths even though their class isn't exported.
    for owner in ("Runtime", "Session", "Edit"):
        if path.startswith(f"zipzi.{owner}.") and path not in exports:
            name = path.rsplit(".", 1)[-1]
            if name == "__init__":
                return None, "remove", "factory"
            if path == "zipzi.Edit.recover":
                return None, "remove", "recovery"
            if path == "zipzi.Edit.apply_plan":
                return "zipzi.Edit.apply", "replace", "operation"
            if name in {"plan", "plan_async"}:
                return f"zipzi.{owner}.{name}", "move", "operation"
            return path, "retain", "lifecycle"
    # Find the containing exported declaration; nested public fields/methods
    # follow their canonical type, not the module they leaked through.
    parent = path
    while parent not in exports and "." in parent:
        parent = parent.rsplit(".", 1)[0]
    if parent not in exports:
        raise ValueError(f"No reviewed declaration owns {path}")
    suffix = path[len(parent):]
    key = parent.removeprefix("zipzi.")
    parts = key.split(".")
    name, module = parts[-1], parts[0] if len(parts) > 1 else ""
    if key == "Edit.recover":
        return None, "remove", "recovery"
    # Backend/codec methods on otherwise retained public classes stay internal.
    if row["kind"] == "method" and (suffix in {".wire", ".read", ".prepare", ".prepare_buffer", ".from_info", ".create", ".create_scene", ".create_modeling", ".capture", ".start"}):
        if not (name == "Buffer" and suffix == ".read"):
            return None, "remove", "codec"
    if suffix == ".__init__" and name in HANDLES:
        return None, "remove", "factory"
    if suffix == ".kind" and (module == "ops" or name == "RenameIntent" or name.startswith("ModelingCommand")):
        return None, "remove", "codec"
    if key in WRAPPERS:
        container, payload = WRAPPERS[key]
        if suffix in {"", ".__init__"}:
            return "zipzi." + container + suffix, "replace", "container"
        if suffix == ".source":
            return "zipzi." + container + ".source", "replace", "container"
        if container == "Batch" or suffix == ".value":
            return "zipzi." + container + suffix, "replace", "container"
        if key.startswith("modeling.") and suffix in {".values", ".context"}:
            return "zipzi.Snapshot.value", "replace", "container"
        return "zipzi." + payload + suffix, "move", "container"
    if key in MOVED_TYPES:
        return "zipzi." + MOVED_TYPES[key] + suffix, "move", "operation" if name == "RenameIntent" else "domain"
    if module == "ops":
        if name in OPS:
            return "zipzi." + OPS[name] + suffix, "move", "operation"
        if name == "Operation":
            return parent + suffix, "retain", "operation"
        if name == "ApplyPlan":
            return None, "remove", "operation"
        return None, "remove", "codec" if name in CODECS else "import"
    if name in ROOT_TYPES:
        return "zipzi." + name + suffix, "retain" if not module else "move", "core"
    if not module:
        if name in MODULES | {"local", "connect"}:
            return parent + suffix, "retain", "core"
        return None, "remove", "import"
    if name in INTERNAL_NAMES or name in CODECS or name.startswith(("read_", "encode_", "validate_")):
        return None, "remove", "codec" if name in CODECS or "_" in name and name.startswith(("read_", "encode_", "validate_")) else "import"
    if name == "Point":
        return "zipzi.meshes.Point" + suffix, "move", "import"
    if key == "plans.Intent":
        return "zipzi.ops.PlannableOperation" + suffix, "replace", "operation"
    if module == "scene":
        # These are the SDK-selected scene values/operations; helpers above have
        # already been removed. The finite frozen declaration list is audited.
        return parent + suffix, "retain", "domain"
    if module == "modeling":
        if name == "ModelingIntent":
            return "zipzi.ops.PlannableOperation" + suffix, "replace", "operation"
        if name.startswith("ModelingIntent"):
            target = "zipzi.modeling.ModelingCommand" + name.removeprefix("ModelingIntent")
            return target + (".intent" if suffix == ".value" else suffix), "replace", "operation"
        if name[:1].isupper() and (exports[parent].get("implementation", "").startswith(("zipzi.", "typing.Union", "typing._Union", "types.Union"))):
            return parent + suffix, "retain", "r4"
        raise ValueError(f"Unreviewed Modeling declaration: {path}")
    if name in DOMAIN_TYPES.get(module, set()):
        return parent + suffix, "retain", "domain"
    if name[:1].islower():  # re-exported modules, never extra public namespaces
        return None, "remove", "import"
    raise ValueError(f"Unreviewed domain declaration: {path}")


def build() -> dict:
    raw = INVENTORY.read_bytes()
    if hashlib.sha256(raw).hexdigest() != INVENTORY_SHA256:
        raise ValueError("The original frozen inventory changed")
    frozen = json.loads(raw)
    exports = {r["path"]: r for r in frozen["symbols"] if r["kind"] == "export"}
    # New pure payload constructors preserve every old field except the source,
    # which belongs to the generic wrapper. They are full signatures too.
    for old, new in (("meshes.Snapshot", "meshes.Mesh"), ("parameters.Snapshot", "parameters.Parameter"),
                     ("bindings.Snapshot", "bindings.MeshBinding"), ("resources.Snapshot", "resources.Image")):
        signature = normalize(exports["zipzi." + old]["signature"])
        function = ast.parse("def target" + signature + ":\n    pass\n").body[0]
        arguments = function.args
        if not arguments.args or arguments.args[0].arg != "source":
            raise ValueError(f"Review source/payload constructor: {old}")
        arguments.args.pop(0)
        SIGNATURES["zipzi." + new] = ast.unparse(function).removeprefix("def target").split(":\n", 1)[0]
    SIGNATURES.update({
        "zipzi.sampling.Result": "(state: str, version: Version, parameters: Sequence[sampling.Pose], frames: Sequence[sampling.Frame], diagnostics: Sequence[sampling.Diagnostic]) -> None",
        "zipzi.meshes.Summary": "(id: str, name: str, vertex_count: int, triangle_count: int, part_id: str | None, deformer_id: str | None, texture_asset_id: str | None) -> None",
        "zipzi.parameters.Summary": "(id: str, name: str, runtime_id: str, kind: parameters.Kind) -> None",
        "zipzi.bindings.Summary": "(id: str, mesh_id: str, axis_count: int, keyform_count: int, name: str | None = None) -> None",
        "zipzi.resources.Summary": "(id: str, name: str, width: int, height: int) -> None",
    })
    enums = {r["path"].rsplit(".", 1)[0] for r in frozen["symbols"] if r["kind"] == "enum_member"}
    mapping = []
    for original in frozen["symbols"]:
        target, action, rule = destination(original, exports, enums)
        row = {"source": original["path"], "source_kind": original["kind"], "decision": action,
               "target": target, "rule": rule, "reason": RULES[rule]}
        if target is not None:
            if target in SIGNATURES:
                row["signatures"] = SIGNATURES[target] if isinstance(SIGNATURES[target], list) else [SIGNATURES[target]]
            elif target.endswith(".__init__") and target.removesuffix(".__init__") in SIGNATURES:
                signature = SIGNATURES[target.removesuffix(".__init__")]
                if isinstance(signature, str):
                    row["signatures"] = ["(self, " + signature[1:]]
            elif target in exports and exports[target].get("signature"):
                row["signatures"] = [constructor_signature(exports[target], normalize(exports[target]["signature"]))]
            elif original.get("signature"):
                row["signatures"] = [constructor_signature(original, normalize(original["signature"]))]
            elif original.get("signature_error") and original["kind"] == "method":
                raise ValueError(f"No target signature for {original['path']}")
            if target in ANNOTATIONS:
                row["annotation"] = ANNOTATIONS[target]
            elif original.get("annotation"):
                row["annotation"] = normalize(original["annotation"])
            if original["path"].removeprefix("zipzi.") in WRAPPERS:
                container, payload = WRAPPERS[original["path"].removeprefix("zipzi.")]
                row["result_type"] = f"{container}[{payload}]"
            if original["kind"] in {"export", "facade"} and (original["path"].rsplit(".", 1)[-1] in HANDLES or original["kind"] == "facade"):
                row.pop("signatures", None)
                row["construction"] = "documented factory only"
            if original["path"] in enums:
                values = [r["value"] for r in frozen["symbols"] if r["kind"] == "enum_member" and r["path"].rsplit(".", 1)[0] == original["path"]]
                scalar = "str" if all(isinstance(v, str) for v in values) else "int"
                row["signatures"] = [f"(value: {scalar}) -> {target.removeprefix('zipzi.')}"]
        mapping.append(row)
    # Explicit declarations augment the old surface with every new entrypoint.
    targets = [{"path": path, "signatures": signature if isinstance(signature, list) else [signature]}
               for path, signature in sorted(SIGNATURES.items())]
    targets += [{"path": path, "annotation": annotation} for path, annotation in sorted(ANNOTATIONS.items())]
    return {"schema": 1, "status": "contract_frozen_implementation_pending",
            "source_inventory_sha256": INVENTORY_SHA256, "rules": RULES,
            "decisions": dict(Counter(r["decision"] for r in mapping)),
            "mapping": mapping, "new_and_changed_targets": targets, "type_definitions": TYPE_DEFINITIONS}


def validate(catalogue: dict) -> None:
    canonical = build()
    frozen = json.loads(INVENTORY.read_text())
    expected = {r["path"] for r in frozen["symbols"]}
    rows = catalogue["mapping"]
    if len(rows) != len(expected) or {r["source"] for r in rows} != expected:
        raise ValueError("Every original symbol needs exactly one target decision")
    for row in rows:
        if row["decision"] not in {"retain", "move", "replace", "remove"} or not row.get("reason"):
            raise ValueError(f"Unresolved decision: {row['source']}")
        if (row["decision"] == "remove") != (row["target"] is None):
            raise ValueError(f"Missing destination/removal reason: {row['source']}")
        if row["rule"] not in RULES or row["reason"] != RULES[row["rule"]]:
            raise ValueError(f"Unregistered semantic rule: {row['source']}")
        if row["target"] and row["source_kind"] == "method" and not row.get("signatures"):
            raise ValueError(f"Missing callable signature: {row['source']}")
        if row.get("annotation") == "inspect constructor/assignment":
            raise ValueError(f"Unresolved target field type: {row['source']}")
        if row["source"].startswith("zipzi.modeling.ModelingCommand") and row["source"].count(".") == 2 and row["decision"] == "remove":
            raise ValueError("Existing R4 authoring capability cannot be removed")
    for row in rows + catalogue["new_and_changed_targets"]:
        for signature in row.get("signatures", []):
            if "Backend" in signature or "Any" in signature or "type: ignore" in signature:
                raise ValueError(f"Internal or untyped public signature: {row}")
            try:
                ast.parse("def target" + signature + ":\n    pass\n")
            except SyntaxError as error:
                raise ValueError(f"Invalid full signature: {row}") from error
    actual = {r["path"]: r for r in catalogue["new_and_changed_targets"] if "signatures" in r}
    if set(actual) != set(SIGNATURES):
        raise ValueError("Missing or extra target method signatures")
    if any(actual[path]["signatures"] != (value if isinstance(value, list) else [value])
           for path, value in SIGNATURES.items()):
        raise ValueError("Target signatures drifted from the frozen contract")
    if catalogue != canonical:
        raise ValueError("Per-symbol destination/type/behavior drifted from the reviewed rules")

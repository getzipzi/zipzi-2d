# Selected capability scope

This list is a delivery checklist for a new package. It does not generate API
signatures or define another wire schema. Domain behavior is checked against
fixed geometry/history and the independent Rust SDK tests.

| Capability | Public entry | Shared typed Rust route | Evidence | State |
| --- | --- | --- | --- | --- |
| Mesh generation | Same public values/ops and query methods | Selected Rust declarations + compiled query registration | Deterministic generation/check-only/protection, real Rust negative compilation, unchanged behavior/static tests | First P2 slice implemented |
| Local ownership | `local()` / Runtime context | `host::LocalHost` | `tests/test_api.py::test_shared_recipe`; Runtime host tests | Implemented |
| Explicit connection | `connect(endpoint=...)` | `Protocol` → same Runtime handlers | Shared recipe on real subprocess host | Implemented |
| Session creation/info | `runtime.sessions.create`, `session.info` | `Runtime::create_session/session_info` | Shared recipe, response-loss tests | Implemented |
| Create | `edit.meshes.create` / `ops.Create` | `Runtime::apply_authoring` → SDK `MeshCreateSpec` | SDK `mesh_create_spec_allocates_identity_and_keeps_defaults_in_sdk`; shared recipe | Implemented |
| Rename/positions | Convenience methods / typed operation values | `Runtime::apply_authoring` → SDK `MeshOperation` | Candidate read-after-write, independent vertex IDs/positions | Implemented |
| Ordered batch | `edit.apply_many` | Same `Runtime::apply_authoring` | Create with explicit ID then rename; middle failure and admission rejection | Implemented |
| Immutable query | `session/edit.meshes.get` | `Runtime::mesh_snapshot` | Source relationship, fixed geometry, frozen values; Runtime budget/session tests | Implemented |
| Commit/Undo/Redo | Edit context, explicit Jobs | Runtime request/completion routes | One publication, no-op/cancel preserves redo | Implemented |
| Save/open | Project/Sessions Jobs | `request_save/request_open` | Save, close host, reopen and compare geometry/history | Implemented |
| Error/recovery ownership | Pure Python errors, `recover()` | Protocol request retention before shared handler | Apply/commit/Job/read/start/cleanup loss; foreign recovery owner refused | Implemented |
| Job lifecycle | `state/cancel/wait/close` | Runtime Job table + owner completion | Queued/entered IO cancellation, real IO and worker failures, retained-slot budget, exact recovery owner | Selected P1 implemented |
| Explicit client close | Runtime close, same-ID acknowledgment recovery | `release_client` / LocalHost shutdown | Immediate client lease cleanup, formal preservation, entered IO finishes; last native owner releases lock | Selected P1 implemented |
| Native blocking/lifetime | Job wait, close, destruction | Local owner + CPU workers | Native waits, idempotent close, Rust completion/last-owner tests | Implemented |
| Import without native | Values, errors, Connected backend | Pure Python | Isolated process without extension | Implemented |
| PreparedImage | `runtime.resources.prepare_image`, `pixels`, `close`/context | Client-owned Runtime image table + SDK preparation | A/B independence; 100 cancelled sources reclaim actual Rust bytes; response-loss recovery; both installed wheels | Selected P1 implemented; 1 MiB limit |
| Image create/replace/query | `edit.resources.create_image/replace_image`, `session/edit.resources.get`, typed ops | `Runtime::apply_authoring/image_snapshot` → SDK image methods | Fixed pixels, immutable snapshots, mixed mesh/resource batch, Undo/Redo, save/reopen | Selected P1 implemented |
| Formal/candidate Plan | Typed rename intents, cached report, `edit.apply_plan` / `ops.ApplyPlan` | Source-bound Runtime plans through `apply_authoring` | Exact source, stale/foreign/closed refusal, batch rollback, report after close, same-ID recovery | Selected P1 implemented; P3 expands |
| Binary buffers/large images | Explicit bounded buffers | Shared SDK/Runtime resource machinery | Independent binary/peak/cancellation coverage required | Planned P4 |
| Scene/parameters/bindings/topology | Selected domain modules | SDK semantics with Runtime ownership | Independent recipes to be written | Planned P3 |
| Complete project formats/binary IO | Resource/Project Jobs | Shared SDK/Runtime | Resource and filesystem failure cases required | Planned P4 |
| CPU sampling/animation/Preview | Typed query and independent sources | Shared SDK/Runtime | Numerical and isolation recipes required | Planned P5 |
| Observer/inspection | Device-backed observer, offline workflow | Rust observation + Python artifact organization | Conditional GPU and offline evidence required | Planned P5 |
| Host discovery and packaged acceptance | Explicit connection lifecycle, both distributions | Same host ownership/protocol | Existing endpoint entry is insufficient | Planned P6 |

P0 has selected the first inputs/results and failure rules: `MeshSpec` creates
without copying a persisted Mesh; Rename/SetPositions express intent; snapshots
contain owned immutable geometry and Source; commit/save/open return explicit
Jobs; failed writes cannot commit; unknown outcomes retain the original owner.
Coordinates are finite f32, IDs are u32, array order is explicit, and all native
input storage is owned before detaching from Python.

The selected P1 slice has a completed lifecycle acceptance matrix. P0 now records
both backends with separate timing/probe binaries and frozen comparison gates;
its explicit copy and requested-heap scopes are documented. Later domain, binary,
CPU/GPU and platform work remains in its own phases. Implemented source APIs and installed-package acceptance are tracked
separately. New capabilities require explicit handlers and independent evidence;
there are no generated unsupported-operation placeholders.

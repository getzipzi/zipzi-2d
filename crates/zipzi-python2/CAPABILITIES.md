# Selected capability scope

This list is a delivery checklist for a new package. It does not generate API
signatures or define another wire schema. Domain behavior is checked against
fixed geometry/history and the independent Rust SDK tests.

| Capability | Public entry | Shared typed Rust route | Evidence | State |
| --- | --- | --- | --- | --- |
| Local ownership | `local()` / Runtime context | `host::LocalHost` | `tests/test_api.py::test_shared_recipe`; Runtime host tests | Implemented |
| Explicit connection | `connect(endpoint=...)` | `Protocol` → same Runtime handlers | Shared recipe on real subprocess host | Implemented |
| Session creation/info | `runtime.sessions.create`, `session.info` | `Runtime::create_session/session_info` | Shared recipe, response-loss tests | Implemented |
| Create | `edit.meshes.create` / `ops.Create` | `Runtime::edit_meshes` → SDK `MeshCreateSpec` | SDK `mesh_create_spec_allocates_identity_and_keeps_defaults_in_sdk`; shared recipe | Implemented |
| Rename/positions | Convenience methods / typed operation values | `Runtime::edit_meshes` → SDK `MeshOperation` | Candidate read-after-write, independent vertex IDs/positions | Implemented |
| Ordered batch | `edit.apply_many` | Same `Runtime::edit_meshes` | Create with explicit ID then rename; middle failure and admission rejection | Implemented |
| Immutable query | `session/edit.meshes.get` | `Runtime::mesh_snapshot` | Source relationship, fixed geometry, frozen values; Runtime budget/session tests | Implemented |
| Commit/Undo/Redo | Edit context, explicit Jobs | Runtime request/completion routes | One publication, no-op/cancel preserves redo | Implemented |
| Save/open | Project/Sessions Jobs | `request_save/request_open` | Save, close host, reopen and compare geometry/history | Implemented |
| Error/recovery ownership | Pure Python errors, `recover()` | Protocol request retention before shared handler | Apply/commit/Job/read/start/cleanup loss; foreign recovery owner refused | Implemented |
| Native blocking/lifetime | Job wait, close, destruction | Local owner + CPU workers | Native waits, idempotent close, Rust completion/last-owner tests | Implemented |
| Import without native | Values, errors, Connected backend | Pure Python | Isolated process without extension | Implemented |
| PreparedImage/Buffer | Resource values + explicit leases | Existing SDK/Runtime resource machinery | Rust resource regressions exist; Python route pending | Planned P1/P4 |
| Formal/candidate Plan | Typed intent, report and owned plan | Existing source-bound Runtime plans | Rust plan regressions exist; Python route pending | Planned P1/P3 |
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

P0/P1 are not complete while their planned resource/plan routes and measurement
gaps remain. Implemented source APIs and installed-package acceptance are tracked
separately. New capabilities require explicit handlers and independent evidence;
there are no generated unsupported-operation placeholders.

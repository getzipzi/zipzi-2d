# Capability scope

This list is a delivery checklist for a new package. It does not generate API
signatures or define another wire schema. Domain behavior is checked against
fixed geometry/history and the independent Rust SDK tests.

| Capability | Public entry | Shared typed Rust route | Evidence | State |
| --- | --- | --- | --- | --- |
| Mesh generation | Same public values/ops and query methods | Selected Rust declarations + compiled query registration | Deterministic generation/check-only/protection, real Rust negative compilation, unchanged behavior/static tests | P2 mesh slice implemented |
| Resource/Plan generation | Image values/usage, typed resource ops and rename intents, Plan report/cost, resource conveniences | Actual Rust declarations and compiled image/pixel/Plan registrations | Finite bool/u64/usize/bytes/string-map shapes, type propagation, direct native conversion and independent recipes | P2 selected slice implemented |
| Local ownership | `local()` / Runtime context | `host::LocalHost` | `tests/test_api.py::test_shared_recipe`; Runtime host tests | Implemented |
| Host discovery/start | `connect()`, `connect(start=True, host=...)`, `runtime.connection` | Same endpoint ownership/inherited fd and strict protocol/build handshake | Installed subprocess cold-start/slow/failure/security cases; current production Editor cold start/reopen | Selected Runtime R3 slice implemented; packaged app paths pending |
| Explicit connection | `connect(endpoint=...)` | `Protocol` → same Runtime handlers | Shared recipe on real subprocess host | Implemented |
| Resident sessions | `sessions.list/attach`, `session.id`, `session.show_window` | Runtime session registry and desktop window route | Both backends; lost list/attach responses; two installed clients share unsaved history; native Editor projection/UI Undo/Metal | Selected Runtime R3 slice implemented |
| Parameter authoring/query | `parameters.ParameterSpec/Patch`, typed ops, `session/edit.parameters.get/get_many/list` | SDK create/patch, shared Runtime batch and source-bounded query | Defaults/identity/field retention, mixed history, budget/read isolation, installed response losses and save/reopen | Implemented; full canonical records cover groups/bindings |
| Typed events | `session.subscribe`, `sessions.subscribe`, `runtime.events_since`, `poll/resync` | Same Runtime metadata subscription and bounded event pages | Independent readers, exact cursor, explicit resync, lost creation/poll, native UI Undo on another client | R3 minimum implemented |
| Mesh batch reads | `session/edit.meshes.get_many` | Whole-selection Runtime admission and selected shared mesh conversion | Source/order/duplicates, total-byte refusal before copies, same-ID recovery, installed recipe | R3 minimum implemented |
| Session creation/info | `runtime.sessions.create`, `session.info` | `Runtime::create_session/session_info` | Shared recipe, response-loss tests | Implemented |
| Create | `edit.meshes.create` / `meshes.Create` | `Runtime::apply_authoring` → SDK `MeshCreateSpec` | SDK `mesh_create_spec_allocates_identity_and_keeps_defaults_in_sdk`; shared recipe | Implemented |
| Rename/positions | Convenience methods / typed operation values | `Runtime::apply_authoring` → SDK `MeshOperation` | Candidate read-after-write, independent vertex IDs/positions | Implemented |
| Ordered batch | `edit.apply_many` | Same `Runtime::apply_authoring` | Create with explicit ID then rename; middle failure and admission rejection | Implemented |
| Immutable query | `session/edit.meshes.get` | `Runtime::mesh_snapshot` | Source relationship, fixed geometry, frozen values; Runtime budget/session tests | Implemented |
| Commit/Undo/Redo | Edit context, explicit Jobs | Runtime request/completion routes | One publication, no-op/cancel preserves redo | Implemented |
| Save/open | Project/Sessions Jobs | `request_save/request_open` | Save, close host, reopen and compare geometry/history | Implemented |
| Complete-call recovery | `OutcomeUnknown.recover`, typed `recover_call`, cached Edit state/receipt | Protocol retains original request; common Python Call applies the complete return projection | Four create IDs, scene get, commit Job, wait/close, repeated loss, old-exception guard and independent owner tests | P3A-1 accepted; final performance noise waived by user |
| Known Job timeout | `job.wait(timeout=...)`, `JobTimeout` | Native/Connected nonterminal observation; real wire loss stays unknown | Both backends, no fake request 0, no implicit cancellation, cached result/receipt | P3A-1 accepted |
| Job lifecycle | `state/cancel/wait/close` | Runtime Job table + owner completion | Queued/entered IO cancellation, real IO and worker failures, retained-slot budget, exact recovery owner | Selected P1 implemented |
| Explicit client close | Runtime close, same-ID acknowledgment recovery | `release_client` / LocalHost shutdown | Immediate client lease cleanup, formal preservation, entered IO finishes; last native owner releases lock | Selected P1 implemented |
| Native blocking/lifetime | Job wait, close, destruction | Local owner + CPU workers | Native waits, idempotent close, Rust completion/last-owner tests | Implemented |
| Import without native | Values, errors, Connected backend | Pure Python | Isolated process without extension | Implemented |
| PreparedImage | `runtime.resources.prepare_image`, `pixels`, `close`/context | Client-owned Runtime image table + SDK preparation | A/B independence; 100 cancelled sources reclaim actual Rust bytes; response-loss recovery; both installed wheels | Implemented; direct bytes or reusable Buffer, aggregate admission |
| Image create/replace/query | `edit.resources.create_image/replace_image`, `session/edit.resources.get`, typed ops | `Runtime::apply_authoring/image_snapshot` → SDK image methods | Fixed pixels, immutable snapshots, mixed mesh/resource batch, Undo/Redo, save/reopen | Selected P1 implemented |
| Formal/candidate Plan | Domain operations, cached report, `session.plan` / `edit.plan` / `edit.apply(plan)` | Source-bound Runtime plans through `apply_authoring` | Exact source, stale/foreign/closed refusal, batch rollback, report after close, same-ID recovery | Selected P1 implemented; P3 expands |
| Binary buffers/large images | `prepare_buffer`, `prepare_image`, Buffer reads | Binary chunks, SHA256, immutable blobs and leases | Every transfer-phase loss, seal peak, owner/hash negatives, installed large grid and images | Implemented R4 |
| Mesh binding/keyforms | `bindings.MeshBindingSpec/Keyform`, `session/edit.bindings`, typed ops | SDK spec/core canonical identities; unified Runtime candidate batch | Numerical interpolation/clamp, identity retention, failures/no-op/redo, installed recovery/save/reopen and real Editor | Selected Runtime R4 slice implemented |
| Selected scene/organization | `scene` typed ops, `session/edit.scene.get/get_many/plan` | SDK `SceneOperation` → existing scene/organization/content/binding intentions → unified Runtime batch | Local/Connected scene/group/Glue/Offscreen recipe, plans, save/reopen; installed wheels and Editor Part/group projections | P3 started; selected slice implemented |
| Mesh BlendShape/bound topology | `scene.CreateBlendTable/CreateMeshBlend/SetMeshBlendVertices/InsertMeshVertex/ReplaceTriangles`, `ReparentLocal` | Existing SDK/Core planning, interpolation, remapping and limits | Independent ordinary+Blend sweep `.5,6.5,12.5`, vertex interpolation, formal/candidate invariance, invalid topology; installed recipe | Selected P3/R4 slice implemented |
| SDK object references/label addresses | `session.reference`, `resolve/scene_capabilities/label`, `LabelAddress.plan`, explicit close | SDK incarnation and context/field epochs; bounded Runtime tables | Move, field change, delete/Undo, stale rejection, ownership/budgets/100 release rounds and lost responses | Implemented R4; all SDK selector domains |
| Full Modeling/collections/conversions | Generated `modeling.ModelingCommand`, typed records/intents/capabilities | Exhaustive SDK dispatch and compiled input/output shapes | Same-backend numerical recipes, all address domains, SDK domain regression | Implemented R4 |
| Project formats and resource IO | Project/Sessions Jobs; PNG Jobs and ImageLease | SDK import/open/generation/export and bounded decoders | Installed PSD/Model3/MOC3/Cubism and all sidecar formats; disk overwrite and memory leases | Implemented R4 |
| Modeling CPU sampling | `session/edit.modeling.sample`, immutable `sampling` values | Same typed Runtime sampling entry and frozen SDK evaluation | Mesh binding/keyform numeric recipe, source/limits/recovery and real Editor human Preview/history isolation | Implemented R4; frozen sync/async CPU batches |
| Animation/Preview | Typed query and independent sources | Shared SDK/Runtime | Numerical and isolation recipes required | Runtime R5 / Python P5 pending |
| Observer/inspection | Device-backed observer, offline workflow | Rust observation + Python artifact organization | Conditional GPU and offline evidence required | Planned P5 |
| Packaged acceptance | Both distributions and installed desktop app locations | Same host ownership/protocol | Pure/native wheels and repository-built desktop tested; paired source archives and rebuilt wheels checked separately; app/platform matrix pending | P6 partial; Runtime R3 discovery implemented above |

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

Current R4 implementation and acceptance: [complete record](../../docs/plans/sdk-runtime/python2-r4-complete.md).
Python [P3A-0](../../docs/plans/python-sdk-rewrite/p3a-0.md) has passed its freeze gate.
[P3A-1](../../docs/plans/python-sdk-rewrite/p3a-1.md) implements complete-call recovery,
cached state and JobTimeout with source/type/installed evidence. P3A-1 is accepted;
the user waived this round's final performance noise blocker. Raw failed measurements
and the original P0 policy remain unchanged.
P3A-3–6 and the overall API/agent contract are accepted for the implemented scope;
evidence and the user's removal of the model/service identity gate are recorded in
[P3A-6](../../docs/plans/python-sdk-rewrite/p3a-6.md).

[P3A-2](../../docs/plans/python-sdk-rewrite/p3a-2.md) is accepted. Existing reads
use root `Snapshot[T]`/`Batch[T]` with pure payloads and exact empty-batch sources.
Domain operations, the common plan/apply path, generated parameter/binding/sampling/
event/reference DTOs and root `PlanCost`/`PlanReport` are implemented.
Collection pagination uses `Page[Summary]`/`PageCursor` and has focused P3A-6
evidence for more than 256 objects, exact source, bounded scans and stale cursors.


P3A-3/4/5 was committed as `8500d9fe`; **P3A acceptance is complete**. It adds generated uniform
facades/backend registration and unique transport hooks, exact-source metadata
pages and selected reads, Runtime capabilities/limits, Session close/detach and
alias invalidation, bounded owned-value output, private codecs/factories, and the
offline catalog/reference/guide/recipes. Agent fixture inputs, installed packet
preparation and bounded recording are present. Local/Connected external controls,
pre-close history/content and independent file verification are connected. The
fixed 42-run agent functional gate passed at 38/42 and at least 2/3 per environment;
a supplemental review corrects three grader false negatives to 41/42 without
rerunning tasks or replacing the original records. Token usage is report-only:
16,093,048 for the fixed matrix. The user confirmed the configured model/effort
as `gpt-6.1-sol/high` and removed the model/service identity gate. Optional metadata
is report-only. Prior behavior/type/distribution/
Editor and P0 evidence is reused within its original scope.

The 2026-10-26 [P3A closeout](../../docs/plans/python-sdk-rewrite/p3a-6.md#2026-10-26-p3a-收尾复核)
synchronizes the current `zipzi` / `zipzi-native` `0.1.0a1` distribution metadata,
generated source fingerprint and usage documentation. The catalog version is
derived from the client manifest. The earlier real probe supplied no model/service
revision; that historical observation remains recorded, and no longer blocks
acceptance. Task and matrix verdicts use the functional thresholds and hard stops.

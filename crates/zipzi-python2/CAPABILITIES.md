# Capability scope

This list is a delivery checklist for a new package. It does not generate API
signatures or define another wire schema. Domain behavior is checked against
fixed geometry/history and the independent Rust SDK tests.

[Python P7 closeout](../../docs/archive/plans/python-sdk-rewrite/p7.md) reconciles the
complete current scope for macOS arm64 / CPython 3.14, including the shared
contract, generated/manual responsibilities, measured costs, agent evidence and
final distributions. Slice labels below retain the stage in which each route
was introduced; later aggregate acceptance does not expand historical results.

| Capability | Public entry | Shared typed Rust route | Evidence | State |
| --- | --- | --- | --- | --- |
| Mesh generation | Same public values/ops and query methods | Selected Rust declarations + compiled query registration | Deterministic generation/check-only/protection, real Rust negative compilation, unchanged behavior/static tests | P2 mesh slice implemented |
| Resource/Plan generation | Image values/usage, typed resource ops and rename intents, Plan report/cost, resource conveniences | Actual Rust declarations and compiled image/pixel/Plan registrations | Finite bool/u64/usize/bytes/string-map shapes, type propagation, direct native conversion and independent recipes | P2 selected slice implemented |
| Local ownership | `local()` / Runtime context | `host::LocalHost` | `tests/test_api.py::test_shared_recipe`; Runtime host tests | Implemented |
| Host discovery/start | `connect()`, `connect(start=True, host=...)`, `runtime.connection` | Same endpoint ownership/inherited fd and strict protocol/build handshake | Installed subprocess cold-start/slow/failure/security cases; current production Editor cold start/reopen | Delivered for the supported macOS app locations; normal installed-app discovery verified in R7 |
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
| Binary buffers/large images | `prepare_buffer`, `prepare_image`, Buffer reads | Binary chunks, SHA256, immutable blobs and leases | Transfer-phase loss, seal peak, owner/hash negatives; installed large grid and 2,101,248-byte RGBA project/package workflow | R4 implemented; P4 scope accepted |
| Mesh binding/keyforms | `bindings.MeshBindingSpec/Keyform`, `session/edit.bindings`, typed ops | SDK spec/core canonical identities; unified Runtime candidate batch | Numerical interpolation/clamp, identity retention, failures/no-op/redo, installed recovery/save/reopen and real Editor | Selected Runtime R4 slice implemented |
| Scene/organization | `scene` typed ops, `modeling.ModelingCommandScene`, scene reads and plans | SDK scene/organization/content/binding intentions → unified Runtime batch | Scene/group/Glue/Offscreen, all scene binding tracks, organization/deformation/drawing collections; source/installed recipes and existing Editor projection | R4 implemented; P3B scope accepted |
| Mesh BlendShape/bound topology | `scene.CreateBlendTable/CreateMeshBlend/SetMeshBlendVertices/InsertMeshVertex/ReplaceTriangles`, `ReparentLocal` | Existing SDK/Core planning, interpolation, remapping and limits | Independent ordinary+Blend sweep `.5,6.5,12.5`, vertex interpolation, formal/candidate invariance, invalid topology; installed recipe | Selected P3/R4 slice implemented |
| SDK object references/label addresses | `session.reference`, `resolve/scene_capabilities/label`, `LabelAddress.plan`, explicit close | SDK incarnation and context/field epochs; bounded Runtime tables | Move, field change, delete/Undo, stale rejection, ownership/budgets/100 release rounds and lost responses | Implemented R4; all SDK selector domains |
| Full Modeling/collections/conversions | Generated `modeling.ModelingCommand`, typed records/intents/capabilities | Exhaustive SDK dispatch and compiled input/output shapes | Affine reparent, grid/range/bake conversion and scene binding numerical recipes; independent collections, all address domains and nested Undo identity expiry | R4 implemented; P3B source/types/installed scope accepted |
| Attachments | `modeling.ModelingCommandAttachment`, selected canonical records | SDK attachment operations; shared binary authoring/records and project IO | 1,600,102-byte UserData persistence and package roundtrip; empty/unavailable distinction, invalid-path rollback, unreferenced export refusal | R4 implemented; P4 accepted; independent convenience collection optional |
| Project formats and resource IO | Project/Sessions Jobs; PNG Jobs and ImageLease | SDK import/open/generation/export and bounded decoders | PSD/Model3/MOC3/Cubism and all sidecar formats; Save As/history paths, disk overwrite/captured leases, generation expiry and installed package roundtrip | R4 implemented; P4 scope accepted |
| Modeling CPU sampling | `session/edit.modeling.sample`, immutable `sampling` values | Same typed Runtime sampling entry and frozen SDK evaluation | Mesh binding/keyform numeric recipe, source/limits/recovery and real Editor human Preview/history isolation | Implemented R4; frozen sync/async CPU batches |
| Animation authoring | `modeling.ModelingCommandMotion/Expression/Physics/Pose`, selected records, intents, addresses and capabilities | Existing SDK planners and core edits through the same Runtime batch; compiled finite types and binary codecs | Four-domain Local/Connected recipe, sidecar contents/save/reopen, SDK address Undo regression, large binary records and complete-call recovery | R5/P5 first slice implemented; see [record](../../docs/archive/plans/sdk-runtime/python2-r5-authoring.md) |
| CPU animation Preview | `session/edit.preview`, `animation` controls, sample/seek/advance Jobs | Independent SDK MotionPreview per client-owned frozen source; bounded shared worker route | Numeric Motion/Expression/geometry, Pose, candidate sequences, two clients/views, cancellation, 100 release rounds, binary/recovery and Local no-JSON | R5/P5 CPU slice implemented; [record](../../docs/archive/plans/sdk-runtime/python2-r5-preview.md) |
| CPU spatial queries | `session/edit.modeling.spatial`, Preview `queries`, typed `spatial` values | Shared SDK bounds/hits on frozen explicit poses or exact animated state; borrowed source object tables | Numeric bounds, stable vertex IDs/barycentric/UV, hidden filtering, Part ancestry, ordering/limits, cancelled candidates, worker cancellation and binary recovery | R5/P5 CPU slice implemented; [record](../../docs/archive/plans/sdk-runtime/python2-r5-spatial.md) |
| Editor animation Preview | Independent human view playback and Timeline projection | Existing Editor/SDK worker and Metal chain | Human/agent clock/Physics isolation, external Motion refresh, old frame refusal and two viewports | Integration/UI isolation and native/window Release comparisons passed; [final record](../../docs/archive/plans/sdk-runtime/python2-r7-closeout.md) |
| GPU observation / inspection export | `modeling.observe`, Preview `view`, `observation` values and `inspection.save` | Shared SDK Observer, one resolved texture set per batch, Rust RGBA/PNG; Python local file organization | Actual Metal pixels, formal/candidate texture replacements, animated source, binary/recovery and offline PNG/JSON export | R5/P5 slice implemented; [record](../../docs/archive/plans/sdk-runtime/python2-r5-observation.md); portable scene rerender remains separate |
| Direct Editor handoff | Attach resident Session; Edit batch `commit().wait()`; next Edit reads human refinement | Existing shared formal document/history and native event refresh; installed `zipzi.docs.editor_handoff` example | Mixed mesh/texture/Motion batch, native property/Timeline inputs, fresh-client continuation, atomic Undo, no-op/redo and image cleanup | Simplified R6 delivered; [scope/evidence](../../docs/archive/plans/sdk-runtime/python2-r6-handoff.md); Proposal/Review deferred |
| Packaged acceptance | Both distributions and installed desktop app locations | Same host ownership/protocol | Paired normal sdists/wheels and GPU recipes, signed macOS app/DMG, installed-app discovery/cold start/reopen/save; current macOS arm64 / CPython 3.14 integrated acceptance passed | P6 current platform scope and Runtime R7 complete; [current results and limits](../../docs/archive/plans/sdk-runtime/python2-r7-closeout.md) |

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

Current R4 implementation and acceptance: [complete record](../../docs/archive/plans/sdk-runtime/python2-r4-complete.md).
R4 predates the Python P3A/P3B split. Its 2026-10-26 closeout passed current
Local/Connected full recipes, paired source/wheel rebuilds and native Editor/Metal
acceptance. Box layout and ordinary mesh batch costs were rechecked in Release;
historical P1 query and binary-image comparison limits are recorded separately.
Existing modeling/resource/IO routes are not new P3B/P4 implementation work.
The [P3B record](../../docs/archive/plans/python-sdk-rewrite/p3b.md) closes the original
modeling scope using these routes and additional public conversion/scene/identity
scenarios. Six focused tests run both backends; strict types and the normal-wheel
Local/Connected installed recipes pass. Product code and public signatures are
unchanged; R4 performance and Editor evidence retain their original scope.
The [P4 record](../../docs/archive/plans/python-sdk-rewrite/p4.md) closes the resource/project
scope with nine focused regressions, types and isolated Local/Connected installed
recipes. It records the SDK's original-path dependency for old disk Undo resources
after Save As; explicit image captures retain their own content. No product code
or public return types changed, and optional attachment conveniences are separate.
Python [P3A-0](../../docs/archive/plans/python-sdk-rewrite/p3a-0.md) has passed its freeze gate.
[P3A-1](../../docs/archive/plans/python-sdk-rewrite/p3a-1.md) implements complete-call recovery,
cached state and JobTimeout with source/type/installed evidence. P3A-1 is accepted;
the user waived this round's final performance noise blocker. Raw failed measurements
and the original P0 policy remain unchanged.
P3A-3–6 and the overall API/agent contract are accepted for the implemented scope;
evidence and the user's removal of the model/service identity gate are recorded in
[P3A-6](../../docs/archive/plans/python-sdk-rewrite/p3a-6.md).

[P3A-2](../../docs/archive/plans/python-sdk-rewrite/p3a-2.md) is accepted. Existing reads
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

The 2026-10-26 [P3A closeout](../../docs/archive/plans/python-sdk-rewrite/p3a-6.md#2026-10-26-p3a-收尾复核)
synchronizes the current `zipzi` / `zipzi-native` `0.1.0a1` distribution metadata,
generated source fingerprint and usage documentation. The catalog version is
derived from the client manifest. The earlier real probe supplied no model/service
revision; that historical observation remains recorded, and no longer blocks
acceptance. Task and matrix verdicts use the functional thresholds and hard stops.

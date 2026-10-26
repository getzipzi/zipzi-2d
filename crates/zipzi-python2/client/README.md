# zipzi

Pure Python authoring client. Import as `zipzi`. The optional
`zipzi-native` distribution supplies `zipzi_native._native` for
`zipzi.local()`; connected clients can import and run without it.

Recovery resumes one complete public call with its original request identity:
creation returns an ID and `edit.commit()` returns a `Job`, including after a
lost response. `zipzi.recover_call(bound_sdk_method, ...)` preserves the method's
static type, invokes it once and recovers once. Otherwise retain the owning
`OutcomeUnknown` and call its `recover()`; there is no `Edit.recover` method.
`edit.state` and `edit.receipt` are cached observations. Context exit requires
confirmed commit or explicit cancellation and raises a caught unknown result
again. `job.wait(timeout=None)` waits until terminal; a finite timeout raises
`JobTimeout` while the accepted Job remains usable. Close an unresolved Runtime
only after recovery or with explicit `abandon_pending=True`.

The native/host pairing is `zipzi-python2-14` / `zipzi-runtime-p3a-345-1`; rebuild
adapters and hosts together after a contract change.

See the [package source](https://github.com/getzipzi/zipzi-2d/tree/main/crates/zipzi-python2)
for the currently implemented scope and validation commands.

`zipzi.connect()` discovers a resident desktop host. Use `start=True` with an
absolute `host=Path(...)` or `ZIPZI_EDITOR_HOST` to launch it under the same
endpoint ownership rule. `runtime.sessions.list()` and `sessions.attach(info.id)`
bind existing unsaved sessions; `session.show_window()` displays one in the
desktop host. Typed parameter create/patch/batch reads, mesh batch reads and
independent cursor subscriptions are implemented. Mesh binding creation/keyforms,
canonical binding queries and explicit formal/candidate CPU pose batches are
available through `bindings` and `modeling.sample`. Sampling retains its exact
source and leaves human Preview/history unchanged. Canonical Modeling commands,
source-bound references/addresses, binary Buffers, cancellable reads and project
import/export routes are also implemented. Runtime R4 validation and the later
platform/application distribution acceptance are tracked separately.

Read results use `zipzi.Snapshot[T]` (`source`, `value`) and `zipzi.Batch[T]`
(`source`, `items`). Domain payloads have no Source field. Use
`session.meshes.get(id).value.name` and `session.meshes.get_many(ids).items`.
Recovery preserves the same container and captured source.

Construct operations in `meshes`, `parameters`, `bindings`, `resources`, `scene`
or `modeling`; `ops` contains only static unions. Use `ParameterPatch` for field
values and `parameters.Patch` for the operation. `session.plan(operation)` and
`edit.plan(operation)` use the same registered operations as direct apply. Apply
Plan values with `edit.apply(plan)`; mixed batches allow one Plan in the first slot.


The installed package includes `zipzi/docs/guide.md`, `reference.md` and
`recipes.py`. `zipzi.describe("zipzi")` is an offline entry point with no native
extension or host requirement. Collection paging, Session detach/close and the
new offline catalog have deterministic and installed P3A-6 evidence. The fixed
agent functional gate passed. P3A is accepted after the user removed the
model/service identity gate; `gpt-6.1-sol/high` is recorded as the configured
model/effort. The P0 stability waiver applies only to its recorded 2026-10-26 round.
Animation authoring, independent
Preview and GPU observer/inspection remain future Runtime capabilities.

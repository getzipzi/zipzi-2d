# zipzi-python2

Pure Python authoring client. Import as `zipzi`. The optional
`zipzi-python2-native` distribution supplies `zipzi_native._native` for
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

The native/host pairing is `zipzi-python2-11` / `zipzi-runtime-p3a-1`; rebuild
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
source and leaves human Preview/history unchanged. Full R4 Modeling,
handles/addresses, larger cancellable workloads and resource/IO coverage remain in progress.

# zipzi-runtime

Host for `zipzi-sdk` authoring sessions, shared history and CPU/GPU jobs. Runtime
owns clients, edits, jobs, project path reservations and event streams. It has no
GPUI or PyO3 dependency. The default build is CPU-only; `gpu-observation` enables
explicit GPU rendering through `zipzi-observe`.

The Editor and Python Local and Connected clients use the same Runtime. Editor
windows attach views to resident sessions, so closing a tab can detach the view
without discarding the document or history.

## Typed access

```rust
use zipzi_core::{Canvas, Vec2};
use zipzi_runtime::Runtime;

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let mut runtime = Runtime::default();
let client = runtime.register_client()?;
let session = runtime.create_session(
    client, "70000000-0000-4000-8000-000000000001",
    Canvas::new(100., 100., Vec2::default(), 1.),
)?;
let subscription = runtime.subscribe(session)?;
let edit = runtime.begin_edit(client, session, "Resize", Some(subscription.info.version))?;
runtime.apply_edit(client, edit, |candidate| {
    candidate.replace_model_setup(Canvas::new(200., 100., Vec2::default(), 1.))
})?;
runtime.commit_edit(client, edit)?;
assert_eq!(runtime.session(session)?.history_lengths(), (1, 0));
assert_eq!(runtime.events_since(subscription.cursor)?.len(), 1);
# Ok(())
# }
```

Formal sessions are read-only through Runtime. Edits operate on isolated
candidates and commit through SDK history. An SDK error aborts the candidate;
busy or admission errors leave it available. Snapshots capture a version and
share retained image content. Capture diagnostics before consuming an edit.

## Jobs and lifetime

`LocalHost` runs a Runtime thread and worker pool for in-process clients. Desktop
hosts dispatch `Work` to an executor and deliver each `Completion` to Runtime.
Accepted work must return a completion even if its view has closed. Cancellation
is advisory once work has started. Sessions return to Ready after a job; a
worker panic faults the session and requires it to be closed and reopened.

Open and Save As reserve normalized project paths within the Runtime. Detaching
a client or view does not close its session. Closing checks other clients,
active work and unsaved content. Completed job results occupy bounded slots
until released.

## Events and limits

Subscriptions capture state and a cursor together. Session and registry events
have separate cursors; a gap or invalid cursor requires resynchronization.
`RuntimeLimits` bounds clients, sessions, candidates, jobs, events and retained
resources. Image, blob and document accounting is conservative and does not
measure process RSS or GPU memory.

The default aggregate accounting limit is 8 GiB and the image limit is 4 GiB
on 64-bit hosts. Cubism import can reserve up to 2 GiB for parsing, subject to
available shared budget; limits are admission ceilings, not preallocated memory.

## Local protocol

The `protocol` and Unix `local` modules provide authenticated connections,
request deduplication, recovery, reconnect grace and endpoint ownership. The
Editor assembles the listener on its application thread. The protocol uses
bounded JSON control frames and binary blobs. `zipzi-runtime-v1` identifies the
wire **major**, independently of package versions, source hashes and additive API
changes. A disconnected client can resume within its grace period; expiry
releases its edits but retains formal sessions and entered jobs.

Compatibility rules for Connected clients and Editor hosts:

- Keep the wire major for new operations, optional services and compatible
  response extensions. Advertise optional services through capabilities; missing
  or unknown capability names do not reject the connection. Unsupported calls
  return `UNSUPPORTED_OPERATION` while existing operations remain usable.
- Ignore unknown handshake and request-envelope metadata. Operation arguments
  stay strict so unsupported mutation options cannot be silently ignored. New
  request options need host capability checks or a separate operation; older
  callers must still work with their existing arguments and defaults.
- Preserve existing operation semantics, required fields and binary layouts.
  Introduce a separate operation/capability for incompatible payloads, or raise
  the wire major if existing calls must change. Binary layout signatures remain
  checked on the affected calls.
- SDK build identities are diagnostic by default. An explicit Python
  `sdk_build=...` pins that exact build. Runtime identity, resume secrets, ownership
  and request deduplication are always enforced.

The earlier feature-specific `rN` protocols require a coordinated upgrade to
this baseline. The in-process `zipzi-native` adapter still pairs with its Python
package and is independent of this Connected compatibility policy.

## Modeling and observation

Typed modeling, animation, resources and project operations use the same
candidate and job boundaries. Plans, selected records, CPU samples and previews
retain their formal or candidate source. Prepared images and image leases keep
content alive until released. Import and replacement publish only validated
results; project reports retain losses and publication warnings.

GPU observation is optional. CPU-only hosts reject image requests with
`GPU_UNAVAILABLE`, while CPU sampling and previews remain available. Read jobs
cooperate with cancellation and scheduling so publication can make progress.

## Tests

From the repository root:

```sh
./tools/test-sdk.sh runtime
cargo test --locked -p zipzi-runtime --test host
cargo test --locked -p zipzi-runtime --test protocol
./tools/test-sdk.sh python2-connected
# Requires a usable GPU:
cargo test --locked -p zipzi-runtime --features gpu-observation --test observation -- --include-ignored
```

See [test coverage](../../tools/README.md#test-coverage) for prerequisites and
broader gates.

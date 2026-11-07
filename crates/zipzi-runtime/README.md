# zipzi-runtime

Single-owner host for the immutable authoring kernel. Sessions and history stay
in Runtime; a Worker receives one Draft, a frozen sourced snapshot, prepared save
input or value-only project load request. Work and Completion have no Session.

## Authoring and dispatch

Register a client and create/insert a session, then `begin_draft` against its exact
Version. Use `batch` for synchronous typed SDK commands or `request_batch` for a
worker. Each successful batch advances DraftSource.sequence once. Drafts are
isolated; one working Draft does not block other Drafts or published reads.
Ownership/busy/resource admission rejection leaves the Draft intact. An accepted
command failure (including ignored command errors or panic) ends the Draft.

`Protocol::execute_response` is the shared Local/Connected response boundary.
Errors for owned Draft operations carry `draft_state` (`ready`, `working`, or
`ended`), captured after execution on the owner thread and cached with the
original response. Clients must not infer admission from an error code alone.
Bounded `Query::Metadata` reads document roots without geometry or image bytes;
all query responses preserve their exact source and content identity.

`publish` or `request_publish` consumes the candidate. Runtime checks the exact
Session, generation, revision, Draft, sequence and job ownership before installing
worker results through SDK publication. Undo/Redo restore existing history nodes
on the owner thread. No-op and failed publication preserve history/redo.

`next_work` supports a manual executor. `dispatch` supports `Executor` and the
persistent `WorkerPool`; rejection returns unentered work for retry. Every
accepted work item must deliver its Completion to `Runtime::complete`, including
after cancellation or client release. Misrouted completions are returned to the
caller so they can reach the correct Runtime. A worker panic ends its task without
requiring a Session recovery copy.

Native hosts can use `take_work(job)` for targeted dispatch and `start_save_as`
to admit and dispatch a synchronous save in one owner call. A full worker pool
leaves queued work intact; immediate save refusal creates no job or save permit.
`adopt_session` installs an initially constructed native session, and
`transfer_session_to` moves an idle, exclusively attached session between owner
hosts without cloning history. Neither operation is a Worker completion API.
`host_state_stamp` provides bounded session/job metadata for view notifications;
candidate bookkeeping does not wake published projections.

`request_read` freezes the requested Published or Draft source. Later edits do not
change its input or relabel its output. This low-level host API accepts trusted
Rust computation callbacks; domain-specific protocol handlers must choose task
bounds before admission. Callbacks cooperate with Cancellation. Read output Vec
capacity must fit its admitted result bound. Actual workspace reservations follow
Work through execution; result leases follow ReadResult clones after job release.
Cancellation alone releases neither entered task slots nor allocations. These
bounds do not estimate document/history size or claim to measure process RSS.

The default shared admission budget is 2 GiB. Native document decoding must fit
alongside resident resources; a saved DeepSeek model alone admits about 1.2 GiB
of JSON workspace. Hosts can set `RuntimeLimits::task_bytes` explicitly for a
smaller or larger workload. Exceeding that limit still rejects the job before
replacing resident content.

`subscribe` captures a snapshot, history state and publication cursor together.
`publications_since` reads bounded receipts without draining other consumers.
Evicted cursors return EVENT_GAP; resubscribe to recover. Clients and sessions,
Drafts, job slots, concurrency, read results and publication retention are bounded.

## Native project work

`request_open` decodes into PreparedProject and replaces a resident Session only
if the original Version still matches. The successful replacement resets history
and advances generation. Failure or stale completion leaves current content intact.

`request_save` / `request_save_as` acquire an exclusive SDK save permit before
dispatch. Published snapshots remain readable. Editing, replacement, Undo/Redo,
other saves and closing are blocked until completion; existing Drafts are retained.
Normalized destinations are reserved against other Session/project jobs. Save
starts only after earlier mutation work completes; read jobs can coexist.

A failed or unentered save releases its permit. Once the file is published, the
host installs the actual ProjectContext and SavedMarker even if cancellation or
client disconnection arrived late. This path has no subsequent model validation or
history admission. Dropping a successful save completion is a host protocol error;
the SDK leaves publication blocked rather than silently forgetting the saved file.

Client release cancels owned outstanding work and ends its Drafts, but does not
close resident Sessions. A later client can attach. Explicit close checks other
attached clients, outstanding work and unsaved content.

## Domain and bulk tasks

`request_samples` runs geometry, spatial queries and detached animation schedules
against one exact Published/Draft source, yielding between poses. Results retain
the original source after later publication. `request_observation` prepares a
bounded texture plan and renders explicit views; output contains length-prefixed
JSON metadata followed by each PNG. GPU availability is a host capability.
`request_export` captures resources and source protection, reserves the output,
and returns actual publication/durability even after late cancellation.

Animation inputs are playback schedules: each motion/expression tuple contains
an ID and an absolute activation time, rather than a blend weight. Playback
activates a motion on its first update at or after that time. Canonical seeks to
at least 1/60 second replay from the first 60 Hz update, so a motion scheduled at
zero has local time `max(0, requested_time - 1/60)` for samples within its first
cycle. The reported `motion_positions.local_time` is the coordinate at which its authored
curve was evaluated. A zero-time sample does not seed later positive replay.
This differs from stateless raw-clip inspection through Rust's
`MotionEditingPreview`, which directly samples the requested clip-local time.

Client-owned blobs provide bounded upload/read chunks. Binary requests use the
same authentication, request IDs, payload digests and recovery cache as control
requests. Cached reads retain just their requested chunk and its allocation
lease. Image replacement consumes sealed RGBA inside one ordinary Draft batch;
source pixels remain valid after the upload is released. Job byte results are
retrieved through `ReadJobBlob`, rather than embedded in control frames.
Job readback handles share the immutable result allocation and its lease; closing
the Job leaves a Blob/escaped reader valid. Recovery still caches only bounded
chunk copies, and the per-client Blob handle limit applies to shared readbacks.

## Focused tests

```sh
./tools/test-sdk.sh runtime
cargo test --locked -p zipzi-runtime --features gpu-observation --lib
cargo test --locked -p zipzi-runtime --test deepseek_roundtrip -- --ignored
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test authoring_kernel
```

The CPU gate covers resident ownership, isolated Drafts, immutable source
identity, cancellation, binary recovery, private transport, native project work
and CPU sampling. The GPU gate requires a usable device. Track the remaining
integration migrations in the [authoring plan](../../docs/archive/plans/authoring-architecture.md).

The optional `deepseek_roundtrip` target requires `models/local/DeepSeek`. It
imports, saves and reopens the full model under default limits while retaining
the original snapshot, comparing geometry, bindings and every image's pixels.

For allocator attribution of fixed-target editing, run the optional probe:

```sh
cargo run --locked --release -p zipzi-runtime --example retention_probe \
  --features bench-instrumentation -- models/local/DeepSeek/大肥鱼.model3.json 1200
```

It reports requested live/peak/cumulative Rust allocations, then closes the
Session and drops Runtime separately. Instrumented CPU time is not a UI/IPC
latency measurement. Cleanup boundaries sweep retired image-budget weak slice
registrations so they do not retain unowned pixel allocations until the next
budget query; escaped snapshots/textures keep their existing ownership.

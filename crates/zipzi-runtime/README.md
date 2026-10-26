# zipzi-runtime

Single-owner host for `zipzi-sdk` authoring sessions and independent CPU/GPU
observation jobs. It owns the sole formal session and history, client/edit/job
tables, project path reservations, bounded event streams and executor handoffs.
There is no GPUI or PyO3 dependency. GPU observation uses `zipzi-sdk-observe` and
initializes a device only on an explicit render worker. SDK does not depend on Runtime.

The desktop Editor uses the same application-owned Runtime. Windows and tabs
attach views. Closing a tab retains resident sessions and history; closing the last
desktop window exits after save confirmation when no SDK client or background job
needs the host. Explicit `--no-window` hosts remain resident. Typed UI writes
and external clients share publication, event and project-path boundaries.
The `protocol` and Unix `local` modules provide the R3 control-message slice,
remote request deduplication, reconnect grace periods and endpoint ownership.
The production listener is assembled by the desktop Editor on its app thread.

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

`session` and `document` borrow the formal instance without copying it and expose
no mutable session. `edit_document` reads previous candidate operations.
`edit_snapshot` captures the candidate with its EditId, base Version and sequence;
formal snapshots include their exact Version and Ready/Busy/Faulted state.
Busy/Faulted reads share the retained frozen snapshot through `Arc`.
Snapshot captures copy document records and share retained memory image content;
they are explicit operations, rather than the default object-query path.

Ordinary SDK failures, including failures caught inside an apply closure, abort
the candidate. A candidate budget overflow also aborts it without changing formal
content/history. Release aborted edits explicitly. A host closure panic poisons
the candidate before propagating. Commit consumes its edit and always calls SDK
`commit_to`; SDK error diagnostics pass through. Capture any desired diagnostic
snapshot before commit. Busy and admission-budget errors leave the edit available.

## Jobs, paths and lifetime

`host::LocalHost` is the in-process host for the independent
[zipzi-python2 SDK](../zipzi-python2/README.md). It explicitly owns
a Runtime thread, bounded call/waiter admission and a fixed CPU worker pool.
Typed `call` requests execute serially; `wait_job` waits on the caller thread.
Worker completions publish without another API call. Close cancels queued work,
consumes entered workers' actual results, then drops all sessions without saving.
Adapters must release the Python GIL while calling/waiting/closing, and must never
invoke user callbacks or wait for this host's jobs inside a `call` closure.
Its thin native adapter enters this host directly; no Python callbacks run here.

Focused foundation tests: `cargo test --locked -p zipzi-runtime --test host`;
the queue admission test is under `--lib host::tests::`.

`request_open`, `request_save`, `request_commit`, `request_undo` and `request_redo`
queue ownership-carrying jobs. The application owns an `Executor`; `dispatch`
passes it `Work`, the worker calls `Work::run_quantum` (or `run_on_thread`), and the application delivers the
returned `Completion` to `Runtime::complete` before dispatching again. Batch
continuations keep their Job Running and produce no result/event until terminal.
`Completion::is_terminal` lets desktop hosts avoid waking views for internal steps.
`Work::run` remains available to explicitly execute a whole batch synchronously.
Accepted work must always deliver
its completion; rejected submissions return the exact unexecuted work. Executors
must not panic during submission. Runtime installs no thread pool or window hook.
The CPU tests include a real threaded executor and filesystem panic injection.

Sessions follow Ready → Busy → Ready. SDK errors return the same SDK instance.
A caught worker unwind discards the state of uncertain consistency, reports a
failed Job and leaves the session Faulted with an inspectable frozen source.
Session metadata retains the fault even after its original client expires.
Faulted sessions reject writes/save and require explicit close before reopening.
Completions are move-only, validated against runtime/session/job/base identity,
and cannot restore a closed or faulted session twice.

Cancelling is advisory until completion confirms the work stopped. Cancellation
before execution returns Cancelled; after SDK execution starts its actual result
wins. The queued/running target reservation remains held until completion.
Open/Save As normalize directory/manifest and symlink aliases using
`zipzi_project::project_identity`. Another session or pending Open/Save As cannot
acquire the target. Save As keeps the old identity until success; failure keeps it.
These rules coordinate callers of this Runtime, not independent processes.

`detach_session` and client expiry do not close formal sessions. Close checks
other attached users, candidates, tasks and unsaved content; Closed is returned
and published in the registry stream before the entry is removed. No windows or
Python clients are required for a completion to publish or for sessions to live.

## Events and limits

Runtime alone drains hosted SDK receipts. `subscribe` atomically captures a
snapshot and cursor. Session and registry cursors have separate order; management
events do not invent content versions. Events carry client/operation identity; receipts retain SDK before/after versions.
A count/byte overflow, oversized event, foreign or future cursor returns
`ResyncRequired`. Re-subscribe to recover the current state. There are no callbacks
under SDK borrows, and UI redraw failures cannot change a successful Job result.

Python metadata subscriptions use `subscribe_info`, which captures SessionInfo
and its cursor atomically without cloning the model. `events_page` admits at most
256 events and a conservative 1 MiB payload estimate before adapter expansion.
The control protocol defaults to pages of 128; cursors advance after delivery.
The original typed `subscribe` retains its full frozen snapshot for Editor
projection callers. Ordinary Rust `events_since` keeps its existing semantics.

The independent Python client also uses typed SDK parameter create/patch intents
in `apply_authoring` and source-bound mesh/parameter batch reads. Whole selections
are admitted before copies. Focused gates are `--test authoring`, `--test protocol`
and SDK `--test integration parameter_editing::`; see the
[R3 evidence](../../docs/plans/sdk-runtime/python2-r3-events.md).

R4 routes full typed modeling commands and SDK plans through `apply_authoring`.
Canonical selected records, geometry, bindings and CPU sample arrays use direct
native conversion or immutable binary payloads. `sample_modeling` and frozen read
Jobs accept at most 64 explicit poses/256 parameters per pose, with geometry-based
aggregate admission before clone/evaluation. Reads preserve exact sources and
cannot modify an Editor Preview or history. One worker remains available for
formal publication. See the [R4 acceptance record](../../docs/plans/sdk-runtime/python2-r4-complete.md).

R1 applies the enabled R0 defaults in `RuntimeLimits`: client/session/edit counts,
candidate estimates, frozen document estimates, aggregate history reservations,
accounted document CPU estimates, job slots/concurrency and event counts/bytes.
In-flight commits retain their candidate charge and reserve publication growth.
Completed job results occupy bounded job slots until `release_job`; expiry drops
an expired client's terminal results and retains ongoing work through completion.
This conservative R1 slot policy is replaced/extended by R3 request-result windows.

Document/metadata accounting is conservative. Unique RGBA/PNG/blob allocations,
preparation and transport leases share the host ceiling. Disk ImageLease captures
pin actual immutable content. The independent Preview and Proposal owners are
implemented in R5/R6 using these same resource roots. Caller-owned typed Python
values are separate from Runtime token counts; RSS is measured independently.

## Tests

From the workspace root:

```sh
./tools/test-sdk.sh runtime
./tools/test-sdk.sh runtime a02_
cargo test --locked -p zipzi-sdk --test integration candidate_snapshot::
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration runtime::
python3 tools/test_tools.py -p test_runtime_dependencies.py
```

See [R1 evidence](../../docs/plans/sdk-runtime/r1-core.md) for the CPU acceptance
mapping and [the implementation plan](../../docs/plans/sdk-runtime-implementation-plan.md)
for the remaining Editor, transport and production gates.

## In-process Editor adapter

`prepare_edit` admits a bounded, non-publishable preflight workspace; `promote_edit`
enforces the writable slot before publication. A view can validate a replacement
before resolving its previous gesture. Prepared work still occupies candidate
counts and bytes. `begin_interaction` prevents another client's publication or
exclusive IO from interrupting a human gesture; Busy preserves its candidate.

`lease_candidate` temporarily moves only an SDK candidate out of the table, so
view helpers can inspect formal state without a Runtime write lock. The host must
call `return_candidate` on success, error and unwind. The candidate remains
charged until returned/released. No mutable formal SDK borrow is exposed.

`request_import` uses the same job boundary as Open/Save. `take_work` is the typed
executor adapter with the same running-job limit as `dispatch`. A host can choose
a decoder stack via `Work::run_on_thread`; spawn failure returns untouched state.
`Completion::returned_session` permits read-only background view preparation.
Apply the completion before any weak-window notification, and keep presentation
failures separate from the formal Job result.

## Local protocol (R3)

`protocol::Protocol` owns authenticated/resumable clients and a bounded request
result window. The host supplies `&mut Runtime` on the app thread; network threads
only frame/decode and exchange messages through a bounded channel. Formal jobs
continue via `Executor` and completion delivery. Terminal Job queries atomically
transfer their results into the deduplicated request window and release Job slots.
Request status queries do not advance the monotonic admission watermark.

The current contract is `zipzi-runtime-r5-observation-1`: big-endian u32 length, UTF-8 JSON control
frames (16 MiB maximum), strict handshake and runtime-scoped IDs. The high length
bit selects binary framing: a bounded JSON header (16 KiB) plus at most 1 MiB raw
payload. Each immutable blob is at most 256 MiB and SHA256 checked before use.
The serial transport admits one outstanding request per connection, at most eight
socket workers and 16 queued transport notifications. Results retain at most
1024 entries/8 MiB of JSON per client, 64 MiB overall and 300 seconds. Binary
backing retention separately caps unique Arc allocations at 256 MiB per client,
512 MiB overall; eviction never makes an old ID executable again. Serialized
outbound responses retain a transfer lease through socket send, including after
request-cache eviction. Admission failure after an executed write produces an
unknown outcome recoverable with the same RequestId.
Disconnected clients get 30 seconds to resume with their secret; expiry releases
ordinary edits while preserving formal sessions and entered jobs.

`local::Endpoint` validates a private user-owned 0700 directory and 0600 lock,
rejects symlinks and overlong profiles/paths, and uses `flock` for owner lifetime.
Python passes the already locked descriptor to cold-started Editor processes;
only an exclusive owner can remove a stale socket. Accepted sockets explicitly
clear inherited nonblocking mode on macOS, and control frames are streamed in
bounded chunks. The CPU example `local_host_fixture` is a subprocess fault-injection
fixture, without Editor window support.

```sh
cargo test --locked -p zipzi-runtime --test protocol
./tools/test-sdk.sh python2-connected
```

See [R3 acceptance evidence](../../docs/plans/sdk-runtime/r3-ipc.md) and the
[implemented connection slice](../../docs/runtime/r3-coverage.json).

## R4 modeling, resources and project IO

`modeling::ModelingCommand` covers all SDK modeling domains, conversions,
dependency-preserving topology, rectangle generation, document/reference/attachment
operations. Generated Python inputs and binary layout signatures come from the
compiled Rust declarations. Read admission counts borrowed fields before copying;
large records are encoded directly from borrowed SDK data. Native output creates
typed Python values directly. Typed binary input validates a nonallocating cost
pass before decoding concrete arrays. Attached upload/decode/SDK failure aborts
the candidate on the server; a pre-attachment refusal leaves it usable.

SDK handles and all field/content/reference/selection/document address domains
retain incarnation and epoch validation, including after moves/delete/Undo.
Plans retain exact formal/candidate sources and immutable reports. Frozen sample
and Plan jobs cooperate with cancellation and do not own the formal session.
PNG preparation is independent of an Edit; only successful completion registers
its PreparedImage. ImageLease pins disk or memory content through replacement.

The project Job route covers new/open/import/replacement/save/diagnostics and
all existing package/Cubism/sidecar exports. PSD and Cubism preparation bound
archive/XML/pixel workspace before decoding. Import and replacement publish only
a fully validated candidate and retain SDK generation semantics. Structured
ProjectReport preserves warnings, diagnostics, codec losses, paths and durability.

`image_memory_usage` separates unique backing allocations, resource entries,
RGBA/PNG/blob/preparing/transfer bytes from metadata estimates. The default
aggregate ceiling is 2 GiB; images/PNG/blobs share 768 MiB; the blob table caps at
512 MiB. Plans have 128 MiB of conservative estimates and 64 slots per client.
Reference/address tables share 1024 slots per client. History, jobs, sources and
leases keep referenced content alive; cancellation, release/client expiry and
history eviction reclaim it after the last root disappears. Binary recovery
retains only the bounded cache described above. RSS/GPU memory are separate.

EditorContextProvider supplies an optional versioned selection/workspace/tool
snapshot. Editor redraw publishes it before taking the Runtime lock; stale or
headless contexts return no snapshot. Native acceptance covers all object kinds,
selection, relations, texture/property projection and a human gesture interleaved
with an external stale candidate. See [R4 coverage](../../docs/runtime/r4-coverage.json)
and [acceptance evidence](../../docs/plans/sdk-runtime/python2-r4-complete.md).

## R5 animation authoring slice

Typed Motion/registration/track/key/event, Expression, Physics and Pose edits,
whole-asset commands, selected canonical records, SDK plans, field addresses and
capability queries use the same Runtime handlers on Local and Connected. The
host advertises `animation_authoring`; this does not advertise independent
Preview or Observer support. Large animation values reuse binary admission and
recovery. Native/IPC pairing is `zipzi-python2-18` / `zipzi-runtime-r5-observation-1`.

The SDK now retains animation field epochs across edits and Undo. Physics change
summaries compare stable row IDs so a sibling reorder preserves field addresses.
The [R5/P5 record](../../docs/plans/sdk-runtime/python2-r5-authoring.md) tracks
source/installed authoring evidence. The [CPU Preview slice](../../docs/plans/sdk-runtime/python2-r5-preview.md)
adds client-owned frozen SDK simulations, exact-source batches and cancellable
seek/advance Jobs. Preview controls never borrow the formal session or its
default preview. One running read worker slot remains reserved for publication.
[CPU spatial queries](../../docs/plans/sdk-runtime/python2-r5-spatial.md) now reuse
SDK geometry on frozen formal/candidate poses and exact Preview states, with
cancellable workers, admission and binary results. [GPU observation](../../docs/plans/sdk-runtime/python2-r5-observation.md)
adds explicit-view image Jobs and actual texture provenance.

Read dispatch rotates between clients and FIFO between each client's jobs and
continuations. Modeling/spatial batches yield after each pose; Preview batches
yield after each target; image batches also yield around shared texture acquisition
and after each rendered frame. A batch keeps one Observer and one texture acquisition
across dispatches, charged to its original Job. Publications take priority, including
on a single-worker host. An entered SDK seek/plan, image decode, texture acquisition
or GPU submission finishes or cooperatively cancels before yielding; this is not a
fixed wall-clock time slice. The desktop animation worker and native Metal display
keep their own state and queue. Focused regressions are `--test scheduling` and
Editor `timeline::runtime_isolation`; [scope and evidence](../../docs/plans/sdk-runtime/python2-r5-scheduling.md).
R5 remains in progress pending integrated native performance/acceptance work.

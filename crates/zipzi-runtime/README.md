# zipzi-runtime

CPU-only, single-owner host for `zipzi-sdk` authoring sessions. It owns the sole
formal session and history, client/edit/job tables, project path reservations,
bounded event streams and executor handoffs. There is no GPUI, PyO3 or GPU dependency. SDK does not depend on Runtime.

The desktop Editor uses the same application-owned Runtime. Windows and tabs
attach views; closing them retains resident sessions and history. Typed UI writes
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

`request_open`, `request_save`, `request_commit`, `request_undo` and `request_redo`
queue ownership-carrying jobs. The application owns an `Executor`; `dispatch`
passes it `Work`, the worker calls `Work::run`, and the application delivers the
returned `Completion` to `Runtime::complete`. Accepted work must always deliver
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

R1 applies the enabled R0 defaults in `RuntimeLimits`: client/session/edit counts,
candidate estimates, frozen document estimates, aggregate history reservations,
accounted document CPU estimates, job slots/concurrency and event counts/bytes.
In-flight commits retain their candidate charge and reserve publication growth.
Completed job results occupy bounded job slots until `release_job`; expiry drops
an expired client's terminal results and retains ongoing work through completion.
This conservative R1 slot policy is replaced/extended by R3 request-result windows.

These are document/metadata estimates, not allocator/RSS measurements. Memory
image payload/PNG-cache accounting and reclamation, disk byte pinning, Plan,
Preview, Proposal, blob and transport queue budgets are later-stage work (R4–R6).
Caller-owned typed snapshots are not Runtime token-table allocations. R1 does not
claim the A17 resource-reclamation or full host-memory acceptance gates.

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

The contract is `zipzi-runtime-r3-2`: big-endian u32 length + UTF-8 JSON, at most
16 MiB per control frame, strict handshake and runtime-scoped IDs. Binary blobs
are unsupported until R4. The serial transport admits one outstanding request per
connection (stricter than the R0 queue maxima), at most eight socket workers and
16 queued transport notifications. Results retain at most 1024 entries/8 MiB per
client, 64 MiB overall and 300 seconds; evicted IDs never execute as new requests.
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
python3 tools/python/test_connected.py
```

See [R3 acceptance evidence](../../docs/plans/sdk-runtime/r3-ipc.md) and the
[implemented connection slice](../../docs/runtime/r3-coverage.json).

# Runtime implementation evidence

This directory records implementation evidence for the paired
[Editor Runtime design](../plans/sdk-runtime-design.md) and
[implementation plan](../plans/sdk-runtime-implementation-plan.md).
R0 records the baseline. R1 now provides the CPU-only
[`zipzi-runtime`](../../crates/zipzi-runtime/README.md) core; see
[R1 implementation and checks](../plans/sdk-runtime/r1-core.md).
R2 migrates Editor ownership and window lifetime; see
[R2 implementation and checks](../plans/sdk-runtime/r2-editor.md). Its interaction
performance gates passed ten alternating process pairs across 145 measurements. R3 adds the local protocol and an independently installable pure Python client;
see [R3 evidence](../plans/sdk-runtime/r3-ipc.md) and the explicit
[implemented connection slice](r3-coverage.json). The source inventory below
remains a classification catalog; `not-connected` there is not a claim that all
entries are unimplemented. R3's coverage manifest records the verified routes.
Python targets CPython 3.14; connection imports work without `_native`.
R4 is in progress: see [implementation/evidence](../plans/sdk-runtime/r4-modeling.md)
and [current verified/pending coverage](r4-coverage.json). The active protocol is
`zipzi-runtime-r4-1`; R3 records retain their historical contract.

## Active Python implementation

The current Python work uses the independent `crates/zipzi-python2` package.
See the [R0–R4 reconnection record](../plans/sdk-runtime/python2-r0-r4.md) and
[new-client coverage](python2-coverage.json). Host discovery, session binding and
the selected real Editor publication recipe have new installed-client evidence.
Typed event subscriptions, parameter creation/patching and mesh/parameter batch
reads now have [new-client R3 evidence](../plans/sdk-runtime/python2-r3-events.md),
including installed response-loss cases, real Editor projection/Undo events and
production killed-client expiry. Full R4 coverage remains pending.
The inventories and older client evidence below retain their historical scope;
they do not establish acceptance for the new package.

## Source inventory

Regenerate and validate from the repository root:

```sh
uv run --locked python tools/runtime/runtime_inventory.py
uv run --locked python tools/runtime/runtime_inventory.py --check
uv run --locked python tools/test_tools.py -p test_runtime_inventory.py
```

[`api-inventory.json`](api-inventory.json) indexes all split SDK implementation
files, the animation implementations re-exported by the SDK, Python facade
methods and records, shared domain intent definitions, native imports and native
stub types. Signatures retain input and result types, with source locations for
review. Public input/result declarations are recorded separately as local value
types or backend references; their Rust definitions are not wire layouts.
Private implementation types such as `Occupied` are excluded. The old
`crates/zipzi-python/python-coverage.json` has 123 entries and its checker reads
only selected source files. Its evidence is retained separately; it cannot
establish complete connected coverage.

Each callable has a required classification:

| Classification | Meaning |
| --- | --- |
| `connected` | Requires a host operation or a backend-neutral public adapter. Not implemented yet. |
| `local-value` | A record, ID, address metadata or Plan report can be represented locally. Applying the associated token still requires host validation. |
| `local-helper` | Pure geometry/value logic or processing an already captured packet/file can stay in the client. Existing imports may still require decoupling from `_native`. |
| `host-only` | Rust filesystem/history configuration and the unique SDK event drain stay inside the host. These are not remote callbacks. |

`facade_route_candidates` are **source matching leads**, not verified bindings.
An empty list means a new public route or an explicit adapter is required; it
never means the capability can be excluded. Every connection implementation
must replace `implementation: not-connected` with recipe-backed evidence. Neither
this catalog nor the legacy binding count is an acceptance pass for R3–R7.

The source indexer is intentionally independent of importing/building PyO3. It
handles split impls, lifetimes, generic methods, comments and raw string braces;
it is not a Rust compiler. New macros generating public APIs or changes to
re-exports require review of discovery as well as regeneration. Trait-derived
value operations (Clone, Debug, equality, serialization) are local value behavior,
not remote operations. The native stub inventory records the current PyO3 type
boundary even for types not exported by `zipzi.__all__`.

## Contracts for indexed APIs

The per-method catalog selects these contracts; current signatures describe the
baseline, while `source_ref`/lifecycle/scheduling describe the **target adapter**.
There is no existing Runtime SourceRef API.

| Family | Sources and result | History / lifetime / bulk behavior | Acceptance |
| --- | --- | --- | --- |
| Session create/open/list/save/replace | Fixed SessionId; current complete SDK Version; SaveReceipt/ImportReceipt retain structured failures | Registry owns path reservations before IO; replace advances generation; save retains history and saved baseline; list/bind unpersisted documents | A02, A08, A13 |
| Query, collection, property, identity | Formal source; candidate reads require the same Edit and sequence; typed values/arrays copied on demand | No full document mirror in Python; ObjectHandle incarnation and address epochs remain SDK checks; batch IDs/fields in one request | A04, A05, A15 |
| Edit / domain intents | Candidate and base Version, typed shared intents/results | Ordinary errors poison the whole Edit; recoverable gesture methods keep their explicit contract; commit uses SDK validation, once; error/no-op/cancel preserve redo | A03–A05 |
| Plan and address | Source plus base Version; affected IDs, cost, losses, report | Client/session-owned token, no revision-only source substitution; explicit release; bulk intents remain whole SDK operations | A05, A15, A17 |
| Geometry and binding | Mesh/scene/blend values, topology maps, shared grid and Blend intents | Same transaction composes writes; conversions keep SDK cost and validation; local pure grid/alpha helpers do not publish | A04, A05, A15 |
| Resources and IO | Host paths or explicit client blobs; prepared immutable content | Decode/encode/hash/import/export on workers; final publication uses original SDK API; retain exact memory/old resource content for candidate/history/snapshots | A02, A08, A14, A17 |
| CPU evaluate and diagnostics | Formal or candidate snapshot, explicit parameter maps; geometry and source metadata | Stateless batched sampling in R4; no playback clock, Preview mutation or history; GPU not required | A14, A15 |
| Motion/Expression/Physics/Pose authoring | The indexed shared domain intents, ID-based tracks/keys/entries, structured errors | Candidate composition and SDK history; no use of private Python `Command` as protocol | A04, A05, A11 |
| Preview and observation | Frozen source plus explicit parameters, schedules and time; geometry/images/reports | Client-owned independent Preview; bounded seek/cache, cancellation, release; CPU authoring works without observe/GPU | A11, A14, A17 |
| Events | Atomically captured initial version/projection plus cursor | Runtime is sole SDK drain; bounded event retention, management events separate from content; explicit resync | A03, A10 |

Rust input/result signatures and the full shared intent definitions are indexed,
including the nested binding/grid/blend operations. The eventual wire format must
use explicit DTOs and blobs rather than Rust layouts or private PyO3 objects.

## Python backend and transaction audit

`__init__.py` directly imports `ObjectHandle`, `SdkFailure`, capabilities and
`NativeSpatialSnapshot`. `_session.py` imports NativeSession, Plan, Address,
prepared-image, cancellation and load-token classes. `_animation.py` wraps native
Preview instances; `_observe.py`, `_inspection.py` and `_spatial.py` depend on
native observation/spatial objects. Even `geometry.py` currently calls `_native`.
The generated native-import and 31-type inventories are the exact source list.

R3 must separate immutable records/errors and backend references, not just add a
socket to NativeSession. Native-only Plan/address/load/control values must become
backend references; metadata/report fields remain values. Do not instantiate
`Arc<Mutex<AuthoringSession>>` in a connected session. Session `set_preview_*`
conveniences must address that client's default Preview in R5, and event polling
must become a cursor subscription rather than consuming Editor's events.

Current `_edit.py` behavior and required R3 changes:

| Path | Current behavior | Required connected behavior |
| --- | --- | --- |
| `_call` and explicit import error handlers | Catch any BaseException and call native abort | Poison only confirmed ordinary domain failure. A confirmed pre-execution Busy leaves the candidate usable. Unknown transport outcome keeps RequestId and blocks further writes pending recovery. |
| `commit` | Unconditionally sets `_closed` in finally | Close only on a confirmed terminal result; an unknown result is recoverable by original RequestId. |
| `__exit__` on exception | Cancel immediately; a cleanup error can mask the original | Respect pending outcome, do not infer rollback from timeout, preserve the original exception if cleanup fails. |
| NativeEdit lifecycle | Native error/abort clears workspace and releases active edit; commit consumes workspace | Retain NativeBackend semantics; introduce explicit connected state handling rather than wrapping every exception in abort. |
| Script failure | Earlier successful commits remain | Same behavior connected; no implicit whole-script rollback. |

## Memory ownership gaps for R4

The following records the pre-R4 baseline. The current preparation API is
`AuthoringSession::prepare_rgba_image`, which returns an owned
`PreparedImageContent` lease without inserting a registry entry. Candidates take
ownership through `create_prepared_image` / `replace_prepared_image`; successful
publication registers the content. R4 now collects unreferenced memory images
while preserving formal/history roots and outstanding leases. The historical
`prepare_rgba_asset` path described below has been removed.

`AuthoringSession::prepare_rgba_asset` enters project `memory_assets` before any
candidate publication. Entries hold `Arc<MemoryAsset>` with decoded RGBA and a
lazy `OnceLock` PNG. `replace_prepared_image_content` registers prepared content
after successful SDK publication via `retain_prepared_image`. Resource lookup
uses the descriptor's source key, not candidate lifetime.

Dropping a candidate, cancelling a load token or evicting a history entry does
not prune that map. Clear paths currently belong to project reset/open/import
and project destruction. Content-hash deduplication does not bound repeated
different images. `estimated_content_bytes` and SDK history estimates account
for document/descriptor/checkpoint data, not the resource table's pixel/PNG
ownership. Do not use either as the total Runtime live byte count.

`read_snapshot` clones document data and retains prepared memory images by Arc;
history/default Preview are not cloned. Disk resources still depend on source
paths: R4 must pin actual immutable content for an observation batch, including
candidate images, old texture versions and Undo resources. A document snapshot
alone is not proof that external files cannot change under it.

Required SDK/project primitives before R4 memory acceptance:

1. Count live unique decoded bytes, encoded bytes and preparation buffers,
   including entries not referenced by the current document.
2. Track roots from formal content, Undo/Redo, candidates, snapshots, previews,
   jobs and later Proposals. Prune entries only after the final root is released.
3. Reserve image and encode/decode budgets before allocation/publication;
   cancellation/error must release temporary registrations without freeing roots
   still in use. Budget rejection preserves existing state.
4. Test at least 100 cycles with distinct image content, cancelled candidates,
   history eviction, retained snapshots and final release. Assert backing entry
   counts/bytes separately from allocator RSS.

## Reproducible CPU fixture and measurements

```sh
uv run --locked python tools/runtime/runtime_baseline.py target/runtime-r0-BASELINE
# Optional local licensed asset: source files are hashed and never modified.
uv run --locked python tools/runtime/runtime_baseline.py target/runtime-r0-MAO \
  --model /absolute/path/to/mao_pro.model3.json
```

The probe builds Release, runs five independent processes per fixture, retains
raw samples and `/usr/bin/time -l` RSS on macOS, validates saved/reopened meshes,
motions and texture bytes, and checks that captures/cancelled candidates/planning
do not publish. Four warmups precede 32 operation samples; first save is one
sample per process and warm save has four warmups/eight samples. Allocation
counts use the existing SDK counting allocator; they include Rust heap requests,
not GPU memory, stack bytes, shared ownership multiplicity or allocator overhead.
Negative net retained deltas are reported as zero, not evidence of zero work.

The license-free fixture contains 128 12×12 mesh grids (21,632 vertices), an
explicitly sampled parameter binding, a one-second parameter motion and a
1024×1024 in-memory RGBA asset. IDs/pixels are deterministic and the driven
geometry change is asserted. Its source is
`crates/zipzi-sdk/examples/support/runtime_fixture.rs`; later Runtime tests can
reuse it without loading licensed models. The optional model uses its original
SDK import API and is saved only into the fresh output directory.

The runner rejects Debug, missing/duplicate samples, absent successful end
validation, compiler activity at process boundaries, source drift and model
drift. Boundary process checks cannot exclude every possible background load;
retain the hardware and run ranges. CPU timing does not establish UI latency.
Results and the exact paired revisions belong to the Editor R0 delivery record.

For native control measurements independent of an imported model's parent
invertibility, generate a separate small fixture:

```sh
cargo run --locked --release -p zipzi-sdk --example runtime_native_fixture -- /absolute/fresh/output
```

It contains two visible 12×12 meshes, a faint framing quad, an asymmetric
in-memory texture, a root 2×2 Warp and an independent unit-scale root Rotation.
The framing keeps outer Warp anchors inside the benchmark inset and fits the
editor's 540-model-unit Rotation axis. Creation uses SDK edits;
structure, evaluated model positions and save/reopen frame equality are asserted.
Pass the emitted manifest to the Editor's existing deformation window benchmark.
This fixture is separate from `runtime_baseline` and does not change that probe's
measurements. An imported-model benchmark failure must still be recorded; a
successful synthetic run does not prove that imported parent deformation works.

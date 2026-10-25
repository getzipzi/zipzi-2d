# zipzi-python2

Independent typed Python SDK built on the shared Rust Runtime. The Python import
is `zipzi`. All public values, errors and lifecycle wrappers live in this package;
native calls use `LocalHost`, while explicit IPC connections use the same Runtime
handlers. The native bridge owns input arrays before releasing the GIL and has
no JSON transport conversion or direct SDK session owner.

## Layout and builds

| Path | Responsibility |
| --- | --- |
| `client/python/zipzi/` | Pure Python client, values, operation objects and backends |
| `client/pyproject.toml` | Pure `zipzi-python2` distribution; no native dependency |
| `src/lib.rs` | Thin PyO3 adapter; Cargo package `zipzi-python2` |
| `native/zipzi_native/` | Native import namespace |
| `pyproject.toml` | Optional `zipzi-python2-native` distribution |
| `tests/` | Shared Local/Connected recipe and positive/negative type examples |
| `tools/test.py`, `tools/baseline.py` | Source validation and P0 measurements |
| `tools/connected.py` | Pure wheel installation, host startup/session acceptance and Editor's isolated interpreter |
| `tools/generate.py`, `examples/authoring_schema.rs` | Compiled Rust contract and the first mesh generator |
| `client/python/zipzi/_generated/`, `src/generated/` | Checked-in mechanical mesh outputs |
| [CAPABILITIES.md](CAPABILITIES.md) | Explicit scope and delivery evidence |

From the repository root, using the workspace's CPython 3.14 tools:

```sh
./tools/test-sdk.sh python2
./tools/test-sdk.sh python2 PythonSdk.test_shared_recipe
./tools/test-sdk.sh python2 --release

# Two independently built distributions:
uv build --wheel --out-dir target/python2/client-wheels crates/zipzi-python2/client
uv build --wheel --out-dir target/python2/native-wheels crates/zipzi-python2
```

The source test runner builds only this adapter and the CPU Runtime fixture,
copies the native artifact into `target/python2/native`, runs a shared recipe in
isolated Python processes, and checks actual valid/invalid public type examples.
It uses macOS extension-module/dynamic-lookup linking, so CPython symbols come
from the importing interpreter. Tests never load Cargo's mutable output in place.

The package currently targets CPython `>=3.14,<3.15`; source acceptance is on
macOS/Unix. Other platform/version claims require separate CI evidence.

## Mesh code generation

The first P2 slice derives mesh values/operations, codecs, query signatures and
simple convenience methods from selected Rust declarations. Public imports stay
`zipzi.meshes` / `zipzi.ops`; lifecycle, resource ownership and recovery stay
handwritten. Local conversion remains direct PyO3, with no JSON roundtrip.

```sh
uv run --locked python crates/zipzi-python2/tools/generate.py --check
uv run --locked python crates/zipzi-python2/tools/generate.py
uv run --locked python tools/test_tools.py -p test_python2_codegen.py -v
```

The source runner checks generated drift before building. Generated files have
explicit ownership markers; the generator cannot overwrite handwritten targets.
Normal wheels consume checked-in outputs and do not run generation at import or
installation. Resource/Plan generation and full source distribution acceptance
remain pending. See the [P2 record](../../docs/plans/python-sdk-rewrite/p2-mesh-generator.md).

## First model

After installing both local wheels:

```python
from pathlib import Path
import zipzi
from zipzi import meshes, ops

with zipzi.local() as runtime:
    session = runtime.sessions.create("79000000-0000-4000-8000-000000000001")
    geometry = meshes.Geometry(
        (42, 7, 99), ((0., 0.), (1., 0.), (0., 1.)),
        ((0., 0.), (1., 0.), (0., 1.)), ((42, 7, 99),),
    )
    with session.edit("Face") as edit:
        face = edit.meshes.create(meshes.MeshSpec("Face", geometry))
        snapshot = edit.meshes.get(face)
        edit.meshes.set_positions(face, vertex_ids=snapshot.geometry.vertex_ids,
                                 positions=((2., 3.), (3., 3.), (2., 4.)))
        edit.apply(ops.Rename(face, "Final"))
    saved = session.project.save(Path("/absolute/output/project")).wait()
```

For a running host, select `zipzi.connect(endpoint=absolute_socket_path)` instead.
Without an endpoint, `connect()` discovers the environment's runtime directory/profile.
Snapshots own immutable tuple
data and exact formal/candidate sources; field access sends no requests. A failed
write makes its candidate uncommittable even when caught. Admission refusal is
distinct from execution failure.

Jobs must be waited or closed to release their bounded result slots. Their actual
result is cached locally. `close()`/context exit waits; it does not claim to cancel
already entered work. Keep an Edit and call `edit.recover()` after a lost write
response, or call `OutcomeUnknown.recover()` on creation, reads and Job startup.
Lost terminal Job queries recover through `job.wait()` or the owning exception.
Recovery uses the original immutable request bytes and request ID.

## Images and source-bound plans

```python
from zipzi import resources

with runtime.resources.prepare_image(1, 1, bytes((12, 34, 56, 255))) as image:
    with session.edit("Texture") as edit:
        asset = edit.resources.create_image("Draft", image)
        with edit.resources.plan(resources.RenameIntent(asset, "Draft", "Texture")) as plan:
            report = plan.report  # Immutable cached value; no request.
            edit.apply_plan(plan)
        pixels = edit.resources.get(asset).pixels
```

Preparation owns an independent lease. Closing it after creating or replacing an
image leaves candidate, formal and history references intact. Use
`edit.resources.replace_image(asset, prepared)` to retain the resource identity
and name while changing its pixels. Pixel snapshots own immutable `bytes`;
`runtime.resources.usage()` reports Runtime-wide lease/plan counts and tracked
Rust image buffers (not process RSS).

`session/edit.meshes.plan(meshes.RenameIntent(...))` and
`session/edit.resources.plan(resources.RenameIntent(...))` create selected typed
Plans. Formal Plans require an initial candidate of the exact session/version;
candidate Plans require the exact Edit and sequence. A Plan must be first in an
explicit batch, followed by ordinary operations. An invalid/stale Plan aborts the
whole candidate; its cached report remains readable, including after `close()`.

Image preparation, reads, Plan creation and handle close also retain their
original recovery owner after a lost response. Call `OutcomeUnknown.recover()`;
another action cannot consume that result. Close/context releases handles only
after a confirmed outcome. Runtime shutdown or client expiry releases remaining
leases. This first control-transport slice limits RGBA payloads to **1 MiB**;
binary buffers and larger images belong to P4.

The native contract is `zipzi-python2-4`. Connected hosts must advertise
`typed_meshes`, `typed_resources_plans` and `typed_job_lifecycle`; a mismatched host is rejected during
connection.

`job.state()` reads status without consuming the result. `job.cancel()` requests
cancellation; queued work reports `CANCELLED`, while already entered IO reports
its actual result. `wait()`/`close()` cache both terminal success and failure.
Recover a lost cancel/status response before waiting or taking another action.

Explicit Connected shutdown releases unsubmitted candidates, prepared images and
Plans while retaining formal sessions. Entered Jobs finish on the host. Closing
with another unresolved request disconnects and leaves cleanup to grace expiry;
it does not send a new write ID or guess the unknown outcome. A lost close
acknowledgment has its own recovery owner.

The lifecycle matrix is recorded in the
[P1 acceptance table](../../docs/plans/python-sdk-rewrite/p1-lifecycle-matrix.md).
Fault gates are compiled only under `test-support`; normal wheels expose neither
fault controls nor allocator probes.

## Desktop host and resident sessions

```python
with zipzi.connect(start=True, host=Path("/absolute/path/to/zipzi-editor")) as runtime:
    metadata = runtime.connection  # Immutable host PID/runtime/build/capabilities/limits.
    for info in runtime.sessions.list():
        session = runtime.sessions.attach(info.id)
        session.show_window()
```

`start=False` is the default. Discovery follows `ZIPZI_RUNTIME_DIR` and
`ZIPZI_RUNTIME_PROFILE`, matching the Rust host. Startup transfers an already
locked endpoint descriptor to a `--no-window` host; a timeout leaves any running
owner intact. Protocol/build mismatch never launches a fallback host. Pass
`sdk_build=` to pin an expected SDK source identity. Recovering a request never
starts a new host. `host=` or `ZIPZI_EDITOR_HOST` chooses an executable; macOS
also searches the user/system `Applications/Zipzi Editor.app` locations.

`SessionId(runtime, serial)` is a pure immutable identity; `session.id` sends no
request. `sessions.list()` and `sessions.attach(id)` work on both backends.
`SessionInfo` includes state/path/version/history. `show_window()` requires a
desktop host. The new client is being connected to Runtime R0–R4; full R4 and
packaged application location acceptance remain pending.

```sh
./tools/test-sdk.sh python2-connected
# Optional current production desktop host, in addition to CPU fault acceptance:
./tools/test-sdk.sh python2-connected --editor-host "$PWD/target/debug/zipzi-editor"
```

The pure wheel runs from `/tmp` under `-I`, without a native package. Editor's
focused `runtime_ipc_acceptance` also uses this installed client. See the
[reconnection record](../../docs/plans/sdk-runtime/python2-r0-r4.md) for the
coverage and remaining event/parameter/modeling work.

## P0 measurements

```sh
./tools/test-sdk.sh python2 --release
./tools/test-sdk.sh python2 --release --instrumented
uv run --locked python crates/zipzi-python2/tools/baseline.py \
  --output target/python2/baseline-first
# Later runs can add --compare docs/plans/python-sdk-rewrite/p0-p1-baseline.json
```

The first measurements cover selected queries, single writes, 100-operation
batches, large position arrays, 256×256 RGBA preparation/create/commit/release,
and formal Plan creation/apply/commit/release. Python JSON/native boundary counts and Python
heap probes are measured in separate processes and instrumented binaries from
latency. Both the adapter and CPU host use frozen Release artifacts. Schema 2
records actual Rust codec calls, requested Rust heap live/peak bytes, selected
explicit payload copies, and array materialization. Copy scope excludes implicit
domain clones and allocator-internal copies; heap scope excludes Python/C heaps,
allocator overhead and realloc transient storage. Connected probes retain an
empty control-window calibration. Noise and regression policies are recorded;
CPU fixture numbers do not claim Editor latency.

Full resource/IO cancellation, animation,
observer/inspection remain planned capabilities. See the
[construction plan](../../docs/plans/python-sdk-rewrite-plan.md) and
[implementation record](../../docs/plans/python-sdk-rewrite/README.md).

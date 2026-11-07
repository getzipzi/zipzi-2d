# Python authoring client and native adapter

The current v4 entrypoint uses the immutable authoring kernel. Both Local and
Connected send the same generated `Operation` and `Command` values. The native
adapter copies owned Python values into the contract before releasing the GIL;
it does not encode and parse JSON for in-process calls.

```python
from uuid import uuid4
import zipzi
from zipzi import values as v

with zipzi.local() as runtime:
    session = runtime.sessions.create()
    parameter = str(uuid4())
    with session.draft("Create parameter") as draft:
        draft.apply(v.CommandCreateParameter(spec=v.ParameterCreateSpec(
            id=parameter, name="Angle", minimum=-30, maximum=30,
            default_value=0,
        )))
    snapshot = session.query(v.QueryObjects(addresses=(
        v.ObjectAddress(kind=v.ObjectKind.PARAMETER, id=parameter),
    )))
    session.undo()
    session.redo()
```

`zipzi.connect(endpoint=...)` uses the same API against a compatible private local
endpoint. The wire major is `zipzi-runtime-v4`; older hosts are rejected before
client registration. There is no `Edit`/field-address compatibility layer.

A Draft carries its full `DraftSource`. Ordinary accepted command failures end it;
`Draft.preview` independently prepares a gesture update and preserves the last
valid candidate on rejection. `apply_async` temporarily makes that Draft busy.
Admission failures (such as an oversized batch or an unsealed Blob) preserve the
candidate. Error responses carry the host's actual Draft state, including when
the response is recovered after a connection loss.
`Job.wait(timeout=...)` does not cancel the job. Session queries continue to read
the resident formal snapshot while workers operate on isolated candidates.

Catch `OutcomeUnknown` and call `recover_call(error)` to resolve the original
call. Recovery preserves its request ID, arguments and result projection. Do not
retry a mutation under a new request ID. Context managers leave an unresolved
call owned so that it can still be recovered, including a lost close response.

The entrypoint covers typed object/collection queries, commands, confirmation
previews, history, events, project open/save/import/export, spatial and animation
sampling, and GPU observation when the host provides it. `runtime.blob(length)`
creates an owned upload; `append(offset, bytes)` and `read(offset, length)` transfer
at most 1 MiB per recoverable call. Seal uploads before `draft.replace_image_blob`.
`session.image_blob` and `job.blob` return owned bulk readback with source metadata.

`QueryMetadata()` reads the complete EditorState, DisplayInfo, Model3Settings,
HierarchyOrder and missing attachment paths at the Snapshot's source. Decode a
root with its generated type's `from_wire` before read/modify/write, so unrelated
settings and opaque extensions survive replacement. Metadata queries share the
normal result size bound and omit geometry and image bytes.

`session.samples(SampleRequest(...))` returns a Job whose blob is UTF-8 JSON with
source/content and ordered frames. `session.observe(input, view)` returns packets:
a four-byte big-endian JSON-header length, that header, then `png_bytes` bytes of
PNG, repeated for each sample. The first header includes the shared texture report.
## Generation and validation

The Contract exporter builds without SDK or Runtime dependencies. It generates
the `values/` package, the installed `_schema.json` codec resource and
`schema/authoring.json` from Rust `ApiShape` declarations. Class and field help
come from the same declarations; generated public source contains no inline
schema dictionary. Constructor defaults are decoded from that resource.
`values/__init__.py` re-exports every type and registers the shared codec once.
Domain modules contain the definitions; the existing `values.Mesh` and the
focused `values.meshes.Mesh` refer to the same class. Cross-domain annotations
use explicit module imports, including cycles, so runtime type-hint inspection
works without constructing a host. Module routing lives in
`tools/codegen/value_modules.py`; fields remain derived from Rust.

The public Python layer lives in `_kernel.py`: Runtime, Sessions, Session, Draft,
Job, Snapshot and Blob. `results.py` describes immutable response keys and query
overloads; `Snapshot.records(Type)` projects object payloads into generated domain
values. `_documentation.py` derives paginated offline help from installed public
objects rather than maintaining another API catalog. The installed guide and
recipes are under `client/python/zipzi/docs/`; recipe workflows are exercised by
both Local and Connected lifecycle tests, including the installed-wheel gate.
The [generated API reference](client/python/zipzi/docs/reference/index.md) ships
beside them. `generate.py` renders domain fields, constructor defaults and enum
members from the same contract/generated declarations, and public signatures,
overloads and docstrings from Python source using AST parsing. Generation does
not import the SDK or native backend. Update Rust contract documentation for
value semantics and Python docstrings for runtime semantics; regenerate rather
than editing `docs/reference/`. The manifest and `--check` cover every reference
page. Guide and recipe prose remains handwritten.
The installed `docs/getting-started.md` covers installation and runnable first
Local/Connected scripts; `docs/lifecycle.md` explains owner lifetimes and
publication/recovery. The first-run scripts are executed by the Python gates.
Independent installed-package agent evaluation is available through the
[v4 diagnostic runner](../../tools/README.md#installed-sdk-agent-evaluation-v4).
It uses isolated CLI participants and a separate artifact/call-observation judge.

```sh
uv run --locked python crates/zipzi-python/tools/generate.py
uv run --locked python crates/zipzi-python/tools/generate.py --check
./tools/test-sdk.sh python
./tools/test-sdk.sh python --recovery
./tools/test-sdk.sh python --discovery
./tools/test-sdk.sh python-connected
./tools/test-sdk.sh python-connected --editor-host /absolute/path/to/zipzi-editor
```

The source gate rebuilds the native adapter and private IPC fixture and checks
generated types. The Connected gate uses an isolated installed wheel. The
optional Editor host runs the same Connected lifecycle cases in the production
desktop process.

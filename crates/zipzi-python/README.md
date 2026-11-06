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

A Draft carries its full `DraftSource`. Ordinary command failures end it;
`Draft.preview` independently prepares a gesture update and preserves the last
valid candidate on rejection. `apply_async` temporarily makes that Draft busy.
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

`session.samples(SampleRequest(...))` returns a Job whose blob is UTF-8 JSON with
source/content and ordered frames. `session.observe(input, view)` returns packets:
a four-byte big-endian JSON-header length, that header, then `png_bytes` bytes of
PNG, repeated for each sample. The first header includes the shared texture report.
## Generation and validation

The Contract exporter builds without SDK or Runtime dependencies. It generates
`values.py` and `schema/authoring.json` from Rust `ApiShape` declarations.

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

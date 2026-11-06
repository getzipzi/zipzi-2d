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

The current slice covers bounded object queries, commands, previews, history,
events and native project open/save. Remaining migration work includes binary
bulk transfers, format/observation job adapters and production Editor integration.
Legacy source files and tests outside the v4 entrypoint are migration inputs,
not supported alternate APIs.

## Generation and focused validation

The Rust Contract exporter has no SDK/Runtime dependency. `values.py` and
`schema/authoring.json` are generated from compiled `ApiShape` declarations;
the manifest controls replacement and retirement of generated files.

```sh
uv run --locked python crates/zipzi-python/tools/generate.py
uv run --locked python crates/zipzi-python/tools/generate.py --check
uv run --locked python tools/test_tools.py -p test_python_codegen.py
cargo build --locked -p zipzi-runtime --example local_host_fixture
./tools/dev-python crates/zipzi-python/tests/test_kernel.py
```

The fixture exercises real local IPC and intentionally dropped responses; it is
not a substitute for the production Editor-host gate. The broad Python gate and
older examples are still being migrated. Use the repository development launcher
rather than global Python or a custom `PYTHONPATH` for native tests.

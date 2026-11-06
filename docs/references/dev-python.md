# dev-python (internal)

Developer-only reference for human/agent SDK experiments in this checkout.
Requires the [workspace development prerequisites](../../README.md#development).

From the repository root:

Windows PowerShell (also available on macOS):

```powershell
uv run --locked python tools/dev.py python path/to/probe.py argument
uv run --locked python tools/dev.py python -c 'import zipzi; print(zipzi.values.CONTRACT)'
```

macOS shell wrapper:

```sh
./tools/dev-python /tmp/probe.py argument
./tools/dev-python -c 'import zipzi; print(zipzi.values.CONTRACT)'
./tools/dev-python - <<'PY'
import zipzi
with zipzi.local() as runtime:
    print(runtime.sessions.list())
PY
```

No arguments opens a REPL; `-m module` also works. From another directory use
the entrypoint's absolute path. Script paths, outputs, stdin, arguments and exit
status retain normal Python behavior. Keep disposable scripts/results under
`target/python/scratch/` or a temporary directory.

The entrypoint maintains CPython 3.14 under `target/python/dev/venv`, installs
the client editable, and incrementally builds the native adapter under
`target/python/dev/cargo` before every launch, including uncommitted changes.
It publishes the extension into a content-addressed version directory and
atomically selects that directory for new Python processes. Existing processes
retain their loaded version; old versions are not automatically deleted.
This avoids overwriting loaded Windows `.pyd` files. Failed preparation
stops execution. Concurrent launches serialize preparation, then run independently.
Build output goes to stderr.

Use this entrypoint instead of bare Python, global installation or custom
`PYTHONPATH`. It clears `PYTHONPATH`/`PYTHONHOME`; `-S` disables the editable client.
Restart existing scripts after source changes. Connected mode preserves host
discovery settings and uses the selected host; rebuild/restart Editor separately.

This source environment is separate from test fixtures and does not validate
wheel installation. Formal checks remain `./tools/test-sdk.sh python` and
`./tools/test-sdk.sh python-connected`.

### Connect to the development Editor

The Python launcher preserves host selection; it does not choose among running
Editors. If the development Editor was started with profile `dev`, select it
explicitly rather than using the SDK's `default` profile:

```sh
ZIPZI_RUNTIME_PROFILE=dev ./tools/dev-python target/python/scratch/probe.py
```

Or pass the absolute endpoint to `zipzi.connect(endpoint=...)`. On macOS the
usual development endpoint is `/tmp/zipzi-runtime-<uid>/dev.sock`. An unavailable
Unix endpoint error includes the attempted address and host-selection guidance;
it never falls back to a different Editor or starts one implicitly.

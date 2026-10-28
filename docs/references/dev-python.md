# dev-python (internal)

Developer-only reference for human/agent SDK experiments in this checkout.
Requires the [workspace development prerequisites](../../README.md#development).

From the repository root:

Windows PowerShell (also available on macOS):

```powershell
uv run --locked python tools/dev.py python path/to/probe.py argument
uv run --locked python tools/dev.py python -c 'import zipzi; print(zipzi.describe("zipzi"))'
```

macOS shell wrapper:

```sh
./tools/dev-python /tmp/probe.py argument
./tools/dev-python -c 'import zipzi; print(zipzi.describe("zipzi"))'
./tools/dev-python - <<'PY'
import zipzi
with zipzi.local() as runtime:
    print(runtime.sessions.list())
PY
```

No arguments opens a REPL; `-m module` also works. From another directory use
the entrypoint's absolute path. Script paths, outputs, stdin, arguments and exit
status retain normal Python behavior. Keep disposable scripts/results under
`target/python2/scratch/` or a temporary directory.

The entrypoint maintains CPython 3.14 under `target/python2/dev/venv`, installs
the client editable, and incrementally builds the native adapter under
`target/python2/dev/cargo` before every launch, including uncommitted changes.
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
wheel installation. Formal checks remain `./tools/test-sdk.sh python2` and
`./tools/test-sdk.sh python2-connected`.

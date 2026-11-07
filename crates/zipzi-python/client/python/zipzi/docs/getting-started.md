# Install and run your first script

This guide uses only the installed packages and their bundled documentation.
Requires CPython 3.14. The Python client and the host must support the same
`zipzi-runtime-v4` contract.

## Choose Local or Connected

| Your task | Entry point | Install | What owns the document? |
| --- | --- | --- | --- |
| Create or process files in a script without an Editor window | `zipzi.local()` | `zipzi-native` and its matching `zipzi` dependency | Your script's Runtime; save before closing it |
| Inspect or edit a document in a running Editor | `zipzi.connect()` | `zipzi` | The host's resident Session; disconnecting leaves the document there |

Both modes use the same commands, queries and Draft API. A Local Runtime is a
separate host: it does not attach to documents already open in the Editor.
Editor-specific services require a Connected Editor; inspect
`runtime.capabilities()` before using optional services.

## Install into a dedicated environment

Create an environment using your CPython 3.14 executable:

```sh
python3.14 -m venv .venv
```

Activate it with `source .venv/bin/activate` on macOS/Linux, or
`.venv\Scripts\Activate.ps1` in Windows PowerShell. Verify that `python --version`
reports 3.14. For a published alpha release, install the package for your mode:

```sh
# Connected: pure Python client, no native adapter required.
python -m pip install --pre zipzi

# Local: also installs the matching client as a dependency.
python -m pip install --pre zipzi-native
```

These index commands require a published release with a wheel for your platform.
For an unreleased build, install the supplied wheels instead. Use the matching
client/native pair from the same distribution; the native wheel must match your
OS, architecture and CPython version:

```sh
python -m pip install /absolute/path/to/zipzi-<version>-py3-none-any.whl
# For Local, additionally install the corresponding supplied native wheel:
python -m pip install /absolute/path/to/zipzi_native-<version>-<platform-tags>.whl
```

Replace the placeholder filenames with the actual files you received. The
examples below use `python` from this activated environment. No repository
checkout or `PYTHONPATH` setting is needed.

Check the installed client before starting a host:

```python
import zipzi
from importlib.metadata import version

print(version('zipzi'))
print(zipzi.describe('Session.query').format())
```

## First Local run: create, query and save

Save the following as `first_local.py`. It creates one parameter, publishes one
edit, reads it back, and saves a new project. Pass a destination whose parent
directory exists and whose file does not yet exist.

<!-- example: first-local -->
```python
import argparse
from pathlib import Path
from uuid import uuid4
import zipzi
from zipzi import values as v

parser = argparse.ArgumentParser()
parser.add_argument('destination', type=Path)
args = parser.parse_args()
destination = args.destination.resolve()

with zipzi.local() as runtime:
    session = runtime.sessions.create()
    parameter_id = str(uuid4())
    with session.draft('Create Angle') as draft:
        draft.apply(v.CommandCreateParameter(spec=v.ParameterCreateSpec(
            id=parameter_id, name='Angle', minimum=-30, maximum=30,
            default_value=0,
        )))

    snapshot = session.query(v.QueryObjects(addresses=(
        v.ObjectAddress(kind=v.ObjectKind.PARAMETER, id=parameter_id),
    )))
    parameter, = snapshot.records(v.Parameter)
    print(parameter.id, parameter.name, parameter.default_value)

    with session.save(destination) as job:
        receipt = job.wait()
    if not receipt['published']:
        raise RuntimeError('Project destination was not published')
    print(receipt['path'], 'durable:', receipt['durable'],
          'warnings:', receipt['warnings'])
```

```sh
python first_local.py ./first-model.zipzi
```

Expect `Angle`, its stable UUID and default value `0`, followed by the saved
absolute path, durability flag and warnings. Closing Local releases its resident
documents; the saved file remains. Check `durable` and `warnings` even when
`published` is true. Repeating this script with the same destination is refused
with `PROJECT_EXISTS`; use a new destination for another trial. Updating an
existing file requires opening it into a Session and then using `session.save()`.

## First Connected run: select and inspect a document

Start a compatible Editor and open or create a document. Save this script as
`first_connected.py`. Its default action lists resident documents. Supplying a
session ID reads that exact document without editing it.

<!-- example: first-connected -->
```python
import argparse
from pathlib import Path
import zipzi
from zipzi import values as v

parser = argparse.ArgumentParser()
parser.add_argument('--endpoint', type=Path)
parser.add_argument('--session')
args = parser.parse_args()

with zipzi.connect(endpoint=args.endpoint) as runtime:
    if args.session is None:
        for info in runtime.sessions.list():
            print(info['session'], info['document_id'], info['path'],
                  'dirty:', info['dirty'])
    else:
        session = runtime.sessions.attach(args.session)
        snapshot = session.query(v.QuerySummary())
        print('session:', session.id, 'document:', snapshot.value['id'],
              'parameters:', snapshot.value['parameters'])
```

```sh
python first_connected.py
python first_connected.py --session <session-id-from-the-list>
```

Replace `<session-id-from-the-list>` with the selected ID. Use the document ID
and path to decide which document you intend to access; names and list order are
not reliable selectors. No output from the first command means there are no
resident documents. Open/create one in the Editor and list again.

By default, `connect()` discovers a running host; it does not launch one. When
you have an explicit private endpoint, pass `--endpoint <endpoint>` on both
commands. `zipzi.connect(start=True, host=Path(...))` can instead launch an
explicit Editor executable when needed. The executable path must be absolute.
SDK file paths refer to the host's filesystem.

Closing the Connected Runtime releases this client's resources and attachments;
it does not save or close the Editor's documents. To edit the selected Session,
follow the [read/edit/verify workflow](guide.md#read-edit-and-verify).

## If the first run fails

| Symptom | Next step |
| --- | --- |
| `ModuleNotFoundError: zipzi` | Use the same environment for `python -m pip` and your script |
| `LOCAL_BACKEND_MISSING` | Install a matching native wheel into that environment, or use Connected |
| No compatible host can be discovered | Start the Editor, or supply its explicit private endpoint |
| `PROTOCOL_MISMATCH` | Use a compatible client/host pair from the supplied distribution |
| `PROJECT_EXISTS` on the Local example | Choose a new output file; this example intentionally creates a new project |
| `PROJECT_BUSY` when opening a file | Its resident owner must close before another Session on that host can open it; detaching alone retains ownership |

Read [ownership and lifetimes](lifecycle.md) before building longer workflows.
The [task guide](guide.md), [recipes](recipes.py) and
[API reference](reference/index.md) are all bundled with the client. Locate them
with `importlib.resources.files('zipzi').joinpath('docs')`.

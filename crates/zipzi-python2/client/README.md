# zipzi

A typed Python client for Zipzi 2D, for
modeling, animation and programmatic collaboration with the Zipzi Editor.

## Install

Requires CPython 3.14. To install the client, allowing alpha releases:

```sh
python -m pip install --pre zipzi
```

Connect to a running Editor with `zipzi.connect()`. To use the in-process
`zipzi.local()` Runtime instead, install `zipzi-native` as well:

```sh
python -m pip install --pre zipzi-native
```

The native package installs its matching client and has wheels prepared for
macOS arm64 and Windows x86-64. PyPI installation requires the corresponding
release to have been published. The Editor is available from
[GitHub Releases](https://github.com/getzipzi/zipzi-2d/releases).

## Connect to an Editor

```python
import zipzi

with zipzi.connect() as runtime:
    for info in runtime.sessions.list():
        print(info.id, info.path)
        # session = runtime.sessions.attach(info.id)
```

Attach a session by ID to read and edit its document. Edits share the Editor's
undo history; saving is separate. `zipzi.local()` provides the same Runtime API
without an Editor window.

For examples and API details, see the [package README](python/zipzi/README.md),
[usage guide](python/zipzi/docs/guide.md),
[API reference](python/zipzi/docs/reference.md)
and [recipes](python/zipzi/docs/recipes.py).
These documents are also included in the installed package. Locate the local
README without a host or network connection:

```python
from importlib.resources import files

print(files("zipzi").joinpath("README.md").read_text(encoding="utf-8"))
```

For offline API discovery, use `zipzi.describe("zipzi").format()`.

If a response is lost, recover the original call with `OutcomeUnknown.recover()`
or `zipzi.recover_call()` rather than sending it again. A timed-out `job.wait()`
does not cancel the job. The guide covers recovery and resource ownership.

The client is [MIT licensed](LICENSE-MIT). See the
[third-party notices](THIRD_PARTY_NOTICES.md) for dependency terms.

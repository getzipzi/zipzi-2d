# Zipzi Python SDK

Start with [installation and your first script](docs/getting-started.md) to
choose Local or Connected and complete a runnable example. Read
[ownership and lifetimes](docs/lifecycle.md) for Session, Draft, Snapshot,
Job and Blob cleanup, publication, timeouts and recovery.

The [workflow guide](docs/guide.md) covers host/document selection,
pagination, editing and saving. The
[executable recipes](docs/recipes.py) cover a complete textured scene, parameter
editing, animation sampling and GPU readback with either Local or Connected.

The [generated API reference](docs/reference/index.md) lists public methods,
overloads, response shapes and domain values, including field defaults and
enum members. It ships with this package and matches its version.

Find API details offline, without a native backend or running Editor:

```python
import zipzi

print(zipzi.describe().format())
print(zipzi.describe('Session.query').format())
print(zipzi.describe('values.CommandCreateParameter').format())
print(zipzi.describe('results.SaveReceipt').format())
```

Follow `children` paths; continue with `offset=entry.next_offset` when present.
Generated commands and domain values live in `zipzi.values`; read-only query and
save response shapes live in `zipzi.results`. `snapshot.records(values.Parameter)`
decodes record payloads into immutable values with named attributes. `repr` is a
preview; `.to_wire()` returns the complete content.

Values are organized by domain. Browse `zipzi.describe('values.meshes')`, or
import `Mesh` from `zipzi.values.meshes`. The unified `zipzi.values.Mesh` entry
remains available and refers to the same class.

Locate installed documents with `importlib.resources.files('zipzi')`. Local
sessions require the matching optional `zipzi-native` package; Connected sessions
use a private endpoint on a v4 host.

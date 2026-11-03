# Zipzi Python SDK: installed documentation

These documents ship with this client and can be read without a network
connection, native backend or running Editor. All links below are relative to
this installed package:

- [Workflow guide](docs/guide.md): host selection, sessions, edits, recovery and resource ownership.
- [API reference](docs/reference.md): installed types, signatures and domain documentation.
- [Recipes](docs/recipes.py): executable authoring examples.
- [Mesh quality](docs/mesh-quality.md): inspection, overlays and coverage.
- [Animation and spatial workflow](docs/animation-spatial.md).
- [Editor handoff example](docs/editor_handoff.py).

Locate this README and the documentation directory from Python:

```python
from importlib.resources import files

package = files("zipzi")
print(package.joinpath("README.md").read_text(encoding="utf-8"))
print(package.joinpath("docs"))
```

Discover API entries offline:

```python
import zipzi

print(zipzi.describe("zipzi").format())
print(zipzi.describe("Session.meshes.list").format())
```

Use `zipzi.connect()` to connect to an Editor, or install the matching optional
`zipzi-native` package to use `zipzi.local()`. Read the workflow guide before
editing a shared document; saving and releasing owned resources are explicit
operations.

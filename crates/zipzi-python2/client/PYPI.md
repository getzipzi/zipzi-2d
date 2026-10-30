# Zipzi Python SDK

Build and animate [Zipzi 2D](https://github.com/getzipzi/zipzi-2d) models from
Python and collaborate with people or AI agents in the Zipzi Editor.

Requires **CPython 3.14**. Install the typed pure Python client:

```sh
python -m pip install --pre zipzi
```

Connect to a running Editor with `zipzi.connect()`. To use the in-process
Runtime with `zipzi.local()`, install the matching optional native backend:

```sh
python -m pip install --pre zipzi-native
```

`zipzi-native` installs its matching client as a dependency. Native wheels cover
macOS arm64 and Windows x86-64. The desktop Editor is available from
[GitHub Releases](https://github.com/getzipzi/zipzi-2d/releases).

## Read the installed documentation offline

The client includes a README, workflow guide, API reference and executable
recipes matching its installed version. Locate them without a network
connection, native backend or running Editor:

```python
from importlib.resources import files

package = files("zipzi")
print(package.joinpath("README.md").read_text(encoding="utf-8"))
print(package.joinpath("docs/guide.md"))
```

Follow the relative links in the installed README to read the local guide,
reference and recipes. For individual API entries:

```python
import zipzi

print(zipzi.describe("zipzi").format())
print(zipzi.describe("Session.meshes.list").format())
```

The Python client and native Runtime backend are MIT licensed. Editor services
run in the GPL-3.0-only Editor. See the
[project repository](https://github.com/getzipzi/zipzi-2d) for source, licensing
and contribution information.

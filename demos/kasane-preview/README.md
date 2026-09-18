# Kasane memory preview

Stage 00–03 runnable sample: left is a `KasaneMeshView` fed directly from
memory; right is a `Document` updated through stable vertex IDs and synchronized
through `KasaneDocumentBridge`. Both animate with fixed topology and reuse their
render resources. The four-color texture is generated in memory; a matching
resource-backed texture supports the checked-in save/reopen sample.

No Cubism SDK, moc3 model, external image, file export, or manual editing tools
are required.

From the repository root:

```sh
python3 tools/run_kasane_preview.py --render
python3 tools/run_kasane_preview.py --skip-build --demo
```

Dependencies: CMake, a C++20 compiler, Python with SCons, the pinned godot-cpp
submodule, and Godot 4.3 or later. Use `--scons-python`/`SCONS_PYTHON` and
`--godot`/`GODOT_BIN` for custom installations. macOS arm64 is the verified host;
other platform mappings are not yet validated.

The default run builds both modules and runs core plus headless integration
checks. `--render` additionally opens a temporary GPU-rendered window, checks
UV orientation and before/after geometry, and captures the animated demo.
`--skip-build` assumes a previous successful build, including CMake tests.

- Logs: `target/kasane/` at the repository root.
- Screenshots: `artifacts/before.png`, `artifacts/after.png`, `artifacts/demo.png`.
- Fixed input and registration example: [fixture.gd](fixture.gd).
- [Architecture and API](../../docs/editor/ARCHITECTURE.md).
- [Document specification](../../docs/editor/DOCUMENT_SPEC.md).
- [Project format v1](../../docs/editor/PROJECT_FORMAT.md).

The `get_mesh_view()` accessor is diagnostic: the bridge owns those child nodes.
Do not free or edit them directly. Use Document mutations and `rebuild_preview()`.

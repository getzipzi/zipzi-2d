# gd-kasane

Internal Godot bindings for Kasane Editor, not a user-installed addon.

- `KasaneDocumentBridge` is a `RefCounted` source-data owner, independent of the
  scene tree. Mesh/Deformer handles reference its stable IDs and generation.
- `KasaneDocumentPreview` is a separate Node2D consuming `DrawableFrame`.
- `KasaneTextureStore` owns loaded Texture2D resources.
- `KasaneProjectIO` handles source snapshot persistence, without texture loading
  or node creation. The current prototype file format is version 4; versions
  1–3 are explicitly rejected. This is not M2 packaged-project acceptance.

Source edits emit object changes. Temporary parameter overrides emit a separate
preview signal, never modify Document revision, and are not persisted. Preview
failures do not roll back or fail successful source edits. The current MeshView
adapter will be replaced by the shared renderer in M4.

Build with the pinned godot-cpp and a Python environment containing SCons:

```sh
python3 -m SCons -C modules/gd-kasane platform=macos arch=arm64 target=template_debug -j8
python3 tools/validate_m1_godot.py
```

The test stages a disposable project under `target/kasane/godot-boundary`, using
only in-memory image fixtures. It checks source/preview ownership and file
boundaries headlessly, not GPU image equivalence. Override `--godot` or
`--library` for other installed macOS arm64 binaries.

The source module includes Purism's shared keyform helper header but does not
link a Core runtime library. Editor packaging and full script-interface
acceptance remain in M5. See [refactor and API migration](../../docs/editor/M1-CORE-REFACTOR.md)
and [engineering roadmap](../../docs/editor/ROADMAP.md).

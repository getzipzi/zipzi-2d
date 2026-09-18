# gd-kasane

Standalone Godot GDExtension with `KasaneMeshView` and `KasaneDocumentBridge`.
Uses the existing pinned godot-cpp dependency but does not load/link Cubism.

Use `python3 tools/run_kasane_preview.py --render` from the repository root to
build, import, and verify. The SCons target places its library in the sample's
`addons/gd_kasane/bin/` directory. No third-party SDK discovery runs in this build.

See [architecture and API](../../docs/editor/ARCHITECTURE.md) and
[runnable sample](../../demos/kasane-preview/README.md).

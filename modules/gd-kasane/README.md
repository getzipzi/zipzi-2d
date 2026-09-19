# gd-kasane

Internal C++ Godot bindings for Kasane Editor. This module is not a user-facing
addon or a standalone product. The desktop Editor will bundle its native
bindings; users will not create a Godot project or install this module.

The current `src/` contains prototype Document handles, `KasaneDocumentBridge`
and mesh preview code. These can be reworked for the new Document and shared
renderer. The old demo, script host, Action helper and prototype tests have
been removed.

For development, build the current native library (macOS arm64 example):

```sh
cd modules/gd-kasane
python3 -m SCons platform=macos arch=arm64 target=template_debug -j8
```

Select platform and arch for your host. The pinned godot-cpp and Python SCons
are required; this module does not link Cubism. Output goes to `build/bin/`
and is an internal build artifact, not a runnable Editor or installable addon.

The Editor project, GDExtension loading configuration and application packaging
will be implemented together in [M5](../../docs/editor/milestones/M5-agent-editor.md).
No generic addon staging tool or distribution package is maintained here.
See [current implementation](../../docs/editor/ARCHITECTURE.md) and
[engineering targets](../../docs/editor/ROADMAP.md).

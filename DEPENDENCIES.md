# Dependency revisions

- GPUI Kit 0.6.5: official `https://github.com/longbridge/gpui-kit`, pinned to `5059de47a6560fdbe24e32cd91b45d6aa6849695`.
- GPUI 0.3.6: three crates patched through root `[patch.crates-io]`: `gpui-pre`, `gpui-pre-macos`, `gpui-pre-apple`. Snapshot: Zed `bcf6582ce3500df93a8a39366640173e6786cea6`.
- Kasane: local `../kasane-2d`, at revision `65c19c938693425ee9dd223e6cf3aa1f60a0f459` or a compatible descendant. This includes `kasane-render-metal` and its backend-neutral changes, plus persistent `HierarchyOrder` metadata and `EditSession::replace_hierarchy_order` for palette sorting.
- Rendering: native Metal via `metal 0.33`, with GPUI's device and command queue shared by Kasane.
- Authoring: `editor-core` depends on the local `kasane-sdk` for document transactions, history and project IO, and `kasane-project` for decoding validated assets. It has no GPU dependency. `editor-render` owns the direct `kasane-render` / `kasane-render-metal` dependencies.

All workspace crates use the root `Cargo.lock`. Build and test with `--locked`.
The editor supports macOS. There are no application backend switches or CPU
readback fallback. No sibling GPUI checkout is required.

GPUI Kit has dependencies for other target platforms, which Cargo may retain in
the cross-platform lockfile. This does not make them active macOS dependencies.
See `vendor/README.md` for the exact local patch scope.

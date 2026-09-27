# Dependency revisions

- GPUI Kit 0.6.5: official `https://github.com/longbridge/gpui-kit`, pinned to `5059de47a6560fdbe24e32cd91b45d6aa6849695`.
- GPUI 0.3.6: three crates patched through root `[patch.crates-io]`: `gpui-pre`, `gpui-pre-macos`, `gpui-pre-apple`. Snapshot: Zed `bcf6582ce3500df93a8a39366640173e6786cea6`.
- Kasane: local `../kesane-2d`, including `kasane-render-metal` and its backend-neutral changes. These changes were uncommitted when integrated; the earlier revision `b7aab701b977ce85febeab9d313f463e5ef7b344` alone does not contain the required backend.
- Rendering: native Metal via `metal 0.33`, with GPUI's device and command queue shared by Kasane.

All workspace crates use the root `Cargo.lock`. Build and test with `--locked`.
The editor supports macOS. There are no application backend switches or CPU
readback fallback. No sibling GPUI checkout is required.

GPUI Kit has dependencies for other target platforms, which Cargo may retain in
the cross-platform lockfile. This does not make them active macOS dependencies.
See `vendor/README.md` for the exact local patch scope.

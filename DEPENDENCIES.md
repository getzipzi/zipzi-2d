# Dependency revisions

- GPUI Kit 0.6.5: official `https://github.com/longbridge/gpui-kit`, pinned to `5059de47a6560fdbe24e32cd91b45d6aa6849695`.
- GPUI family: `gpui-pre 0.3.6`, selected by Kit; no local compatibility patches.
- kasane-2d: local `../kesane-2d`, validated revision `b7aab701b977ce85febeab9d313f463e5ef7b344`.
- Kasane renderer: wgpu 29, independent device; CPU readback bridge into GPUI images.

All three workspace crates use the root `Cargo.lock`. Use `--locked` when building and testing. No gpui-ce dependency or sibling GPUI checkout is required.

The former gpui-ce shared-texture experiment is preserved in Git history through `3d1ef63`; its separate fork is no longer part of this workspace's build.

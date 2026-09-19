# Runtime monorepo

## Layout

- `modules/purism-core/` — forked PurismCore provider, pinned as a Git
  submodule.
- `modules/gd-cubism/` — `gd_cubism` Godot GDExtension, imported from
  `gd_cubism` and adapted to support an alternate Cubism Core implementation.
- `modules/kasane-core/` — independent C++20 source Document and geometry validation.
- `modules/gd-kasane/` — native in-memory mesh rendering and Document bridge;
  builds without the Cubism SDK or Framework.
- `demos/godot/` — interactive Godot comparison and regression demo.
- `demos/kasane-preview/` — native memory/Document preview and GPU checks.
- `benchmarks/cubism-matrix/` — reproducible benchmark matrix between official
  Cubism Core and PurismCore.
- `third_party/` — local, untracked third-party SDKs shared by modules and apps.
- `tools/` — repository-wide development and staging utilities.
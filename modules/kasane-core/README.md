# kasane-core

C++20 editable source data, reference-aware editing and pure in-memory evaluation.
Document has no Godot, runtime model, texture resource or filesystem dependency.
Purism supplies shared keyform, Rotation/Warp and nested-direction algorithms.

```sh
# CMake, C++20 compiler and libpng development files are needed for all targets.
cmake -S modules/kasane-core -B target/kasane/core -DCMAKE_BUILD_TYPE=Debug
cmake --build target/kasane/core -j8
ctest --test-dir target/kasane/core --output-on-failure
```

- `model.hpp`: Canvas, ImageAsset, Part, Mesh, Transform (Rotation/Warp), Parameter,
  MeshBinding and SceneBinding with explicit complete Cartesian Keyforms.
- `document.hpp`: identity, CRUD, atomic geometry/Keyform updates, reference-aware
  deletion, cycle checks, object changes and revisions.
- `evaluation.hpp`: pure `evaluate_frame` returning DrawableFrame. Temporary
  parameter values stay outside persistent source data.
- `moc3.hpp` / `kasane_moc3`: compile source into MOC3 v5 and resource descriptions.
  No Core ABI or filesystem dependency.
- `package.hpp` / `kasane_package`: libpng validation, a required application
  runtime-validation callback and staged whole-directory publication.
- `legacy_deformer.hpp`: explicitly isolated prototype data. Formal evaluation
  and export reject legacy deformers, requiring explicit migration.

Root positions use canvas pixels. Rotation children use local runtime units;
Warp children use normalized grid coordinates, including extrapolation outside
`[0,1]²`. Reparenting does not implicitly transform source geometry.

The core does not own an undo stack. See [coordinate/format mapping](../../docs/editor/formats/MOC3-WRITER.md),
[API boundaries](../../docs/editor/M1-CORE-REFACTOR.md) and
[M1 acceptance](../../docs/editor/M1-ACCEPTANCE.md) for reproducible tests and limits.

```sh
python3 tools/validate_m1_core.py
# Complete macOS arm64 acceptance, including rebuilding Godot libraries and GPU:
target/kasane/buildenv/bin/python tools/validate_m1.py
```

# kasane-core

C++20 editable source data, validation, and in-memory evaluation. No Godot,
Cubism runtime library, runtime model, textures, or files are needed to edit or
evaluate. The pinned Purism submodule supplies the shared `PurismKeyform.h`
algorithm header; it is used by both this evaluator and the Purism runtime.

```sh
cmake -S modules/kasane-core -B target/kasane/core -DCMAKE_BUILD_TYPE=Debug
cmake --build target/kasane/core -j8
ctest --test-dir target/kasane/core --output-on-failure
```

- `model.hpp`: source types (Canvas, ImageAsset, Mesh, Parameter, MeshBinding,
  explicit MeshKeyform combinations).
- `document.hpp`: source ownership, validation, reference-aware editing/deletion,
  atomic Keyform/topology updates, object change sets and revisions.
- `evaluation.hpp`: pure `evaluate_frame`, returning a renderer/data-inspection
  `DrawableFrame`. Preview values are separate from persistent source data.
- `moc3.hpp`: independent `kasane_moc3` target encoding new MOC3 v5 bytes and
  resource descriptions. Source data is recompiled on each export.
- `legacy_deformer.hpp`: isolated prototype Rotation/Warp representation. Its
  approximate evaluator is explicitly named `evaluate_legacy_mesh` and is not
  used by formal evaluation, preview, or export.

Direct writes do not create history. The core does not own an undo stack.

Reproduce static and parameter/Keyform comparisons against both Core providers:

```sh
python3 tools/validate_m1_core.py
```

The full M1 milestone remains unaccepted. See [M1](../../docs/editor/milestones/M1-document-moc3.md),
[refactor/API boundaries](../../docs/editor/M1-CORE-REFACTOR.md), and
[MOC3 field mapping](../../docs/editor/formats/MOC3-WRITER.md) for supported
operations, coordinates, capacities, tests, and pending work.

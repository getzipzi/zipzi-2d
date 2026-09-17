# Runtime monorepo

## Layout

- `modules/purism-core/` — forked PurismCore provider, pinned as a Git
  submodule.
- `modules/gd-cubism/` — `gd_cubism` Godot GDExtension, imported from
  `gd_cubism` and adapted to support an alternate Cubism Core implementation.
- `demos/godot/` — interactive Godot comparison and regression demo.
- `benchmarks/cubism-matrix/` — reproducible benchmark matrix between official
  Cubism Core and PurismCore.
- `third_party/` — local, untracked third-party SDKs shared by modules and apps.
- `tools/` — repository-wide development and staging utilities.

Initialize submodules after cloning:

```sh
git submodule update --init --recursive
```

PurismCore uses CMake and CTest:

```sh
cmake -S modules/purism-core -B target/cubism-matrix/core/purism-v6 \
  -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DPURISM_CORE_ABI=v6 \
  -DPURISM_CORE_BUILD_TESTS=ON
cmake --build target/cubism-matrix/core/purism-v6 -j8
ctest --test-dir target/cubism-matrix/core/purism-v6 --output-on-failure
```

Build and test the Godot integration with PurismCore:

```sh
tools/run_cubism_core_experiment.sh
```

The proprietary Cubism Native SDK and the demo model are deliberately not
tracked. Put the SDK under `third_party/CubismSdkForNative-5-r.5/` and the model
under `demos/godot/assets/live2d/`. The canonical Godot addon lives under
`modules/gd-cubism/addons/`; `tools/stage_godot_addon.py` stages it into the
demo and benchmark projects. See the demo, benchmark, and ABI READMEs for build
and test commands.

## Imported sources

- `modules/gd-cubism/` was imported from `pakholeung37/gd_cubism` at commit
  `36ad060`.
- The Godot demo and ABI experiment were imported from
  `pakholeung37/manosaba-live2d` at commit `b6f8820`.

Those source repositories were imported as files rather than nested Git
repositories. Their commit IDs above provide a stable provenance point.

# kasane-core

Independent C++20 source model and geometry validation. No Godot or Cubism
headers, libraries, or files are required.

```sh
cmake -S modules/kasane-core -B target/kasane/core -DCMAKE_BUILD_TYPE=Debug
cmake --build target/kasane/core -j8
ctest --test-dir target/kasane/core --output-on-failure
```

Run from the repository root. See the [Document specification](../../docs/editor/DOCUMENT_SPEC.md)
for identity, coordinates, ownership, mutation semantics, and stage limits.

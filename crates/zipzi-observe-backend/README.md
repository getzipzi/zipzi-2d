# zipzi-observe-backend

Selects the optional observation renderer dependencies: native Metal on macOS,
WGPU on other platforms, and WGPU on macOS with `wgpu-backend`. The crate only
re-exports renderer APIs; importing it does not initialize a GPU.

`zipzi-observe` enables this crate through its `gpu` feature. Keeping platform
selection behind that optional dependency allows CPU planning to exclude both
backends without enabling WGPU in default macOS builds. Cargo features are
additive, so platform selection and the optional GPU boundary live in separate
packages.

Check the dependency matrix with:

```sh
uv run --locked python tools/test_tools.py -p test_runtime_dependencies.py
cargo check --locked -p zipzi-observe
cargo check --locked -p zipzi-observe --features wgpu-backend
cargo check --locked -p zipzi-observe --no-default-features
```

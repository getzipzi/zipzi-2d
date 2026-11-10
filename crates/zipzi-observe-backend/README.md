# zipzi-observe-backend

Selects observation renderer dependencies. Defaults are native Metal on macOS,
native D3D11 on Windows, and wgpu elsewhere. The `wgpu-backend` feature selects
wgpu for comparison. The crate re-exports renderer APIs. GPU initialization
happens when an observation context is created.

`zipzi-observe` enables this crate through its `gpu` feature. Keeping platform
selection behind that optional dependency keeps CPU planning independent of the
GPU backends.

Check the dependency matrix with:

```sh
uv run --locked python tools/test_tools.py -p test_runtime_dependencies.py
cargo check --locked -p zipzi-observe
cargo check --locked -p zipzi-observe --features wgpu-backend
cargo check --locked -p zipzi-observe --no-default-features
```

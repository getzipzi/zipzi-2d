# zipzi-api-derive

Internal compile-time descriptions for explicitly selected Rust authoring types.
The derive emits `zipzi_core::api_shape::ApiShape`; the Python exporter chooses
its finite input/output roots. It neither validates documents nor implements
SDK behavior. Selected serde defaults are evaluated in Rust during export.

Unreviewed serde representations fail compilation. The client and its native
adapter are generated before packaging; installs perform no reflection or code
generation. SDK components use the workspace MIT license.

Focused verification:

```sh
uv run --locked python tools/test_tools.py -p test_python2_codegen.py
uv run --locked python crates/zipzi-python2/tools/generate.py --check
```

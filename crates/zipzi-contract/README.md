# zipzi-contract

The shared Rust definitions for authoring data, API shape metadata, binary
wire codecs and copy-on-write sequences. Core, SDK and Runtime re-export these
same types; operation declarations are bound to Runtime handlers with matching
input and output types. This crate does not depend on execution packages.

The `authoring-schema` binary selects the Python projections and emits their
schema. `crates/zipzi-python2/tools/generate.py` always exports the current
contract before rendering and uses a checkout-local Cargo target and lock.

The `model`, `document`, `authoring`, `runtime` and `binary_wire` module files
include their domain-focused source files in the same module; their shared
imports live in the module files. This preserves Rust type paths and binary wire
signatures when organizing definitions. Rustfmt does not follow `include!`;
when editing these files, format them explicitly, for example:

```sh
rustfmt --edition 2021 --config skip_children=true crates/zipzi-contract/src/{model,document,authoring,runtime,binary_wire}/*.rs
```

Run focused checks with `cargo test --locked -p zipzi-contract` and
`uv run --locked python tools/test_tools.py -p test_python2_codegen.py`.

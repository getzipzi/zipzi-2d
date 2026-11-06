# zipzi-contract

The shared Rust definitions for authoring data, API shape metadata, binary
wire codecs and copy-on-write sequences. Core, SDK and Runtime re-export these
same types; operation declarations are bound to Runtime handlers with matching
input and output types. This crate does not depend on execution packages.

`kernel` contains the new authoring kernel's content/resource identities, publication
and candidate sources, and path-independent image records. These preparatory values
are not yet exported by the current wire/schema; protocol and clients will switch
with the SDK/Runtime implementation. They do not own sessions or resource leases.

The `authoring-schema` binary selects the Python projections and emits their
schema. `crates/zipzi-python/tools/generate.py` always exports the current
contract before rendering and uses a checkout-local Cargo target and lock.

Maintain shared type, operation-variant and field semantics in their `///`
comments. Both derived shapes and `api_record!` / `api_union!` declarations
export these comments, including enum member descriptions, to downstream API
documentation. Python-specific calling conventions belong to Python definitions
or their generation descriptors rather than a separate type-name lookup table.

The `model`, `document`, `authoring`, `runtime` and `binary_wire` module files
include their domain-focused source files in the same module; their shared
imports live in the module files. This preserves Rust type paths and binary wire
signatures when organizing definitions. Rustfmt does not follow `include!`;
when editing these files, format them explicitly, for example:

```sh
rustfmt --edition 2021 --config skip_children=true crates/zipzi-contract/src/{model,document,authoring,runtime,binary_wire}/*.rs
```

Run focused checks with `cargo test --locked -p zipzi-contract` and
`uv run --locked python tools/test_tools.py -p test_python_codegen.py`.

The tooling gate exercises real contract mutations and implementation isolation
in a temporary checkout. For five-sample timing evidence, run
`uv run --locked python tools/python/python_codegen_acceptance.py --output target/python/contract-acceptance`
without competing builds. The output directory must be new; it retains source
copies, Cargo freshness and phase timings. See the
[integration acceptance report](../../docs/archive/plans/python2-contract-acceptance.md).

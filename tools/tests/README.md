# Tooling tests

`test_*.py` tests the development tools in [`tools/`](../README.md): package domains,
test gates, packaging, process lifecycle, benchmark reports, validators and
experiment harnesses. New tooling tests are discovered automatically.

Run from the repository root:

```sh
uv run --locked python tools/test_tools.py
uv run --locked python tools/test_tools.py -p test_cargo_domain.py -v
uv run --locked python tools/test_tools.py -k failure -v
```

The runner accepts unittest discovery options and resolves paths from its own
location, so it also works when invoked by absolute path outside this checkout.
The complete tooling suite needs a POSIX environment, Git, Cargo (metadata only),
and a C compiler for development-process fixtures. It does not compile the SDK
or Editor. Individual platform-independent modules can run on Windows; CI selects
those explicitly. Experiment wheel tests skip unless `SDK_EXPERIMENT_WHEEL` is set.

## Test ownership

| Location | Responsibility |
| --- | --- |
| `tools/tests/` | Regression tests for repository tools |
| `crates/<crate>/src/` | Rust unit tests near their implementation |
| `crates/<crate>/tests/` | Crate-owned integration and native acceptance tests |
| `crates/zipzi-python/tests/` | Tests of the built Python extension |
| `fixtures/` | [Shared test data](../../fixtures/README.md), independent of tooling |
| `models/local/` | Ignored, separately obtained licensed model assets |

See [test gates](../../tools/README.md#test-coverage) for CPU, GPU, wheel, stress
and licensed-model coverage.

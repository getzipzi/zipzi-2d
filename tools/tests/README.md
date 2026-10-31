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
those explicitly.

`test_dev.py` exercises watcher lifecycle controls without requiring `watchexec`.
When it is installed, additional tests use a real event stream with watchexec's
polling backend so they can run on hosts without native filesystem notifications.
Set `ZIPZI_TEST_WATCHEXEC_NATIVE=1` to run those integration cases with the
production native backend instead.

## Test ownership

| Location | Responsibility |
| --- | --- |
| `tools/tests/` | Regression tests for repository tools |
| `crates/<crate>/src/` | Rust unit tests near their implementation |
| `crates/<crate>/tests/` | Crate-owned integration and native acceptance tests |
| `crates/zipzi-python2/tests/` | Maintained Python client tests |
| `fixtures/` | [Shared test data](../../fixtures/README.md), independent of tooling |
| `models/local/` | Ignored, separately obtained licensed model assets |

See [test gates](../../tools/README.md#test-coverage) for CPU, GPU, wheel, stress
and licensed-model coverage.

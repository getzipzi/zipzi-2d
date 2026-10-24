# Documentation

- [Repository layout](../MONOREPO.md) and [licensing](../LICENSING.md)
- [SDK boundaries](../MONOREPO.md#sdk-and-language-boundaries), [Python SDK](../crates/kasane-python/README.md) and [API](../crates/kasane-python/API.md)
- [Editor development](../crates/kasane-editor/README.md) and [dependencies](../vendor/README.md#dependencies)
- [Test domains and CI](../tools/README.md#test-coverage)
- [Source, wheel and app artifacts](../tools/README.md#local-distribution-artifacts)
- [Tool guide](../tools/README.md) and [model inventory](../models/README.md)
- [Runtime preparation](runtime/README.md) and [Runtime plan](plans/sdk-runtime-implementation-plan.md)
- [Editor application](../crates/kasane-editor/README.md), [editing core](../crates/editor-core/README.md) and [render adapter](../crates/editor-render/README.md)
- [Current topic references](references/README.md) and [benchmark guides](../tools/benchmarks/README.md)
- [Tooling tests](../tools/tests/README.md) and [shared fixtures](../fixtures/README.md)

`references/` holds current topic guides. `archive/`, completed implementation/acceptance
plans and `plans/sdk-runtime/r0-*` retain historical commands, measurements and paths.
Historical records may describe pre-merge layouts; do not run old commands verbatim.
Current tool paths are listed by function in [tools/README.md](../tools/README.md#directory-ownership).
Current commands run from the repository root unless a guide explicitly says otherwise.
